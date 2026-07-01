use std::{path::PathBuf, rc::Rc};

use image::RgbaImage;
use imgui_app::{Extras, Fonts, ImguiCursorExt, ImguiExt};
use imgui_app::dear_imgui_rs::{DockBuilder, InputText, InputTextCallbackHandler, InputTextFlags, MouseButton, SelectableFlags, SplitDirection, StyleVar, TableColumnFlags, TableColumnSetup, TableColumnWidth, TableFlags, Ui, WindowFlags};
use ksmap::drawing::DrawContext;
use ksmap::{
    analysis::list_assets,
    definitions::ObjectDefs,
    drawing::{self, alpha_to_trans, DrawOptions, TintStrategy},
    graphics::Graphics,
    partition::{GridPartitioner, IslandsPartitioner, Partition, Partitioner},
    seed::MapSeed,
    synchronization::{SyncOptions, WorldSync},
};
use libks::{ScreenCoord, map_bin, world_ini};
use libks_ini::edit::Ini;
use ksmap::{definitions, screen_map::ScreenMap};
use rustc_hash::FxHashMap;

use crate::{map_widget::{build_map, MapState}, ui_extensions::UiExt};

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
    use_multithreaded_encoder: bool,
    map_state: MapState,
    partition_state: PartitionState,
    drawing_state: DrawingState,
}

pub enum Task {
    ShowLevelList,
}

pub fn build_ui(ui: &Ui, mut ex: Extras, state: &mut State) -> Option<Task> {
    let dockspace_id = ui.dockspace_over_main_viewport();
    
    if state.setup_windows {
        let proportion_left = {
            let width_left = unsafe {
                600.0
                + 2.0 * ui.style().window_padding()[0]
                + 0.5 * ui.style().docking_separator_size()
            };
            let width_avail = ui.main_viewport().size()[0];
            (width_left / width_avail).min(0.5)
        };
        
        let (dock_left, dock_main) = DockBuilder::split_node(dockspace_id, SplitDirection::Left, proportion_left);
        DockBuilder::dock_window("Map", dock_main);
        
        let proportion_top = {
            let height_bottom = unsafe {
                240.0
                + 2.0 * ui.style().window_padding()[1]
                + 0.5 * ui.style().docking_separator_size()
                + 2.0 * ui.style().frame_padding()[1]
                + ui.style().window_border_size()
                + ui.text_line_height()
            };
            let height_avail = ui.main_viewport().size()[1];
            1.0 - f32::min(0.5, height_bottom / height_avail)
        };
        
        let (dock_top_left, dock_bottom_left) = DockBuilder::split_node(dock_left, SplitDirection::Up, proportion_top);
        DockBuilder::dock_window("Export", dock_top_left);
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
                build_map(
                    ui,
                    &mut state.map_state,
                    &state.screen_map,
                    state.partitions.get(state.selected),
                    &state.partition_members
                )
            })
            .unwrap_or(None)
    };
    
    let should_go_to_level_list = ui.window("Export").build(|| {
        build_window_export(ui, &mut ex, state)
    }).unwrap_or_default();
    
    let go_to_partition_index = ui.window("Partitions").build(|| {
        build_window_partitions(ui, &mut ex, state)
    }).unwrap_or_default();
    if let Some(i) = go_to_partition_index
        && let Some(partition) = state.partitions.get(i)
        && let Some(first_screen_pos) = partition.positions().first()
    {
        state.map_state.center = *first_screen_pos;
    }
    
    ui.window("Drawing").build(|| {
        build_window_drawing(ui, &mut ex,
            &mut state.drawing_state,
            &mut state.draw_options,
            &mut state.sync_options,
            &mut state.seed);
    });
    
    ui.window("Preview").build(|| {
        build_window_preview(ui, &mut ex, state, hover_pos);
    });
    
    if should_go_to_level_list {
        Some(Task::ShowLevelList)
    }
    else {
        None
    }
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

fn build_window_partitions(ui: &Ui, ex: &mut Extras, state: &mut State) -> Option<usize> {
    let partition_state = &mut state.partition_state;
    
    let button_width = ui.window_width() * 0.65;
    let button_height = ui.text_line_height() * 2.0;
    if ui.button_with_size("Rebuild partitions", [button_width, button_height]) {
        let max_size = (partition_state.max_width as u64, partition_state.max_height as u64);
        state.partitions = match partition_state.algorithm {
            PartitionAlgorithm::Islands => {
                let gap = partition_state.min_gap as u64 ..= partition_state.max_gap as u64;
                let partitioner = IslandsPartitioner {
                    max_size,
                    gap,
                    force: partition_state.force,
                };
                partitioner.partitions(&state.screen_map)
            }
            PartitionAlgorithm::Grid => {
                let partitioner = GridPartitioner {
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
    
    {
        let mut index = partition_state.algorithm as usize;
        ui.combo_simple_string("Algorithm", &mut index, &["Islands", "Grid"]);
        partition_state.algorithm = match index {
            0 => PartitionAlgorithm::Islands,
            1 => PartitionAlgorithm::Grid,
            _ => PartitionAlgorithm::Islands
        };
    }
    
    let max_width_px = partition_state.max_width * 600;
    ui.drag_int_config("Max width")
        .range(1, i32::MAX)
        .speed(0.1)
        .display_format(format!("%d screens / {max_width_px}px"))
        .build(ui, &mut partition_state.max_width);
    
    let max_height_px = partition_state.max_height * 240;
    ui.drag_int_config("Max height")
        .range(1, i32::MAX)
        .speed(0.1)
        .display_format(format!("%d screens / {max_height_px}px"))
        .build(ui, &mut partition_state.max_height);
    
    {
        let max_bytes = max_width_px as usize * max_height_px as usize * 4;
        let unit = best_unit_for_bytes(max_bytes);
        let mut max_size = convert_bytes_to_unit(max_bytes, unit);
        let _disabled = ui.begin_disabled();
        ui.drag_float_config("Max memory")
            .display_format(format!("%.1f{unit}"))
            .build(ui, &mut max_size);
    }
    
    match partition_state.algorithm {
        PartitionAlgorithm::Islands => build_partition_options_islands(ui, partition_state),
        PartitionAlgorithm::Grid => build_partition_options_grid(ui, partition_state),
    };
    
    ui.new_line();
    build_partition_table(ui, ex.fonts, &state.partitions, &mut state.selected)
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
    ui.checkbox("Force gap size", &mut state.force);
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
    ui.move_cursor_right(ui.calc_text_width("Columns") - ui.calc_text_width("Rows"));
    ui.checkbox_small("Auto##Auto rows", &mut state.auto_rows);
    
    {
        let _disabled = ui.begin_disabled_with_cond(state.auto_cols);
        ui.drag_int_config("Columns")
            .range(state.min_gap, i32::MAX)
            .speed(0.05)
            .build(ui, &mut state.cols);
    }
    ui.same_line();
    ui.checkbox_small("Auto##Auto cols", &mut state.auto_cols);
    
    ui.checkbox("Force rows and columns", &mut state.force);
}

fn build_partition_table(ui: &Ui, fonts: &Fonts, partitions: &[Partition], selected: &mut usize) -> Option<usize> {
    let mut go_to_partition_index: Option<usize> = None;
    
    let columns = [
        "Xmin",
        "Ymin",
        "Xmax",
        "Ymax",
        "Width",
        "Height",
        "Width (px)",
        "Height (px)",
        "Memory",
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
            let memory_bytes = (width_px * height_px * 4) as usize;
            
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
            if ui.is_item_clicked() && ui.is_mouse_double_clicked(MouseButton::Left) {
                go_to_partition_index = Some(i);
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
            ui.text_aligned_right(&bytes_to_string(memory_bytes, 1));
        }
    });
    
    go_to_partition_index
}

fn build_window_preview(ui: &Ui, ex: &mut Extras, state: &mut State, hover_pos: Option<ScreenCoord>) { 
    let Some(pos) = hover_pos else {
        ui.text("Mouse over the map to preview a screen");
        return;
    };
    
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

struct DrawingState {
    min_alpha: i32,
    min_alpha_threshold: i32,
    alpha_sim_frames: i32,
}

impl Default for DrawingState {
    fn default() -> Self {
        Self {
            min_alpha: 12,
            min_alpha_threshold: 5,
            alpha_sim_frames: 150,
        }
    }
}

fn build_window_drawing(
    ui: &Ui,
    _ex: &mut Extras,
    state: &mut DrawingState,
    draw_options: &mut DrawOptions,
    sync_options: &mut SyncOptions,
    seed: &mut MapSeed
) {
    let mut seed_buffer = seed.to_string();
    if InputText::new(ui, "Seed", &mut seed_buffer)
        .flags(InputTextFlags::CHARS_HEXADECIMAL | InputTextFlags::CALLBACK_EDIT)
        .callback(MapSeedEditCallback(16))
        .build()
    {
        if let Ok(new_seed) = MapSeed::try_from(seed_buffer) {
            *seed = new_seed;
        }
    }
    
    ui.same_line();
    if ui.small_button("Random") {
        *seed = MapSeed::random();
    }
    
    let mut lasers_index = match (draw_options.ignore_laser_phase, sync_options.maximize_visible_lasers) {
        (false, true) => 0,
        (false, false) => 1,
        (true, _) => 2
    };
    if ui.combo_simple_string("Lasers", &mut lasers_index, &[
        "Maximize",
        "Randomize",
        "All"
    ]) {
        match lasers_index {
            0 => {
                draw_options.ignore_laser_phase = false;
                sync_options.maximize_visible_lasers = true;
            }
            1 => {
                draw_options.ignore_laser_phase = false;
                sync_options.maximize_visible_lasers = false;
            }
            2 => {
                draw_options.ignore_laser_phase = true;
            }
            _ => {}
        }
    }
    
    let mut tint_index = match draw_options.tint_strategy {
        TintStrategy::Ignore => 0,
        TintStrategy::Explicit => 1
    };
    if ui.combo_simple_string("Tints", &mut tint_index, &[
        "Ignore",
        "Explicit"
    ]) {
        match tint_index {
            0 => draw_options.tint_strategy = TintStrategy::Ignore,
            1 => draw_options.tint_strategy = TintStrategy::Explicit,
            _ => {}
        }
    }
    
    if ui.drag_int_config("Min alpha")
        .range(0, 255)
        .speed(0.1)
        .build(ui, &mut state.min_alpha)
    {
        draw_options.trans_max_override = alpha_to_trans(state.min_alpha as u8);
    }
    
    if ui.drag_int_config("Min alpha threshold")
        .range(0, i32::MAX)
        .speed(0.1)
        .build(ui, &mut state.min_alpha_threshold)
    {
        draw_options.trans_max_threshold = state.min_alpha_threshold as u32;
    }
    
    let alpha_sim_secs = state.alpha_sim_frames as f32 / 50.0;
    if ui.drag_int_config("Alpha sim frames")
        .range(0, i32::MAX)
        .display_format(format!("%d / {alpha_sim_secs:.1}s"))
        .build(ui, &mut state.alpha_sim_frames)
    {
        draw_options.trans_frames = state.alpha_sim_frames as u32;
    }
    
    ui.checkbox("Show invisible objects", &mut draw_options.show_invisible);
    ui.checkbox("Show proximity-sensitive objects", &mut draw_options.show_proximity);
}

struct MapSeedEditCallback(usize);

impl InputTextCallbackHandler for MapSeedEditCallback {
    fn on_edit(&mut self, mut data: imgui_app::dear_imgui_rs::TextCallbackData<'_>) {
        let excess = data.str().len().saturating_sub(self.0);
        if excess > 0 {
            data.remove_chars(self.0, excess);
        }
    }
}

fn build_window_export(ui: &Ui, _ex: &mut Extras, state: &mut State) -> bool {
    let button_width = ui.window_width() * 0.65;
    let button_height = ui.text_line_height() * 2.0;
    if ui.button_with_size("Export", [button_width, button_height]) {
        let world_sync = state.world_sync.get_or_insert_with(|| {
            WorldSync::new(
                state.seed,
                &state.screen_map,
                &state.object_defs,
                &state.sync_options
            )
        });
        let draw_context = DrawContext {
            seed: state.seed,
            screens: &state.screen_map,
            gfx: &state.gfx,
            defs: &state.object_defs,
            ini: &state.ini,
            world_sync: &world_sync,
            options: state.draw_options.clone(),
        };
        
        let output_dir =
            if state.partitions.len() > 1 {
                let dir = PathBuf::from(state.level_dir.file_name().unwrap());
                std::fs::create_dir_all(&dir).unwrap();
                dir
            }
            else {
                PathBuf::from(".")
            };
        
        for partition in &state.partitions {
            let bounds = partition.bounds();
            let canvas = drawing::draw_partition(draw_context, partition).unwrap();
            
            let file_name = format!("{bounds}.png");
            let path = output_dir.join(file_name);
            
            if state.use_multithreaded_encoder {
                drawing::export_canvas_multithreaded(canvas, path.as_ref()).unwrap();
            }
            else {
                drawing::export_canvas(canvas, path.as_ref()).unwrap();
            }
        }
    }
    
    ui.checkbox("Multithreaded encoding", &mut state.use_multithreaded_encoder);
    
    ui.new_line();
    if ui.small_button("Open another level") {
        true
    }
    else {
        false
    }
}

#[derive(Clone, Copy)]
enum BytesUnit {
    B,
    KB,
    MB,
    GB,
    TB,
}

const KB_SIZE: usize = 1024;
const MB_SIZE: usize = KB_SIZE * 1024;
const GB_SIZE: usize = MB_SIZE * 1024;
const TB_SIZE: usize = GB_SIZE * 1024;

impl BytesUnit {
    fn to_bytes(&self) -> usize {
        match self {
            Self::B => 1,
            Self::KB => KB_SIZE,
            Self::MB => MB_SIZE,
            Self::GB => GB_SIZE,
            Self::TB => TB_SIZE,
        }
    }
}

impl std::fmt::Display for BytesUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::B => f.write_str("B"),
            Self::KB => f.write_str("KB"),
            Self::MB => f.write_str("MB"),
            Self::GB => f.write_str("GB"),
            Self::TB => f.write_str("TB"),
        }
    }
}

fn best_unit_for_bytes(bytes: usize) -> BytesUnit {
    match bytes {
        0..KB_SIZE => BytesUnit::B,
        KB_SIZE..MB_SIZE =>BytesUnit::KB,
        MB_SIZE..GB_SIZE => BytesUnit::MB,
        GB_SIZE..TB_SIZE => BytesUnit::GB,
        _ => BytesUnit::TB,
    }
}

fn convert_bytes_to_unit(bytes: usize, unit: BytesUnit) -> f32 {
    bytes as f32/ unit.to_bytes() as f32
}

fn bytes_to_string(bytes: usize, precision: usize) -> String {
    let unit = best_unit_for_bytes(bytes);
    match unit {
        BytesUnit::B => format!("{bytes}{unit}"),
        _ => {
            let value = convert_bytes_to_unit(bytes, unit);
            format!("{value:.prec$}{unit}", prec = precision)
        }
    }
}

impl State {
    pub fn new(level_dir: PathBuf) -> Self {    
        let screens = map_bin::parse_map_file(level_dir.join("Map.bin")).unwrap();
        let screen_map = ScreenMap::new(screens);
        let ini = world_ini::load_ini_from_dir(&level_dir).unwrap();
        
        let object_defs_path = {
            let mut current_dir = std::env::current_exe()
                .unwrap_or_else(|_| PathBuf::new());
            current_dir.set_file_name("ksmap_data/object_definitions.toml");
            current_dir
        };
        let object_defs = {
            let mut defs = definitions::load_object_defs(object_defs_path).unwrap();
            definitions::insert_custom_obj_defs(&mut defs, &ini);
            Rc::new(defs)
        };
        
        let data_dir = level_dir.join("../../Data");
        let templates_dir = {
            let mut current_dir = std::env::current_exe()
                .unwrap_or_else(|_| PathBuf::new());
            current_dir.set_file_name("ksmap_data/templates");
            current_dir
        };
        let mut gfx = Graphics::new(data_dir, level_dir.clone(), templates_dir, Rc::clone(&object_defs));
        
        let assets = list_assets(screen_map.as_slice(), &object_defs);
        let mut warnings = Vec::new();
        gfx.load_tilesets(&assets.tilesets, &mut warnings).unwrap();
        gfx.load_gradients(&assets.gradients, &mut warnings).unwrap();
        gfx.load_objects(&assets.objects, &mut warnings).unwrap();
        
        let partitioner = IslandsPartitioner {
            max_size: (120, 300),
            gap: 1..=10,
            force: false,
        };
        let partitions = partitioner.partitions(&screen_map);
        let mut partition_members = FxHashMap::default();
        for (i, positions) in partitions.iter().enumerate() {
            for pos in positions {
                partition_members.insert(*pos, i);
            }
        }
        
        let seed = MapSeed::random();
        let first_screen_pos = screen_map.first()
            .map(|screen| screen.position)
            .unwrap_or((1000, 1000));
        
        State {
            level_dir,
            ini,
            object_defs,
            gfx,
            screen_map,
            seed,
            partitions,
            partition_members,
            world_sync: None,
            draw_options: DrawOptions::default(),
            sync_options: SyncOptions::default(),
            selected: 0,
            setup_windows: true,
            preview: None,
            use_multithreaded_encoder: true,
            map_state: MapState::new(first_screen_pos),
            partition_state: PartitionState::default(),
            drawing_state: DrawingState::default(),
        }
    }
}
