use std::{path::PathBuf, rc::Rc};

use image::RgbaImage;
use imgui_app::{Extras, Fonts, ImguiExt};
use imgui_app::dear_imgui_rs::{DockBuilder, MouseButton, SelectableFlags, SplitDirection, StyleColor, StyleVar, TableColumnFlags, TableColumnSetup, TableColumnWidth, TableFlags, Ui, WindowFlags};
use ksmap::partition;
use ksmap::{
    analysis::list_assets,
    definitions::ObjectDefs,
    drawing::DrawOptions,
    graphics::Graphics,
    partition::{Partition, Partitioner},
    seed::MapSeed,
    synchronization::{SyncOptions, WorldSync},
};
use libks::{ScreenCoord, map_bin, world_ini};
use libks_ini::edit::Ini;
use ksmap::{definitions, screen_map::ScreenMap};
use rustc_hash::FxHashMap;

use crate::ui_extensions::UiExt;

pub struct State {
    #[allow(dead_code)]
    level_dir: PathBuf,
    ini: Ini,
    object_defs: Rc<ObjectDefs>,
    gfx: Graphics,
    screen_map: ScreenMap,
    seed: MapSeed,
    partitions: Vec<Partition>,
    partition_members: FxHashMap<ScreenCoord, usize>,
    world_sync: Option<WorldSync>,
    draw_options: DrawOptions,
    sync_options: SyncOptions,
    selected: usize,
    setup_windows: bool,
    preview: Option<(ScreenCoord, u64)>,
    map_state: MapState,
    partition_state: PartitionState,
}

pub fn build_ui(ui: &Ui, mut ex: Extras, state: &mut State) {
    let dockspace_id = ui.dockspace_over_main_viewport();
    
    if state.setup_windows {
        let width_left = unsafe {
            600.0
            + 2.0 * ui.style().window_padding()[0]
            + 0.5 * ui.style().docking_separator_size()
        };
        let width_avail = ui.main_viewport().size()[0];
        let proportion_left = (width_left / width_avail).min(0.5);
        
        let (dock_left, dock_main) = DockBuilder::split_node(dockspace_id, SplitDirection::Left, proportion_left);
        DockBuilder::dock_window("Map", dock_main);
        
        let (dock_top_left, dock_bottom_left) = DockBuilder::split_node(dock_left, SplitDirection::Up, 0.5);
        DockBuilder::dock_window("Partitions", dock_top_left);
        DockBuilder::dock_window("Drawing", dock_top_left);
        DockBuilder::dock_window("Preview", dock_bottom_left);
        
        DockBuilder::finish(dockspace_id);
        state.setup_windows = false;
    }
    
    let hover_pos = {
        let _map_padding = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0])); 
        ui.window("Map")
            .flags(WindowFlags::NO_MOVE)
            .build(|| {
                build_window_map(
                    ui,
                    &mut state.map_state,
                    &state.screen_map,
                    state.partitions.get(state.selected),
                    &state.partition_members
                )
            })
            .unwrap_or(None)
    };
    
    ui.window("Partitions").build(|| {
        build_window_partitions(ui, &mut ex, state);
    });
    
    ui.window("Drawing").build(|| {
        build_window_drawing(ui, &mut ex, state);
    });
    
    ui.window("Preview").build(|| {
        build_window_preview(ui, &mut ex, state, hover_pos);
    });
}

struct PartitionState {
    algorithm: PartitionAlgorithm,
    max_width: i32,
    max_height: i32,
    min_gap: i32,
    max_gap: i32,
    auto_rows: bool,
    auto_cols: bool,
    rows: i32,
    cols: i32,
    force: bool,
}

impl Default for PartitionState {
    fn default() -> Self {
        Self {
            algorithm: PartitionAlgorithm::default(),
            max_width: 120,
            max_height: 300,
            min_gap: 1,
            max_gap: 10,
            auto_rows: true,
            auto_cols: true,
            rows: 10,
            cols: 10,
            force: false,
        }
    }
}

#[derive(Clone, Copy, Default)]
enum PartitionAlgorithm {
    #[default]
    Islands,
    Grid,
}

fn build_window_partitions(ui: &Ui, ex: &mut Extras, state: &mut State) {
    const GB: f32 = 1073741824.0;
    let partition_state = &mut state.partition_state;
    
    {
        let mut index = partition_state.algorithm as usize;
        ui.combo_simple_string("Algorithm", &mut index, &["Islands", "Grid"]);
        partition_state.algorithm = match index {
            0 => PartitionAlgorithm::Islands,
            1 => PartitionAlgorithm::Grid,
            _ => PartitionAlgorithm::Islands
        };
    }
    
    ui.drag_int_config("Max width")
        .range(1, i32::MAX)
        .speed(0.1)
        .build(ui, &mut partition_state.max_width);
    let max_width_px = partition_state.max_width * 600;
    {
        let _color = ui.push_style_color(StyleColor::Text, ui.style_color(StyleColor::TextDisabled));
        ui.same_line();
        ui.text(format!("{max_width_px}px"));
    }
    
    ui.drag_int_config("Max height")
        .range(1, i32::MAX)
        .speed(0.1)
        .build(ui, &mut partition_state.max_height);
    let max_height_px = partition_state.max_height * 240;
    {
        let _color = ui.push_style_color(StyleColor::Text, ui.style_color(StyleColor::TextDisabled));
        ui.same_line();
        ui.text(format!("{max_height_px}px"));
    }
    
    let mut max_memory_gb = max_width_px as f32 * max_height_px as f32 * 4.0 / GB;
    {
        let _disabled = ui.begin_disabled();
        ui.drag_float("Max memory (GB)", &mut max_memory_gb);
    }
    
    match partition_state.algorithm {
        PartitionAlgorithm::Islands => build_partition_options_islands(ui, partition_state),
        PartitionAlgorithm::Grid => build_partition_options_grid(ui, partition_state),
    };
    
    if ui.button("Rebuild partitions") {
        let max_size = (partition_state.max_width as u64, partition_state.max_height as u64);
        state.partitions = match partition_state.algorithm {
            PartitionAlgorithm::Islands => {
                let gap = partition_state.min_gap as u64 ..= partition_state.max_gap as u64;
                let partitioner = ksmap::partition::IslandsPartitioner {
                    max_size,
                    gap,
                    force: partition_state.force,
                };
                partitioner.partitions(&state.screen_map)
            }
            PartitionAlgorithm::Grid => {
                let partitioner = ksmap::partition::GridPartitioner {
                    max_size,
                    rows: if partition_state.auto_rows { None } else { Some(partition_state.rows as u64) },
                    cols: if partition_state.auto_cols { None } else { Some(partition_state.cols as u64) },
                    force: partition_state.force,
                };
                partitioner.partitions(&state.screen_map)
            }
        };
        
        state.partition_members.clear();
        for (i, positions) in state.partitions.iter().enumerate() {
            for pos in positions {
                state.partition_members.insert(*pos, i);
            }
        }
    }
    
    build_partition_table(ui, ex.fonts, &state.partitions, &mut state.selected);
}

fn build_partition_options_islands(ui: &Ui, state: &mut PartitionState) {
    ui.drag_int_config("Min gap")
        .range(1, i32::MAX)
        .speed(0.05)
        .build(ui, &mut state.min_gap);
    state.max_gap = state.max_gap.max(state.min_gap);
    ui.drag_int_config("Max gap")
        .range(state.min_gap, i32::MAX)
        .speed(0.05)
        .build(ui, &mut state.max_gap);
    ui.checkbox("Enforce gap size", &mut state.force);
}

fn build_partition_options_grid(ui: &Ui, state: &mut PartitionState) {
    {
        let _disabled = ui.begin_disabled_with_cond(state.auto_rows);
        ui.drag_int_config("Rows")
            .range(1, i32::MAX)
            .speed(0.05)
            .build(ui, &mut state.rows);
    }
    ui.same_line();
    ui.checkbox("Auto##Auto rows", &mut state.auto_rows);
    
    {
        let _disabled = ui.begin_disabled_with_cond(state.auto_cols);
        ui.drag_int_config("Columns")
            .range(state.min_gap, i32::MAX)
            .speed(0.05)
            .build(ui, &mut state.cols);
    }
    ui.same_line();
    ui.checkbox("Auto##Auto cols", &mut state.auto_cols);
    
    ui.checkbox("Enforce rows and columns", &mut state.force);
}

fn build_partition_table(ui: &Ui, fonts: &Fonts, partitions: &[Partition], selected: &mut usize) {
    let columns = [
        "Xmin",
        "Ymin",
        "Xmax",
        "Ymax",
        "Width",
        "Height",
        "Width (px)",
        "Height (px)",
        "Memory (MB)",
    ];
    let mut table_builder = ui.table("##RageTable")
        .flags(TableFlags::BORDERS | TableFlags::NO_HOST_EXTEND_X);
    
    for column in columns {
        table_builder = table_builder.add_column(TableColumnSetup {
            name: column,
            flags: TableColumnFlags::NONE,
            width: Some(TableColumnWidth::Fixed(0.0)),
            indent: None,
            user_id: None,
        });
    }
    
    table_builder.build(|ui| {
        ui.table_headers_row();
        
        let _font = ui.push_font(fonts.mono);
        
        for (i, partition) in partitions.iter().enumerate() {
            let bounds = partition.bounds();
            let x_min = bounds.x.start;
            let x_max = bounds.x.end - 1;
            let y_min = bounds.y.start;
            let y_max = bounds.y.end - 1;
            let width = x_max - x_min + 1;
            let height = y_max - y_min + 1;
            let width_px = width * 600;
            let height_px = height * 240;
            let memory_bytes = width_px * height_px * 4;
            let memory_mb = memory_bytes as f64 / (2.0f64).powi(20);
            
            ui.table_next_row();
            ui.table_next_column();
            let id = ui.push_id(i);
            let x_min_str = x_min.to_string();
            ui.align_next_item_right(ui.calc_text_size(&x_min_str)[0]);
            if ui.selectable_config(x_min_str)
                .selected(*selected == i)
                .flags(SelectableFlags::SPAN_ALL_COLUMNS)
                .build()
            {
                *selected = i;
            }
            drop(id);
            
            ui.table_next_column();
            ui.text_aligned_right(y_min.to_string());
            ui.table_next_column();
            ui.text_aligned_right(x_max.to_string());
            ui.table_next_column();
            ui.text_aligned_right(y_max.to_string());
            ui.table_next_column();
            ui.text_aligned_right(width.to_string());
            ui.table_next_column();
            ui.text_aligned_right(height.to_string());
            ui.table_next_column();
            ui.text_aligned_right(width_px.to_string());
            ui.table_next_column();
            ui.text_aligned_right(height_px.to_string());
            ui.table_next_column();
            ui.text_aligned_right(format!("{memory_mb:.1}"));
        }
    });
}

struct MapState {
    center: ScreenCoord,
    drag_origin: Option<ScreenCoord>,
}

impl Default for MapState {
    fn default() -> Self {
        Self {
            center: (1000, 1000),
            drag_origin: None,
        }
    }
}

fn build_window_map(
    ui: &Ui,
    map_state: &mut MapState,
    screens: &ScreenMap,
    selected_partition: Option<&Partition>,
    partition_members: &FxHashMap<ScreenCoord, usize>
) -> Option<ScreenCoord> {
    const COLORS: &'static [[f32; 4]] = &[
        [0.5, 0.0, 0.0, 1.0],
        [0.5, 0.5, 0.0, 1.0],
        [0.0, 0.5, 0.0, 1.0],
        [0.0, 0.5, 0.5, 1.0],
        [0.0, 0.0, 0.5, 1.0],
        [0.5, 0.0, 0.5, 1.0],
    ];
    
    let cell_width: f32 = 10.0;
    let cell_height: f32 = 10.0;

    let [width_avail, height_avail] = ui.get_content_region_avail();
    let cols = (width_avail / (cell_width + 1.0)).ceil() as i32;
    let rows = (height_avail / (cell_height + 1.0)).ceil() as i32;
    
    // Pan
    if ui.is_mouse_released(MouseButton::Right) {
        map_state.drag_origin = None;
    }
    else if ui.is_mouse_clicked(MouseButton::Right)
        && ui.is_window_hovered() 
    {
        map_state.drag_origin = Some(map_state.center);
    }
    if let Some((origin_x, origin_y)) = &map_state.drag_origin {
        let [dx, dy] = ui.mouse_drag_delta(MouseButton::Right);
        let new_x = origin_x - (dx / (cell_width + 1.0)) as i32;
        let new_y = origin_y - (dy / (cell_height + 1.0)) as i32;
        map_state.center = (new_x, new_y);
    }
    
    let (x_center, y_center) = map_state.center;
    let x_min = x_center - cols / 2;
    let y_min = y_center - rows / 2;
    let x_max = x_min + cols;
    let y_max = y_min + rows;
    
    let draw_list = ui.get_window_draw_list();
    let [origin_x, origin_y] = ui.get_cursor_screen_pos();
    
    for x_grid in 1..=cols {
        let x_screen = (origin_x - 1.0) + (x_grid as f32) * (cell_width + 1.0);
        draw_list.add_line_v(x_screen, origin_y, origin_y + height_avail, [0.1, 0.1, 0.1], 1.0);
    }
    for y_grid in 1..=rows {
        let y_screen = (origin_y - 1.0) + (y_grid as f32) * (cell_height + 1.0);
        draw_list.add_line_h(origin_x, origin_x + width_avail, y_screen, [0.1, 0.1, 0.1], 1.0);
    }
    
    for y in y_min..y_max {
        for x in x_min..x_max {
            if screens.index_of(&(x, y)).is_none() {
                continue;
            }
            
            let dx = x - x_min;
            let dy = y - y_min;
            
            let top_left = [
                origin_x + (dx as f32) * (cell_width + 1.0),
                origin_y + (dy as f32) * (cell_height + 1.0),
            ];
            let bottom_right = [
                top_left[0] + cell_width,
                top_left[1] + cell_height
            ];
            
            let partition_index = partition_members.get(&(x, y)).unwrap();
            let color = COLORS[*partition_index % COLORS.len()];
            
            draw_list.add_rect(top_left, bottom_right, color)
                .filled(true)
                .build();
            draw_list.add_rect(top_left, bottom_right, [1.0, 1.0, 1.0, 0.1])
                .filled(false)
                .build();
        }
    }
    
    if let Some(partition) = selected_partition {
        let bounds = partition.bounds();
        // if bounds.x.is_empty() || bounds.y.is_empty() { return; }
        let top_left_dx = bounds.x.start as i32 - x_min;
        let top_left_dy = bounds.y.start as i32 - y_min;
        let bottom_right_dx = bounds.x.end as i32 - x_min;
        let bottom_right_dy = bounds.y.end as i32 - y_min;
        let top_left = [
            origin_x + top_left_dx as f32 * (cell_width + 1.0) - 1.0,
            origin_y + top_left_dy as f32 * (cell_height + 1.0) - 1.0,
        ];
        let bottom_right = [
            origin_x + bottom_right_dx as f32 * (cell_width + 1.0),
            origin_y + bottom_right_dy as f32 * (cell_height + 1.0),
        ];
        draw_list.add_rect(top_left, bottom_right, [1.0, 1.0, 1.0, 1.0])
            .filled(false)
            .build();
    }
    
    if !ui.is_window_hovered() {
        return None;
    }
    
    let [mouse_x, mouse_y] = ui.mouse_pos();
    let dx = mouse_x - origin_x;
    let dy = mouse_y - origin_y;
    let grid_x = (dx / (cell_width + 1.0)) as i32;
    let grid_y = (dy / (cell_height + 1.0)) as i32;
    let screen_x = x_min + grid_x;
    let screen_y = y_min + grid_y;
    
    let hover_top_left = [
        origin_x + grid_x as f32 * (cell_width + 1.0),
        origin_y + grid_y as f32 * (cell_height + 1.0),
    ];
    let hover_bottom_right = [
        hover_top_left[0] + cell_width,
        hover_top_left[1] + cell_height
    ];
    draw_list.add_rect(hover_top_left, hover_bottom_right, [1.0, 1.0, 1.0, 1.0])
        .filled(false)
        .build();
    
    Some((screen_x, screen_y))
}

fn build_window_preview(ui: &Ui, ex: &mut Extras, state: &mut State, hover_pos: Option<ScreenCoord>) { 
    let Some(pos) = hover_pos else { return };
    
    let pos_changed = state.preview.as_ref().is_none_or(|preview| {
        preview.0 != pos
    });
    
    if pos_changed {
        if let Some(preview) = state.preview.take() {
            ex.textures.delete_texture(preview.1);
        }
        
        state.preview = match draw_single_screen(state, pos) {
            Some(image) => {
                let id = ex.textures.create_texture_from_bytes(image.width(), image.height(), &image);
                Some((pos, id))
            }
            None => None
        };
    }
    
    if let Some(preview) = &state.preview
        && let Some(texture) = ex.textures.get_texture(preview.1)
    {
        ui.image(texture, texture.size());
    }
}

fn draw_single_screen(state: &mut State, screen_pos: ScreenCoord) -> Option<RgbaImage> {
    let world_sync = state.world_sync.get_or_insert_with(|| {
        WorldSync::new(
            state.seed,
            &state.screen_map,
            &state.object_defs,
            &state.sync_options
        )
    });
    let screen_index = state.screen_map.index_of(&screen_pos)?;
    let screen = &state.screen_map[screen_index];
    
    ksmap::drawing::draw_screen(
        state.seed,
        screen,
        screen_index,
        &state.gfx,
        &state.object_defs,
        &state.ini,
        state.draw_options,
        world_sync
    ).ok()
}

fn build_window_drawing(ui: &Ui, _ex: &mut Extras, _state: &mut State) {
    ui.text("Drawing");
}

impl State {
    pub fn new(level_dir: PathBuf) -> Self {    
        let screens = map_bin::parse_map_file(level_dir.join("Map.bin")).unwrap();
        let screen_map = ScreenMap::new(screens);
        let ini = world_ini::load_ini_from_dir(&level_dir).unwrap();
        
        let object_defs_path = "ksmap_data/object_definitions.toml";
        let object_defs = {
            let mut defs = definitions::load_object_defs(object_defs_path).unwrap();
            definitions::insert_custom_obj_defs(&mut defs, &ini);
            Rc::new(defs)
        };
        
        let data_dir = level_dir.join("../../Data");
        let templates_dir = "ksmap_data/templates";
        let mut gfx = Graphics::new(data_dir, level_dir.clone(), templates_dir, Rc::clone(&object_defs));
        
        let assets = list_assets(screen_map.as_slice(), &object_defs);
        let mut warnings = Vec::new();
        gfx.load_tilesets(&assets.tilesets, &mut warnings).unwrap();
        gfx.load_gradients(&assets.gradients, &mut warnings).unwrap();
        gfx.load_objects(&assets.objects, &mut warnings).unwrap();
        
        use ksmap::partition::Partitioner;
        let partitioner = ksmap::partition::IslandsPartitioner {
            max_size: (40, 25),
            gap: 1..=1,
            force: true,
        };
        // let partitioner = ksmap::partition::GridPartitioner {
        //     max_size: (8, 8),
        //     rows: None,
        //     cols: None,
        //     force: false,
        // };
        let partitions = partitioner.partitions(&screen_map);
        let mut partition_members = FxHashMap::default();
        for (i, positions) in partitions.iter().enumerate() {
            for pos in positions {
                partition_members.insert(*pos, i);
            }
        }
        
        State {
            level_dir,
            ini,
            object_defs,
            gfx,
            screen_map,
            seed: MapSeed::random(),
            partitions,
            partition_members,
            world_sync: None,
            draw_options: DrawOptions::default(),
            sync_options: SyncOptions::default(),
            selected: 0,
            setup_windows: true,
            preview: None,
            map_state: MapState::default(),
            partition_state: PartitionState::default(),
        }
    }
}
