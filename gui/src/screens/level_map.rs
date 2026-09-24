use std::cmp;
use std::thread::JoinHandle;
use std::{path::{Path, PathBuf}, sync::{Arc, atomic::{self, AtomicBool}, mpsc::{self, TryRecvError}, RwLock}};
use image::RgbaImage;
use imgui_app::{Extras, Fonts, Textures};
use imgui_app::dear_imgui_rs::{Condition, DockLayout, DockLayoutApply, DockSplit, InputText, InputTextCallbackHandler, InputTextFlags, Key, MouseButton, SelectableFlags, SortDirection, StyleColor, StyleVar, TableColumnFlags, TableColumnSetup, TableColumnUserData, TableColumnWidth, TableFlags, TextureId, Ui, WindowFlags, WindowKey};
use ksmap::drawing::DrawContext;
use ksmap::{
    definitions::ObjectDefs,
    drawing::{self, alpha_to_trans, DrawOptions, TintStrategy},
    graphics::Graphics,
    partition::{GridPartitioner, IslandsPartitioner, Partition, Partitioner},
    seed::MapSeed,
    synchronization::{SyncOptions, WorldSync},
};
use libks::ScreenCoord;
use libks_ini::edit::Ini;
use ksmap::screen_map::ScreenMap;
use rustc_hash::FxHashMap;

use crate::map_widget::MAP_COLORS;
use crate::name_pattern::{self, NamePattern};
use crate::tooltips::{set_tooltips_enabled, toggle_tooltips, tooltip, tooltips_are_enabled};
use crate::{map_widget::{build_map, MapState, map_get_center_screen}, ui_extensions::UiExt};
use crate::format_bytes::*;

pub struct State {
    can_return_to_level_list: bool,
    layout: Option<DockLayout>,
    reset_layout: bool,
    render_thread: Option<JoinHandle<()>>,
    render_rx: Option<mpsc::Receiver<RenderMessage>>,
    render_state_lock: RenderStateLock,
    render_progress: RenderProgress,
    render_cancel: Arc<AtomicBool>,
    render_error: Option<String>,
    export_state: ExportState,
    map_state: MapState,
    partition_state: PartitionState,
    drawing_state: DrawingState,
    preview_state: PreviewState,
    generate_mipmaps: bool,
}

impl State {
    pub fn new(
        level_dir: PathBuf,
        render_state: RenderState,
        partition_state: PartitionState,
        can_return_to_level_list: bool
    ) -> Self {
        State {
            can_return_to_level_list,
            layout: None,
            reset_layout: false,
            render_state_lock: Arc::new(RwLock::new(render_state)),
            render_rx: None,
            render_progress: RenderProgress::default(),
            render_cancel: Arc::new(AtomicBool::new(false)),
            render_error: None,
            map_state: MapState::default(),
            partition_state,
            drawing_state: DrawingState::default(),
            preview_state: PreviewState::default(),
            export_state: ExportState::new(level_dir),
            render_thread: None,
            generate_mipmaps: false,
        }
    }
}

pub enum Task {
    ShowLevelList,
    Exit,
}

pub fn build_ui(ui: &Ui, ex: &mut Extras, state: &mut State) -> Option<Task> {
    // Show progress window when rendering
    if state.render_thread.is_some() || state.render_error.is_some() {
        let (width, height) = ex.window.size();
        ui.window("Main")
            .position([0.0, 0.0], Condition::Always)
            .size([width as f32, height as f32], Condition::Always)
            .flags(WindowFlags::NO_TITLE_BAR | WindowFlags::NO_MOVE | WindowFlags::NO_RESIZE)
        .build(|| {
            build_window_progress(ui, ex, state);
        });
        return None;
    }
    
    let State {
        can_return_to_level_list,
        layout,
        reset_layout,
        render_thread,
        render_rx,
        render_state_lock,
        render_progress,
        render_cancel,
        render_error: _render_error,
        export_state,
        map_state,
        partition_state,
        drawing_state,
        preview_state,
        generate_mipmaps,
    } = state;
    
    // Initialize dockspace
    {
        let menu_bar_height = ui.text_line_height() + 2.0 * unsafe { ui.style().frame_padding()[1] };
        let (width, height) = ex.window.size();
        let window_padding = unsafe { ui.style().window_padding() };
        let _token = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));
        ui.window("Main")
            .position([0.0, menu_bar_height], Condition::Always)
            .size([width as f32, height as f32 - menu_bar_height], Condition::Always)
            .flags(WindowFlags::NO_TITLE_BAR | WindowFlags::NO_MOVE | WindowFlags::NO_RESIZE | WindowFlags::NO_BRING_TO_FRONT_ON_FOCUS)
        .build(|| {
            let _token = ui.push_style_var(StyleVar::WindowPadding(window_padding));
            if *reset_layout {
                layout.replace(create_dockspace_layout(ui));
            }
            let layout = layout.get_or_insert_with(|| create_dockspace_layout(ui));
            let dockspace_id = ui.get_id("Dockspace");
            ui.dockspace()
                .layout(layout, if *reset_layout { DockLayoutApply::Replace } else { DockLayoutApply::IfMissing })
                .root_id(dockspace_id)
                .current_window([width as f32, height as f32 - menu_bar_height])
                .build()
                .expect("Invalid dockspace layout");
            *reset_layout = false;
        });
    }
    
    let Ok(mut render_state) = render_state_lock.write() else {
        return Some(Task::ShowLevelList);
    };
    
    let mut requested_center: Option<(i64, i64)> = None;
    let mut show_level_directory = false;
    let mut show_output_directory = false;
    let mut show_level_list = false;
    let mut copy_screen_pos = false;
    let mut open_popup_controls = false;
    let mut open_popup_map_textures = false;
    let mut enable_map_textures = false;
    
    // Main menu
    if let Some(_menu_bar) = ui.begin_main_menu_bar() {
        if let Some(_file_menu) = ui.begin_menu("File") {
            if ui.menu_item_with_shortcut("Show output directory", "Ctrl+O") {
                show_output_directory = true;
            }
            if ui.menu_item_with_shortcut("Show level directory", "Ctrl+F") {
                show_level_directory = true;
            }
            {
                let _token = ui.begin_disabled_with_cond(!*can_return_to_level_list);
                if ui.menu_item_with_shortcut("Return to level list", "F2") {
                    show_level_list = true;
                }
            }
            ui.separator();
            if ui.menu_item("Exit") {
                return Some(Task::Exit);
            }
        }
        ui.menu("Edit", || {
            if ui.menu_item_with_shortcut("Copy screen coordinates", "Ctrl+C") {
                copy_screen_pos = true;
            }
        });
        ui.menu("View", || {
            let mut true_aspect_ratio = map_state.opts.aspect_ratio == 2.5;
            ui.menu_item_toggle("Use true aspect ratio", None::<&str>, &mut true_aspect_ratio, !map_state.opts.use_textures);
            if ui.is_item_edited() {
                map_state.opts.aspect_ratio = if true_aspect_ratio { 2.5 } else { 1.0 };
                if let Some(geom) = &map_state.prev_geom {
                    requested_center = Some(map_get_center_screen(geom));
                }
            }
            
            ui.menu_item_toggle("Draw gridlines", None::<&str>, &mut map_state.opts.draw_gridlines, true);
            
            let mut use_textures = map_state.opts.use_textures;
            ui.menu_item_toggle("Draw screens on map", None::<&str>, &mut use_textures, true);
            if ui.is_item_edited()
            {
                if !use_textures {
                    map_state.opts.use_textures = false;
                }
                else if map_state.screen_textures.is_empty() {
                    open_popup_map_textures = true;
                }
                else {
                    enable_map_textures = true;
                }
            }
            
            {
                let _token = ui.begin_disabled_with_cond(!map_state.opts.use_textures);
                if ui.menu_item("Refresh screen cache") {
                    for (_, texture_id) in map_state.screen_textures.drain() {
                        ex.textures.destroy_texture(texture_id);
                    }
                    draw_all_screens_and_create_textures(
                        &mut render_state,
                        &mut ex.textures,
                        &mut map_state.screen_textures,
                        *generate_mipmaps
                    );
                }
            }
            
            ui.separator();
            if ui.menu_item("Recenter preview") {
                preview_state.center = [0.5, 0.5];
            }
        });
        ui.menu("Window", || {
            if ui.menu_item("Reset layout") {
                *reset_layout = true;
            }
        });
        ui.menu("Help", || {
            let mut enabled = tooltips_are_enabled();
            ui.menu_item_toggle("Show tooltips", Some("F1"), &mut enabled, true);
            if ui.is_item_edited() {
                set_tooltips_enabled(enabled);
            }
            if ui.menu_item("Controls") {
                open_popup_controls = true;
            }
        });
    }
    
    // Controls popup
    if open_popup_controls {
        ui.open_popup("Controls");
    }
    {
        let [viewport_width, viewport_height] = ui.main_viewport().size();
        ui.set_next_window_pos([viewport_width * 0.5, viewport_height * 0.5], Condition::Always, [0.5, 0.5]);
    }
    if let Some(_token) = ui.begin_modal_popup_config("Controls")
        .flags(WindowFlags::ALWAYS_AUTO_RESIZE | WindowFlags::NO_MOVE | WindowFlags::NO_RESIZE)
        .begin()
    {
        let header = |label: &str| {
            let _token = ui.push_font_with_size(None, 24.0);
            ui.text(label);
        };
        
        header("Map");
        ui.bullet_text("Right click and drag to pan.");
        ui.bullet_text("Scroll wheel to zoom in and out.");
        ui.bullet_text("Click on a screen to lock the preview.");
        ui.bullet_text("To unlock the preview, click on the selected screen again, or on an empty screen.");
        ui.new_line();
        
        header("Partition List");
        ui.bullet_text("Double click on a partition to center it.");
        ui.bullet_text("Click on a column header to sort by that column.");
        ui.bullet_text("To invert the sort order, click on the column header again.");
        ui.new_line();
        
        header("Preview");
        ui.bullet_text("Scroll wheel to zoom in and out.");
        ui.bullet_text("If the preview is too large, simply move the mouse over the preview to pan.");
        ui.new_line();
        
        let button_width = ui.calc_text_width("OK") * 4.0;
        if ui.button_with_size("OK", [button_width, 0.0])
            || ui.is_key_pressed(Key::Escape)
        {
            ui.close_current_popup();
        }
    }
    
    // Textures popup
    if open_popup_map_textures {
        ui.open_popup("Warning##MapTextures");
    }
    {
        let [viewport_width, viewport_height] = ui.main_viewport().size();
        ui.set_next_window_pos([viewport_width * 0.5, viewport_height * 0.5], Condition::Always, [0.5, 0.5]);
    }
    if let Some(_token) = ui.begin_modal_popup_config("Warning##MapTextures")
        .flags(WindowFlags::ALWAYS_AUTO_RESIZE | WindowFlags::NO_MOVE | WindowFlags::NO_RESIZE)
        .begin()
    {
        ui.text("This feature is experimental. The application may crash if you don't have enough memory available.");
        ui.new_line();
        
        ui.bullet_text("After clicking OK, the application may become unresponsive while the screens are rendered. :)");
        ui.bullet_text("The map will not update automatically as you change options.");
        ui.bullet_text("To update the map, select View -> Refresh screen cache from the menu.");
        ui.bullet_text("That menu item may also cause the app to become unresponsive or crash.");
        ui.new_line();
        
        ui.checkbox("Generate mipmaps (looks better when zoomed out, but takes more time and memory)", generate_mipmaps);
        ui.new_line();
        
        let bytes_per_screen = if *generate_mipmaps { 767848 } else { 576000 };
        let memory_needed = render_state.screen_map.len() * bytes_per_screen;
        ui.text(format!("Estimated VRAM required: {}", bytes_to_string(memory_needed, 1)));
        ui.new_line();
        
        let button_width = ui.calc_text_width("OK") * 4.0;
        if ui.button_with_size("OK", [button_width, 0.0]) {
            enable_map_textures = true;
            ui.close_current_popup();
        }
        ui.same_line();
        if ui.button_with_size("Cancel", [button_width, 0.0])
            || ui.is_key_pressed(Key::Escape)
        {
            ui.close_current_popup();
        }
    }
    
    if enable_map_textures {
        map_state.opts.use_textures = true;
        // Force true aspect ratio on
        if map_state.opts.aspect_ratio == 1.0 {
            map_state.opts.aspect_ratio = 2.5;
            if let Some(geom) = &map_state.prev_geom {
                requested_center = Some(map_get_center_screen(geom));
            }
        }
        if map_state.screen_textures.is_empty() {
            draw_all_screens_and_create_textures(
                &mut render_state,
                &mut ex.textures,
                &mut map_state.screen_textures,
                *generate_mipmaps
            );
        }
    }
    
    // Export
    ui.window("Export").build(|| {
        build_window_export(ui, ex, export_state, &mut render_state, render_thread, render_rx, render_state_lock, render_progress, render_cancel)
    });
    
    // Partition options
    ui.window("Partition Options").build(|| {
        build_window_partitions(ui, ex, partition_state, &mut render_state)
    });
    
    // Partition list
    {
        let go_to_partition_index = ui.window("Partition List").build(|| {
            build_partition_table(ui, ex.fonts, partition_state, &mut render_state.partitions)
        }).unwrap_or_default();
        
        if let Some(i) = go_to_partition_index
            && let Some(partition) = render_state.partitions.get(i)
        {
            let bounds = partition.bounds();
            let partition_center = (
                (bounds.x.start + (bounds.x.end - bounds.x.start) / 2) as i64,
                (bounds.y.start + (bounds.y.end - bounds.y.start) / 2) as i64,
            );
            requested_center = Some(partition_center);
        }
    }
    
    // Drawing options
    {
        let invalidations = ui.window("Drawing Options").build(|| {
            let RenderState { draw_options, sync_options, seed, .. } = &mut *render_state;
            build_window_drawing(ui, ex,
                drawing_state,
                draw_options,
                sync_options,
                seed)
        }).unwrap_or_default();
        if invalidations.preview {
            preview_state.preview = None;
        }
        if invalidations.world_sync {
            render_state.world_sync = WorldSync::new(
                render_state.seed,
                &render_state.screen_map,
                &render_state.object_defs,
                &render_state.sync_options
            );
        }
    }
    
    // Map
    let hover_pos = {
        // Initial map center
        if map_state.prev_geom.is_none() {
            if let Some(partition) = render_state.partitions.first() {
                let bounds = partition.bounds();
                let partition_center = (
                    (bounds.x.start + (bounds.x.end - bounds.x.start) / 2) as i64,
                    (bounds.y.start + (bounds.y.end - bounds.y.start) / 2) as i64,
                );
                requested_center = Some(partition_center);
            }
            else {
                requested_center = Some((1000, 1000));
            }
        }
        
        let _token = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0])); 
        ui.window("Map").build(|| {
            build_map(
                ui,
                map_state,
                &render_state.screen_map,
                render_state.partitions.get(partition_state.selected),
                &partition_state.partition_members,
                requested_center,
            )
        }).flatten()
    };
    
    // Preview
    {
        let _token = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));
        ui.window("Preview")
            .flags(WindowFlags::NO_SCROLLBAR | WindowFlags::NO_SCROLL_WITH_MOUSE)
        .build(|| {
            let preview_screen =
                if map_state.selected_screen.is_some() {
                    map_state.selected_screen.clone()
                }
                else {
                    hover_pos
                };
            build_window_preview(ui, ex, preview_state, &mut render_state, preview_screen);
        });
    }
    
    // Hotkeys that should always trigger
    let ctrl = ui.io().key_ctrl();
    if ui.is_key_pressed(Key::F1) {
        toggle_tooltips();
    }
    if ui.is_key_pressed(Key::F2) {
        show_level_list = true;
    }
    if ctrl && ui.is_key_pressed(Key::F) {
        show_level_directory = true;
    }
    if ctrl && ui.is_key_pressed(Key::O) {
        show_output_directory = true;
    }
    
    // Hotkeys that shouldn't trigger when a widget has focus
    if !ui.is_any_item_focused() && !ui.is_any_item_active() {
        if ctrl && ui.is_key_pressed(Key::C) {
            copy_screen_pos = true;
        }
    }
    
    // Complete any tasks triggered by a menu item or hotkey
    if show_output_directory {
        let mut did_show = false;
        if !export_state.no_subdir {
            let level_info = name_pattern::LevelInfo::new(&render_state.ini, &export_state.level_dir);
            let subdir_pattern = NamePattern::parse(&export_state.subdir_spec);
            let subdir_name = subdir_pattern.make_string(&level_info, None);
            let output_dir = export_state.output_dir.join(subdir_name);
            if output_dir.exists() {
                show_dir_in_file_explorer(&output_dir);
                did_show = true;
            }
        }
        if !did_show {
            show_dir_in_file_explorer(&export_state.output_dir);
        }
    }
    if show_level_directory {
        show_dir_in_file_explorer(&export_state.level_dir);
    }
    if copy_screen_pos {
        if let Some(pos) = map_state.selected_screen.as_ref()
            .or(hover_pos.as_ref())
        {
            let coord_string = format!("x{}y{}", pos.0, pos.1);
            let _ = ex.clipboard.set_clipboard_text(&coord_string);
        }
    }
    
    if show_level_list {
        Some(Task::ShowLevelList)
    }
    else {
        None
    }
}

pub fn on_close_screen(_ui: &Ui, ex: &mut Extras, state: &mut State) {
    if let Some((_, texture_id)) = state.preview_state.preview.take() {
        ex.textures.destroy_texture(texture_id);
    }
    for (_, texture_id) in state.map_state.screen_textures.drain() {
        ex.textures.destroy_texture(texture_id);
    }
}

fn create_dockspace_layout(ui: &Ui) -> DockLayout {
    let style = unsafe { ui.style() };
    let [_window_padding_x, window_padding_y] = style.window_padding();
    let [_frame_padding_x, frame_padding_y] = style.frame_padding();
    let [_item_spacing_x, item_spacing_y] = style.item_spacing();
    
    // Estimate sizes
    let menu_bar_height = ui.text_line_height() + 2.0 * frame_padding_y;
    let tab_bar_height = ui.text_line_height() + 2.0 * frame_padding_y + style.window_border_size();
    let half_separator = 0.5 * style.docking_separator_size();
    let sidebar_width = 720.0 + half_separator;
    let preview_inner_height = 240.0 + 2.0 * window_padding_y;
    let preview_total_height = preview_inner_height + tab_bar_height + half_separator;
    let export_inner_height =
        (ui.text_line_height() * 2.0 + item_spacing_y) // Button
        + (ui.text_line_height() + 2.0 * frame_padding_y + item_spacing_y) * 8.0 // Options
        + 2.0 * window_padding_y; // Padding
    let export_total_height = export_inner_height + tab_bar_height + half_separator;

    // Calculate proportions
    let width_avail = ui.main_viewport().size()[0];
    let preview_percent_width = (sidebar_width / width_avail).min(0.5);
    
    let height_avail = ui.main_viewport().size()[1] - menu_bar_height;
    let preview_percent_height = (preview_total_height / height_avail).min(1.0 / 3.0);
    
    let remaining_height_avail = height_avail - f32::round(preview_percent_height * height_avail) - half_separator;
    let export_percent_height = (export_total_height / remaining_height_avail).min(2.0 / 3.0);
    
    let key_map = WindowKey::new("Map", "Map").unwrap();
    let key_export = WindowKey::new("Export", "Export").unwrap();
    let key_partition_opts = WindowKey::new("Partition Options", "Partition Options").unwrap();
    let key_drawing_opts = WindowKey::new("Drawing Options", "Drawing Options").unwrap();
    let key_preview = WindowKey::new("Preview", "Preview").unwrap();
    let key_partitions = WindowKey::new("Partition List", "Partition List").unwrap();
    
    DockLayout::split(
        DockSplit::Right,
        preview_percent_width,
        DockLayout::split(
            DockSplit::Down,
            preview_percent_height,
            DockLayout::tabs(&[key_preview]),
            DockLayout::split(
                DockSplit::Up,
                export_percent_height,
                DockLayout::tabs(&[key_export, key_partition_opts, key_drawing_opts]),
                DockLayout::tabs(&[key_partitions]),
            ),
        ),
        DockLayout::tabs(&[key_map])
    )
}

pub struct PartitionState {
    partition_members: FxHashMap<ScreenCoord, usize>,
    selected: usize,
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
    grid_fallback: bool,
    /// Set up the default sort column on the first frame
    set_sort_column: bool,
    sort_descending: bool,
    sort_column_index: usize,
}

impl PartitionState {
    /// `partitions` will be sorted. This is kind of weird and can probably be improved later.
    pub fn from_islands(partitioner: IslandsPartitioner, partitions: &mut [Partition]) -> Self {
        let mut state = Self {
            selected: 0,
            algorithm: PartitionAlgorithm::Islands,
            max_width: partitioner.max_size.0 as i32,
            max_height: partitioner.max_size.1 as i32,
            min_gap: *partitioner.gap.start() as i32,
            max_gap: *partitioner.gap.end() as i32,
            force: partitioner.force,
            grid_fallback: partitioner.fallback_to_grid,
            ..Default::default()
        };
        sort_partitions(partitions, state.sort_column_index, state.sort_descending);
        update_partition_members(&mut state.partition_members, partitions);
        
        state
    }
    
    /// `partitions` will be sorted. This is kind of weird and can probably be improved later.
    pub fn from_grid(partitioner: GridPartitioner, partitions: &mut [Partition]) -> Self {
        let mut state = Self {
            selected: 0,
            algorithm: PartitionAlgorithm::Grid,
            max_width: partitioner.max_size.0 as i32,
            max_height: partitioner.max_size.1 as i32,
            auto_rows: partitioner.rows.is_none(),
            auto_cols: partitioner.cols.is_none(),
            rows: partitioner.rows.unwrap_or(10) as i32,
            cols: partitioner.cols.unwrap_or(10) as i32,
            force: partitioner.force,
            ..Default::default()
        };
        sort_partitions(partitions, state.sort_column_index, state.sort_descending);
        update_partition_members(&mut state.partition_members, partitions);
        
        state
    }
}

impl Default for PartitionState {
    fn default() -> Self {
        Self {
            partition_members: FxHashMap::default(),
            selected: 0,
            algorithm: PartitionAlgorithm::Islands,
            max_width: 120,
            max_height: 300,
            min_gap: 1,
            max_gap: 10,
            auto_rows: true,
            auto_cols: true,
            rows: 10,
            cols: 10,
            force: false,
            grid_fallback: true,
            set_sort_column: true,
            sort_descending: true,
            sort_column_index: PARTITION_TABLE_COL_MEMORY,
        }
    }
}

#[derive(Clone, Copy, Default)]
enum PartitionAlgorithm {
    #[default]
    Islands,
    Grid,
}

fn update_partition_members(members: &mut FxHashMap<ScreenCoord, usize>, partitions: &[Partition]) {
    members.clear();
    for (i, positions) in partitions.iter().enumerate() {
        for pos in positions {
            members.insert(*pos, i);
        }
    }
}

fn build_window_partitions(ui: &Ui, _ex: &mut Extras, partition_state: &mut PartitionState, render_state: &mut RenderState) {
    let _token = ui.widget_group_begin();
    
    let button_height = ui.text_line_height() * 2.0;
    if ui.button_with_size("Rebuild partitions", [-1.0, button_height]) {
        let max_size = (partition_state.max_width as u64, partition_state.max_height as u64);
        render_state.partitions = match partition_state.algorithm {
            PartitionAlgorithm::Islands => {
                let gap = partition_state.min_gap as u64 ..= partition_state.max_gap as u64;
                let partitioner = IslandsPartitioner {
                    max_size,
                    gap,
                    force: partition_state.force,
                    fallback_to_grid: partition_state.grid_fallback,
                };
                partitioner.partitions(&render_state.screen_map)
            }
            PartitionAlgorithm::Grid => {
                let partitioner = GridPartitioner {
                    max_size,
                    rows: if partition_state.auto_rows { None } else { Some(partition_state.rows as u64) },
                    cols: if partition_state.auto_cols { None } else { Some(partition_state.cols as u64) },
                    force: partition_state.force,
                };
                partitioner.partitions(&render_state.screen_map)
            }
        };
        sort_partitions(&mut render_state.partitions, partition_state.sort_column_index, partition_state.sort_descending);
        update_partition_members(&mut partition_state.partition_members, &render_state.partitions);
    }
    
    let mut algo_index = partition_state.algorithm as usize;
    ui.widget_group_label("Algorithm");
    ui.combo_simple_string("##Algorithm", &mut algo_index, &["Islands", "Grid"]);
    partition_state.algorithm = match algo_index {
        0 => PartitionAlgorithm::Islands,
        1 => PartitionAlgorithm::Grid,
        _ => PartitionAlgorithm::Islands
    };
    tooltip(ui, "\
        How to split up the map when it's too large to fit into a single image.\n\
        - Islands: Divide the map into clusters of screens that are near one another.\n\
        - Grid: Divide the map according to a grid.");
    
    let max_width_px = partition_state.max_width * 600;
    ui.widget_group_label("Max width");
    ui.drag_int_config("##MaxWidth")
        .range(1, i32::MAX)
        .speed(0.1)
        .try_display_format(format!("%d screens / {max_width_px}px"))
        .expect("Invalid display format")
        .build(ui, &mut partition_state.max_width);
    tooltip(ui, "The maximum width of a single output image.");
    
    let max_height_px = partition_state.max_height * 240;
    ui.widget_group_label("Max height");
    ui.drag_int_config("##MaxHeight")
        .range(1, i32::MAX)
        .speed(0.1)
        .try_display_format(format!("%d screens / {max_height_px}px"))
        .expect("Invalid display format")
        .build(ui, &mut partition_state.max_height);
    tooltip(ui, "The maximum height of a single output image.");
    
    {
        let max_bytes = max_width_px as usize * max_height_px as usize * 4;
        let unit = best_unit_for_bytes(max_bytes);
        let mut max_size = convert_bytes_to_unit(max_bytes, unit);
        
        ui.widget_group_label("Max memory");
        let _token = ui.begin_disabled();
        ui.drag_float_config("##MaxMemory")
            .try_display_format(format!("%.1f{unit}"))
            .expect("Invalid display format")
            .build(ui, &mut max_size);
        tooltip(ui, "The max possible size of a single output image in RAM if it had width and height equal to the \
            max values set above.\n\
            \n\
            This is likely higher than the actual amount the largest partition will require. Check the Partitions \
            window to see the amount required (excluding assets and working memory).");
    }
    
    match partition_state.algorithm {
        PartitionAlgorithm::Islands => build_partition_options_islands(ui, partition_state),
        PartitionAlgorithm::Grid => build_partition_options_grid(ui, partition_state),
    };
}

fn build_partition_options_islands(ui: &Ui, state: &mut PartitionState) {
    ui.widget_group_label("Min gap");
    ui.drag_int_config("##MinGap")
        .range(1, i32::MAX)
        .speed(0.05)
        .build(ui, &mut state.min_gap);
    tooltip(ui, "If the max gap setting produces an island that is too big, that island will be broken up into \
        smaller islands by gradually reducing the gap size down to this value. Set this to the same value as max gap \
        if you don't want that to happen.");

    state.max_gap = state.max_gap.max(state.min_gap);
    ui.widget_group_label("Max gap");
    ui.drag_int_config("##MaxGap")
        .range(state.min_gap, i32::MAX)
        .speed(0.05)
        .build(ui, &mut state.max_gap);
    tooltip(ui, "The number of empty screens allowed between the screens of an island.");

    ui.checkbox("Force gap size", &mut state.force);
    tooltip(ui, "When enabled, the max gap setting will be respected even if the entire level fits into the chosen \
        max size.");
    
    ui.checkbox("Subdivide oversized islands", &mut state.grid_fallback);
    tooltip(ui, "When enabled, if the gap settings produce an island that is too big, that island will be subdivided \
        according to a grid (i.e. using the Grid algorithm).");
}

fn build_partition_options_grid(ui: &Ui, state: &mut PartitionState) {
    let inner_spacing_x = unsafe { ui.style().item_inner_spacing()[0] };
    let checkbox_width = ui.calc_checkbox_width("Auto");
    
    ui.widget_group_label("Rows");
    {
        let _token = ui.begin_disabled_with_cond(state.auto_rows);
        ui.set_next_item_width(-checkbox_width - inner_spacing_x);
        ui.drag_int_config("##Rows")
            .range(1, i32::MAX)
            .speed(0.05)
            .build(ui, &mut state.rows);
        tooltip(ui, "The number of rows to divide the level into.");
    }
    ui.same_line_with_spacing(0.0, inner_spacing_x);
    ui.checkbox("Auto##AutoRows", &mut state.auto_rows);
    tooltip(ui, "Calculate the number of rows based on the max height.");
    
    ui.widget_group_label("Cols");
    {
        let _token = ui.begin_disabled_with_cond(state.auto_cols);
        ui.set_next_item_width(-checkbox_width - inner_spacing_x);
        ui.drag_int_config("##Columns")
            .range(state.min_gap, i32::MAX)
            .speed(0.05)
            .build(ui, &mut state.cols);
        tooltip(ui, "The number of columns to divide the level into.");
    }
    ui.same_line_with_spacing(0.0, inner_spacing_x);
    ui.checkbox("Auto##AutoCols", &mut state.auto_cols);
    tooltip(ui, "Calculate the number of columns based on the max width.");
    
    ui.checkbox("Force rows and columns", &mut state.force);
    tooltip(ui, "When enabled, the row and column settings will be respected even if the entire level fits into the \
        chosen max size.");
}

const PARTITION_TABLE_COL_X_MIN     : usize = 0;
const PARTITION_TABLE_COL_Y_MIN     : usize = 1;
const PARTITION_TABLE_COL_X_MAX     : usize = 2;
const PARTITION_TABLE_COL_Y_MAX     : usize = 3;
const PARTITION_TABLE_COL_WIDTH     : usize = 4;
const PARTITION_TABLE_COL_HEIGHT    : usize = 5;
const PARTITION_TABLE_COL_WIDTH_PX  : usize = 6;
const PARTITION_TABLE_COL_HEIGHT_PX : usize = 7;
const PARTITION_TABLE_COL_MEMORY    : usize = 8;
const PARTITION_TABLE_N_COLUMNS     : usize = 9;
const PARTITION_TABLE_COL_LABELS: [&'static str; PARTITION_TABLE_N_COLUMNS] = [
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

fn build_partition_table(ui: &Ui, fonts: &Fonts, partition_state: &mut PartitionState, partitions: &mut [Partition]) -> Option<usize> {
    let mut go_to_partition_index: Option<usize> = None;
    
    let table_height = {
        let style = unsafe { ui.style() };
        let n_rows = partitions.len() + 1; // +1 for header
        let row_height = ui.text_line_height()
            + 2.0 * style.cell_padding()[1];
        let total_height = n_rows as f32 * row_height + 1.0; // +2 for outer borders, -1 for bottom border of last row
        if total_height > ui.content_region_avail_height() {
            -1.0 // fill
        }
        else {
            total_height
        }
    };
    
    let mut table_builder = ui.table("PartitionsTable")
        .outer_size([-1.0, table_height])
        .flags(TableFlags::BORDERS | TableFlags::SCROLL_Y | TableFlags::SORTABLE);

    for label in PARTITION_TABLE_COL_LABELS {
        table_builder = table_builder.add_column(TableColumnSetup {
            name: label,
            flags: TableColumnFlags::NONE,
            width: Some(TableColumnWidth::Fixed(0.0)),
            indent: None,
            user_data: TableColumnUserData::default(),
        });
    }
    
    table_builder.build(|ui| {
        ui.table_setup_scroll_freeze(0, 1);
        ui.table_headers_row();
        
        // Sorting
        if partition_state.set_sort_column {
            ui.table_set_column_sort_direction(8, SortDirection::Descending, false);
            partition_state.selected = 0;
            partition_state.set_sort_column = false;
        }
        if let Some(mut specs) = ui.table_get_sort_specs()
            && specs.is_dirty()
        {
            specs.clear_dirty(ui);
            let selection_bounds = partitions.get(partition_state.selected)
                .map(|p| p.bounds());

            if let Some(spec) = specs.iter().next() {
                partition_state.sort_column_index = spec.column_index.get();
                partition_state.sort_descending = spec.sort_direction == SortDirection::Descending;
                sort_partitions(partitions, partition_state.sort_column_index, partition_state.sort_descending);
            }
            
            if let Some(bounds) = selection_bounds
                && let Some(index) = partitions.iter().position(|p| p.bounds() == bounds)
            {
                partition_state.selected = index;
            }
        }
        
        let _token = ui.push_font(fonts.mono);
        
        for (i, partition) in partitions.iter().enumerate() {
            let bounds = partition.bounds();
            let x_min = bounds.x_min();
            let x_max = bounds.x_max();
            let y_min = bounds.y_min();
            let y_max = bounds.y_max();
            let width = x_max - x_min + 1;
            let height = y_max - y_min + 1;
            let width_px = width * 600;
            let height_px = height * 240;
            let memory_bytes = (width_px * height_px * 4) as usize;
            
            ui.table_next_row();
            ui.table_next_column();
            
            // Selection
            {
                let _id = ui.push_id(i);
                if ui.selectable_config("##RowSelect")
                    .selected(partition_state.selected == i)
                    .flags(SelectableFlags::SPAN_ALL_COLUMNS)
                    .build()
                {
                    partition_state.selected = i;
                }
                if ui.is_item_clicked() && ui.is_mouse_double_clicked(MouseButton::Left) {
                    go_to_partition_index = Some(i);
                }
            }
            
            // Color indicator
            ui.same_line_with_spacing(0.0, 0.0);
            {
                let color_index = partition.positions()
                    .first()
                    .and_then(|pos| partition_state.partition_members.get(pos).cloned())
                    .unwrap_or(0);
                let color = MAP_COLORS[color_index % MAP_COLORS.len()];
                just_a_square(ui, color, true, true);
            }
            ui.same_line();
            
            ui.text_aligned_right(x_min.to_string());
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

fn sort_partitions(partitions: &mut [Partition], column_index: usize, descending: bool) {
    macro_rules! do_sort {
        ($partitions:ident, $is_descending:expr, $p:ident, $get_key:block) => {
            if $is_descending {
                $partitions.sort_by_key(|$p: &Partition| cmp::Reverse($get_key));
            }
            else {
                $partitions.sort_by_key(|$p: &Partition| $get_key);
            }
        };
    }
    match column_index {
        PARTITION_TABLE_COL_X_MIN => do_sort!(partitions, descending, p, {
            (p.bounds().x_min(), p.bounds().y_min())
        }),
        PARTITION_TABLE_COL_Y_MIN => do_sort!(partitions, descending, p, {
            (p.bounds().y_min(), p.bounds().x_min())
        }),
        PARTITION_TABLE_COL_X_MAX => do_sort!(partitions, descending, p, {
            (p.bounds().x_max(), p.bounds().y_max())
        }),
        PARTITION_TABLE_COL_Y_MAX => do_sort!(partitions, descending, p, {
            (p.bounds().y_max(), p.bounds().x_max())
        }),
        PARTITION_TABLE_COL_WIDTH => do_sort!(partitions, descending, p, {
            (p.bounds().width(), p.bounds().height())
        }),
        PARTITION_TABLE_COL_HEIGHT => do_sort!(partitions, descending, p, {
            (p.bounds().height(), p.bounds().width())
        }),
        PARTITION_TABLE_COL_WIDTH_PX => do_sort!(partitions, descending, p, {
            (p.bounds().width_px(), p.bounds().height_px())
        }),
        PARTITION_TABLE_COL_HEIGHT_PX => do_sort!(partitions, descending, p, {
            (p.bounds().height_px(), p.bounds().width_px())
        }),
        PARTITION_TABLE_COL_MEMORY => do_sort!(partitions, descending, p, {
            p.bounds().size_bytes_rgba()
        }),
        _ => {}
    }
}

struct PreviewState {
    preview: Option<(ScreenCoord, TextureId)>,
    center: [f32; 2],
    zoom_level: i32,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            preview: None,
            center: [0.5, 0.5],
            zoom_level: 0,
        }
    }
}

fn build_window_preview(ui: &Ui, ex: &mut Extras, preview_state: &mut PreviewState, render_state: &mut RenderState, screen_pos: Option<ScreenCoord>) { 
    let [origin_x, origin_y] = ui.cursor_pos();
    let [origin_x_screen, origin_y_screen] = ui.cursor_screen_pos();
    let [width_avail, height_avail] = ui.content_region_avail();
    let is_window_hovered = ui.is_window_hovered();
    
    let Some(pos) = screen_pos else {
        ui.text_aligned_center_center("Mouse over the map to preview a screen");
        return;
    };
    
    let pos_changed = preview_state.preview.as_ref().is_none_or(|preview| {
        preview.0 != pos
    });
    
    if pos_changed {
        if let Some((_, texture_id)) = preview_state.preview.take() {
            ex.textures.destroy_texture(texture_id);
        }
        
        preview_state.preview = match draw_single_screen(render_state, pos) {
            Some(image) => {
                let id = ex.textures.create_texture(image.width(), image.height(), &image, false);
                Some((pos, id))
            }
            None => None
        };
        preview_state.center = [0.5, 0.5];
    }
    
    const MIN_ZOOM: i32 = 0;
    const MAX_ZOOM: i32 = 5;
    const MAG_LEVELS: [f32; 1 + MAX_ZOOM as usize] = [1.0, 2.0, 3.0, 4.0, 6.0, 8.0];
    if is_window_hovered {
        let mouse_wheel = ui.get_mouse_wheel() as i32;
        preview_state.zoom_level = (preview_state.zoom_level + mouse_wheel).clamp(MIN_ZOOM, MAX_ZOOM);
    }
    
    if let Some(preview) = &preview_state.preview
        && let Some(texture) = ex.textures.get_texture_info(preview.1)
    {
        let scale = MAG_LEVELS[preview_state.zoom_level as usize];
        let width = texture.width() * scale;
        let height = texture.height() * scale;
        
        if is_window_hovered {
            let border_x = (width_avail * 0.1).round().min(10.0);
            let border_y = (height_avail * 0.1).round().min(10.0);
            let [mouse_x_screen, mouse_y_screen] = ui.mouse_pos();
            let mouse_x_window = mouse_x_screen - (origin_x_screen + border_x);
            let mouse_y_window = mouse_y_screen - (origin_y_screen + border_y);
            let mouse_x_proportion = mouse_x_window / (width_avail - border_x * 2.0);
            let mouse_y_proportion = mouse_y_window / (height_avail - border_y * 2.0);
            preview_state.center = [mouse_x_proportion.clamp(0.0, 1.0), mouse_y_proportion.clamp(0.0, 1.0)];
        }
        if width <= width_avail {
            preview_state.center[0] = 0.5;
        }
        if height <= height_avail {
            preview_state.center[1] = 0.5;
        }
        
        let x = f32::round((width_avail - width) * smoothstep(preview_state.center[0]));
        let y = f32::round((height_avail - height) * smoothstep(preview_state.center[1]));
        ui.set_cursor_pos([origin_x + x, origin_y + y]);
        
        let draw_list = ui.get_window_draw_list();
        draw_list.set_sampler_nearest();
        ui.image(texture, [width, height]);
        draw_list.set_sampler_linear();
    }
}

fn smoothstep(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

fn rgb_to_rgba(image: image::RgbImage) -> RgbaImage {
    image::DynamicImage::from(image).to_rgba8()
}

fn draw_single_screen(render_state: &mut RenderState, screen_pos: ScreenCoord) -> Option<RgbaImage> {
    let screen_index = render_state.screen_map.index_of(&screen_pos)?;
    let screen = &render_state.screen_map[screen_index];
    
    let image_rgb = ksmap::drawing::draw_screen(
        render_state.seed,
        screen,
        screen_index,
        &render_state.gfx,
        &render_state.object_defs,
        &render_state.ini,
        render_state.draw_options,
        &render_state.world_sync
    );
    Some(rgb_to_rgba(image_rgb))
}

fn draw_all_screens_and_create_textures(
    render_state: &mut RenderState,
    textures: &mut Textures,
    lookup: &mut FxHashMap<ScreenCoord, TextureId>,
    generate_mipmaps: bool,
) {
    for (i, screen) in render_state.screen_map.iter().enumerate() {
        let image_rgb = ksmap::drawing::draw_screen(
            render_state.seed,
            screen,
            i,
            &render_state.gfx,
            &render_state.object_defs,
            &render_state.ini,
            render_state.draw_options,
            &render_state.world_sync
        );
        let image = rgb_to_rgba(image_rgb);
        let texture_id = textures.create_texture(image.width(), image.height(), &image.into_vec(), generate_mipmaps);
        lookup.insert(screen.position, texture_id);
    }
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

#[derive(Default)]
struct Invalidations {
    world_sync: bool,
    preview: bool,
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

fn build_window_drawing(
    ui: &Ui,
    _ex: &mut Extras,
    state: &mut DrawingState,
    draw_options: &mut DrawOptions,
    sync_options: &mut SyncOptions,
    seed: &mut MapSeed,
) -> Invalidations {
    let original_seed = seed.clone();
    let original_draw_options = draw_options.clone();
    let original_sync_options = sync_options.clone();
    
    let _token = ui.widget_group_begin();
    
    let mut seed_buffer = seed.to_string();
    let button_width = ui.calc_button_width("Random");
    let inner_spacing_x = unsafe { ui.style().item_inner_spacing()[0] };
    ui.widget_group_label("Seed");
    ui.set_next_item_width(-button_width - inner_spacing_x);
    if InputText::new(ui, "##Seed", &mut seed_buffer)
        .flags(InputTextFlags::CHARS_HEXADECIMAL | InputTextFlags::CALLBACK_EDIT)
        .callback(MapSeedEditCallback(16))
        .build()
    {
        if let Ok(new_seed) = MapSeed::try_from(seed_buffer) {
            *seed = new_seed;
        }
    }
    tooltip(ui, "The RNG seed. If you use the same seed with the same settings, you will get the same output. Pick a \
        new seed if you want different results. Must be between 1 and 16 hexadecimal digits (0-9 A-F).");
    
    ui.same_line_with_spacing(0.0, inner_spacing_x);
    if ui.button("Random") {
        *seed = MapSeed::random();
    }
    
    let mut lasers_index = match (draw_options.ignore_laser_phase, sync_options.maximize_visible_lasers) {
        (false, true) => 0,
        (false, false) => 1,
        (true, _) => 2
    };
    ui.widget_group_label("Lasers");
    if ui.combo_simple_string("##Lasers", &mut lasers_index, &[
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
    tooltip(ui, "How to handle laser phases.\n\
        - Maximize: Choose the phase (red/green) with the most lasers\n\
        - Randomize: Choose a phase (red/green) randomly\n\
        - All: Draw all lasers regardless of phase");
    
    let mut tint_index = match draw_options.tint_strategy {
        TintStrategy::Ignore => 0,
        TintStrategy::Explicit => 1
    };
    ui.widget_group_label("Tints");
    if ui.combo_simple_string("##Tints", &mut tint_index, &[
        "Ignore",
        "Explicit"
    ]) {
        match tint_index {
            0 => draw_options.tint_strategy = TintStrategy::Ignore,
            1 => draw_options.tint_strategy = TintStrategy::Explicit,
            _ => {}
        }
    }
    tooltip(ui, "How to handle screen tints.\n\
        - Ignore: Ignore screen tints.\n\
        - Explicit: Apply tints to screens that explicitly have one.");
    
    ui.widget_group_label("Min alpha");
    if ui.drag_int_config("##MinAlpha")
        .range(0, 255)
        .speed(0.1)
        .build(ui, &mut state.min_alpha)
    {
        draw_options.trans_max_override = alpha_to_trans(state.min_alpha as u8);
    }
    tooltip(ui, "The minimum alpha value (0-255) for objects that have random opacity. \
        This helps ensure objects such as ghosts are visible on the map.");
    
    ui.widget_group_label("Min alpha threshold");
    if ui.drag_int_config("##AlphaThreshold")
        .range(0, i32::MAX)
        .speed(0.1)
        .build(ui, &mut state.min_alpha_threshold)
    {
        draw_options.trans_max_threshold = state.min_alpha_threshold as u32;
    }
    tooltip(ui, "The number of copies of an object a screen must have to ignore the min alpha setting. \
        This allows for more natural variation when an object appears many times on one screen.");
    
    let alpha_sim_secs = state.alpha_sim_frames as f32 / 50.0;
    ui.widget_group_label("Alpha sim frames");
    if ui.drag_int_config("##AlphaFrames")
        .range(0, i32::MAX)
        .try_display_format(format!("%d / {alpha_sim_secs:.1}s"))
        .expect("Invalid display format")
        .build(ui, &mut state.alpha_sim_frames)
    {
        draw_options.trans_frames = state.alpha_sim_frames as u32;
    }
    tooltip(ui, "The number of game frames to simulate for objects that have random opacity (50 = 1 second).");
    
    ui.checkbox("Show invisible objects", &mut draw_options.show_invisible);
    tooltip(ui, "Draw editor icons for invisible objects such as shifts and signs.");
    
    ui.checkbox("Show proximity-sensitive objects", &mut draw_options.show_proximity);
    tooltip(ui, "Draw objects that are only visible when Juni is nearby such as 14:19.");
    
    let mut invalidations = Invalidations::default();
    if *draw_options != original_draw_options {
        invalidations.preview = true;
    }
    if *sync_options != original_sync_options {
        invalidations.preview = true;
        invalidations.world_sync = true;
    }
    if *seed != original_seed {
        invalidations.preview = true;
        invalidations.world_sync = true;
    }
    
    invalidations
}

#[derive(Default, Clone)]
struct ExportState {
    level_dir: PathBuf,
    output_dir: PathBuf,
    subdir_spec: String,
    partition_spec: String,
    no_subdir: bool,
    no_subdir_for_single: bool,
    use_subdir_name_for_single: bool,
    use_multithreaded_encoder: bool,
    compression_level: u8,
}

impl ExportState {
    pub fn new(level_dir: PathBuf) -> Self {
        Self {
            level_dir,
            output_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            subdir_spec: String::from("$author - $name"),
            partition_spec: String::from("$bounds"),
            no_subdir: false,
            no_subdir_for_single: true,
            use_subdir_name_for_single: true,
            use_multithreaded_encoder: true,
            compression_level: 9,
        }
    }
}

fn build_window_export(
    ui: &Ui,
    _ex: &mut Extras,
    export_state: &mut ExportState,
    render_state: &mut RenderState,
    render_thread: &mut Option<JoinHandle<()>>,
    render_rx: &mut Option<mpsc::Receiver<RenderMessage>>,
    render_state_lock: &RenderStateLock,
    render_progress: &mut RenderProgress,
    render_cancel: &mut Arc<AtomicBool>,
) {
    let _token = ui.widget_group_begin();
    
    let button_height = ui.text_line_height() * 2.0;
    if ui.button_with_size("Export", [-1.0, button_height]) {
        let (tx, rx) = mpsc::channel();
        let render_state_for_thread = Arc::clone(render_state_lock);
        let render_cancel_for_thread = Arc::clone(render_cancel);
        let export_state_for_thread = export_state.clone();
        let handle = std::thread::spawn(|| {
            do_the_render(render_state_for_thread, export_state_for_thread, tx, render_cancel_for_thread)
        });
        render_rx.replace(rx);
        render_thread.replace(handle);
        render_cancel.store(false, atomic::Ordering::Relaxed);
        render_progress.tasks.clear();
        render_progress.screens_done = 0;
        render_progress.screens_total = 0;
        for partition in &render_state.partitions {
            render_progress.tasks.push(RenderTask {
                status: RenderTaskStatus::NotStarted,
                label: partition.bounds().to_string(),
                n_screens: partition.len(),
            });
            render_progress.screens_total += partition.len();
        }
    }
    
    {
        ui.widget_group_label("Output directory");
        let _token = ui.begin_group();
        
        let inner_spacing_x = unsafe { ui.style().item_inner_spacing()[0] };
        let button_width = ui.calc_button_width("Browse");
        ui.set_next_item_width(-button_width - inner_spacing_x);
        
        let mut path_buffer = export_state.output_dir.display().to_string();
        if ui.input_text("##OutputDir", &mut path_buffer)
            .build()
        {
            export_state.output_dir.clear();
            export_state.output_dir.push(path_buffer);
        }
        
        ui.same_line_with_spacing(0.0, inner_spacing_x);
        if ui.button("Browse") {
            if let Some(path) = rfd::FileDialog::new()
                .set_directory(&export_state.output_dir)
                .pick_folder()
            {
                export_state.output_dir = path;
            }
        }
    }
    tooltip(ui, "The base directory save images to. See also: Subdirectory name");
    
    ui.widget_group_label("Subdirectory name");
    ui.input_text("##SubdirectoryName", &mut export_state.subdir_spec).build();
    tooltip(ui, "The name of the subdirectory to save the images to, relative to the output directory.\n\
        \n\
        The following expressions will be substituted:\n\
        - $dirname: The name of the level directory.\n\
        - $author: The author of the level from World.ini.\n\
        - $name: The name of the level from World.ini.\n\
        \n\
        Use $$ if you want the directory name to have a dollar sign.");
    
    ui.widget_group_label("Partition name");
    ui.input_text("##PartitionName", &mut export_state.partition_spec).build();
    tooltip(ui, "Specifies the filename for each partition. You should use the $ expressions below to ensure each \
        partition is given a unique name. The file extension (.png) will be added automatically.\n\
        \n\
        The following expressions will be substituted:\n\
        - $index: The number of the partition in the order exported, starting from 0.\n\
        - $bounds: The full bounds of the partition, e.g. \"x100y200 to x300y400\".\n\
        - $min: The top left screen of the partition, e.g. \"x100y200\".\n\
        - $max: The bottom right screen of the partition, e.g. \"x300y400\".\n\
        - $xmin: The minimum x coordinate in the partition, e.g. \"100\".\n\
        - $xmax: The maximum x coordinate in the partition, e.g. \"300\".\n\
        - $ymin: The minimum y coordinate in the partition, e.g. \"200\".\n\
        - $ymax: The maximum y coordinate in the partition, e.g. \"400\".\n\
        - $dirname: The name of the level directory.\n\
        - $author: The author of the level from World.ini.\n\
        - $name: The name of the level from World.ini.\n\
        \n\
        Use $$ if you want the file name to have a dollar sign.");
    
    ui.checkbox("Don't create subdirectory", &mut export_state.no_subdir);
    tooltip(ui, "When enabled, the subdirectory name setting is ignored and the images are saved directly to the \
        output directory.");
    
    ui.checkbox("Don't create subdirectory for single partition", &mut export_state.no_subdir_for_single);
    tooltip(ui, "When enabled, if there is only one partition, it will be saved directly to the output directory.");
    
    ui.checkbox("Use subdirectory name for single partition", &mut export_state.use_subdir_name_for_single);
    tooltip(ui, "When enabled, if there is only one partition, it will be named according to the subdirectory name \
        setting instead of the partition name setting.");
    
    ui.widget_group_label("Compression level");
    ui.slider("##CompressionLevel", 1, 9, &mut export_state.compression_level);
    tooltip(ui, "Desired compression level from 1 (fastest, worst compression) to 9 (slowest, best compression).");
    
    ui.checkbox("Multithreaded encoding", &mut export_state.use_multithreaded_encoder);
    tooltip(ui, "When enabled, multiple threads will be used for PNG encoding. This is usually MUCH faster, but the \
        compression ratio may be slightly worse.");
}

pub struct RenderState {
    pub ini: Ini,
    pub object_defs: Arc<ObjectDefs>,
    pub gfx: Graphics,
    pub screen_map: ScreenMap,
    pub seed: MapSeed,
    pub partitions: Vec<Partition>,
    pub world_sync: WorldSync,
    pub draw_options: DrawOptions,
    pub sync_options: SyncOptions,
}
pub type RenderStateLock = Arc<RwLock<RenderState>>;

struct RenderTask {
    status: RenderTaskStatus,
    label: String,
    n_screens: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RenderTaskStatus {
    NotStarted,
    Rendering,
    Exporting,
    Done
}

impl std::fmt::Display for RenderTaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderTaskStatus::NotStarted => write!(f, "Not Started"),
            RenderTaskStatus::Rendering => write!(f, "Rendering"),
            RenderTaskStatus::Exporting => write!(f, "Exporting"),
            RenderTaskStatus::Done => write!(f, "Done"),
        }
    }
}

#[derive(Default)]
struct RenderProgress {
    tasks: Vec<RenderTask>,
    screens_done: usize,
    screens_total: usize,
}

enum RenderMessage {
    PartitionUpdate(usize, RenderTaskStatus),
    Done,
    Aborted,
    Error(String),
}

fn do_the_render(render_state_lock: RenderStateLock, export_state: ExportState, tx: mpsc::Sender<RenderMessage>, cancel: Arc<AtomicBool>) {
    let Ok(render_state) = render_state_lock.read() else { return };    
    let draw_context = DrawContext {
        seed: render_state.seed,
        screens: &render_state.screen_map,
        gfx: &render_state.gfx,
        defs: &render_state.object_defs,
        ini: &render_state.ini,
        world_sync: &render_state.world_sync,
        options: render_state.draw_options.clone(),
    };
    
    let level_info = name_pattern::LevelInfo::new(&render_state.ini, &export_state.level_dir);
    let partition_name_pattern = NamePattern::parse(&export_state.partition_spec);
    let subdir_name_pattern = NamePattern::parse(&export_state.subdir_spec);
    let subdir_name = subdir_name_pattern.make_string(&level_info, None);
    
    let is_single_partition = render_state.partitions.len() < 2;
    let output_dir =
        if export_state.no_subdir || (is_single_partition && export_state.no_subdir_for_single) {
            export_state.output_dir.clone()
        }
        else {
            export_state.output_dir.join(&subdir_name)
        };
    
    if let Err(err) = std::fs::create_dir_all(&output_dir) {
        let _ = tx.send(RenderMessage::Error(err.to_string()));
        return;
    }
    
    for (i, partition) in render_state.partitions.iter().enumerate() {
        if cancel.load(atomic::Ordering::Relaxed) {
            let _ = tx.send(RenderMessage::Aborted);
            return;
        }
        
        let _ = tx.send(RenderMessage::PartitionUpdate(i, RenderTaskStatus::Rendering));
        let canvas = match drawing::draw_partition(draw_context, partition, None) {
            Ok(canvas) => canvas,
            Err(err) => {
                let _ = tx.send(RenderMessage::Error(err.to_string()));
                return;
            }
        };

        if cancel.load(atomic::Ordering::Relaxed) {
            let _ = tx.send(RenderMessage::Aborted);
            return;
        }
        
        let _ = tx.send(RenderMessage::PartitionUpdate(i, RenderTaskStatus::Exporting));
        let file_name =
            if is_single_partition && export_state.use_subdir_name_for_single {
                subdir_name.clone()
            }
            else {
                let partition_info = name_pattern::PartitionInfo {
                    index: i,
                    bounds: partition.bounds(),
                };
                partition_name_pattern.make_string(&level_info, Some(partition_info))
            };
        let output_path = output_dir.join(file_name).with_extension("png");
        
        if export_state.use_multithreaded_encoder {
            if let Err(err) = drawing::export_canvas_multithreaded(canvas, &output_path, export_state.compression_level) {
                let _ = tx.send(RenderMessage::Error(err.to_string()));
                return;
            }
        }
        else {
            if let Err(err) = drawing::export_canvas(canvas, &output_path, export_state.compression_level) {
                let _ = tx.send(RenderMessage::Error(err.to_string()));
                return;
            }
        }
        
        let _ = tx.send(RenderMessage::PartitionUpdate(i, RenderTaskStatus::Done));
    }

    let _ = tx.send(RenderMessage::Done);
}

fn build_window_progress(ui: &Ui, _ex: &mut Extras, state: &mut State) {
    if let Some(rx) = state.render_rx.as_mut() {
        loop {
            match rx.try_recv() {
                Ok(RenderMessage::PartitionUpdate(i, progress)) => {
                    state.render_progress.tasks[i].status = progress;
                    if progress == RenderTaskStatus::Done {
                        state.render_progress.screens_done += state.render_progress.tasks[i].n_screens;
                    }
                }
                Ok(RenderMessage::Error(error_message)) => {
                    state.render_thread.take();
                    state.render_rx.take();
                    state.render_error = Some(error_message);
                    break;
                }
                Ok(RenderMessage::Done) | Ok(RenderMessage::Aborted) => {
                    state.render_thread.take();
                    state.render_rx.take();
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    state.render_thread.take();
                    state.render_rx.take();
                    let error_message = String::from("The render thread panicked :(");
                    state.render_error = Some(error_message);
                    break;
                }
            }
        }
    }

    let item_spacing_x = unsafe { ui.style().item_spacing()[0] };
    let progress_bar_width = {
        let avail_width = ui.content_region_avail_width();
        avail_width * (10.0 / 12.0) - item_spacing_x
    };
    let percent_done = state.render_progress.screens_done as f32 / state.render_progress.screens_total as f32;
    ui.progress_bar(percent_done)
        .size([progress_bar_width, 0.0])
        .build();
    
    ui.same_line();
    if ui.button_with_size("Cancel", [-1.0, 0.0]) {
        state.render_cancel.store(true, atomic::Ordering::Relaxed);
        if state.render_thread.is_none() {
            state.render_error.take(); // The error is what's keeping the progress screen open
        }
    }
    
    if let Some(error_message) = &state.render_error {
        ui.text_wrapped(format!("Render failed. Reason: {error_message}"));
        return;
    }
    
    ui.child_window("Render Progress")
        .size([-1.0, -1.0])
    .build(ui, || {
        let avail_width = ui.content_region_avail_width();
        for task in &state.render_progress.tasks {
            if task.status == RenderTaskStatus::Done { continue }
            
            let label_width = ui.calc_text_width(&task.label);
            let label_x = f32::round((avail_width - item_spacing_x) * 0.5) - label_width;
            ui.set_cursor_pos_x(label_x);
            ui.text(&task.label);
            
            let color = match task.status {
                RenderTaskStatus::NotStarted => [0.5, 0.5, 0.5, 1.0],
                RenderTaskStatus::Rendering => ui.style_color(StyleColor::PlotHistogram),
                RenderTaskStatus::Exporting => ui.style_color(StyleColor::PlotHistogram),
                RenderTaskStatus::Done => [0.5, 1.0, 0.5, 1.0],
            };
            let _token = ui.push_style_color(StyleColor::Text, color);
            ui.same_line();
            ui.text(task.status.to_string());
        }
    });
}

fn show_dir_in_file_explorer<P: AsRef<Path>>(path: P) {
    let Ok(abs_path) = std::path::absolute(path) else { return };
    let _ = std::process::Command::new("explorer.exe")
        .arg("/root,")
        .arg(abs_path)
        .output();
}

fn just_a_square(ui: &Ui, color: [f32; 4], filled: bool, rounded: bool) {
    let [cursor_x, cursor_y] = ui.cursor_screen_pos();
    let line_height = ui.text_line_height();
    let size = (line_height * 2.0 / 3.0).round();
    ui.dummy([size, line_height]);
    
    if ui.is_item_visible() {
        let offset_y = ((line_height - size) / 2.0).round();
        let top_left = [
            cursor_x,
            cursor_y + offset_y
        ];
        let bottom_right = [
            top_left[0] + size,
            top_left[1] + size
        ];
        let rounding =
            if rounded {
                unsafe { ui.style().frame_rounding() }
            }
            else {
                0.0
            };
        
        ui.get_window_draw_list()
            .add_rect(top_left, bottom_right, color)
            .filled(filled)
            .rounding(rounding)
            .build();
    }
}
