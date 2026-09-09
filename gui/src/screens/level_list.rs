use std::{fs, path::{Path, PathBuf}};

use imgui_app::{Extras, dear_imgui_rs::{Key, MouseButton, SelectableFlags, StyleColor}};
use imgui_app::dear_imgui_rs::{Condition, Ui, TableFlags, TableSizingPolicy, WindowFlags};

pub struct State {
    levels: Vec<LevelListItem>,
    filter: String,
    selected_index: usize,
    get_focus: bool,
}

struct LevelListItem {
    abs_path: PathBuf,
    display_name: String,
    search_name: String,
    is_visible: bool,
}

impl State {
    pub fn new(ks_dir: impl AsRef<Path>) -> Self {
        let worlds_dir = ks_dir.as_ref().join("Worlds");
        let levels = list_levels(worlds_dir).unwrap();
        Self {
            levels,
            filter: String::new(),
            selected_index: 0,
            get_focus: true,
        }
    }
}

pub fn build_ui(ui: &Ui, ex: &mut Extras, state: &mut State) -> Option<PathBuf> {
    let (width, height) = ex.window.size();
    let open_level = ui.window("Level List")
        .position([0.0, 0.0], Condition::Always)
        .size([width as f32, height as f32], Condition::Always)
        .flags(WindowFlags::NO_TITLE_BAR | WindowFlags::NO_MOVE | WindowFlags::NO_RESIZE)
    .build(|| {
        let mut open_level = false;
        
        // Filter
        {
            let inner_spacing_x = unsafe { ui.style().item_inner_spacing()[0] };
            
            let mut hint_color = ui.style_color(StyleColor::Text);
            hint_color[3] *= 0.25;
            let _token = ui.push_style_color(StyleColor::TextDisabled, hint_color);
            
            if state.get_focus {
                ui.set_keyboard_focus_here();
                state.get_focus = false;
            }
            let full_width = ui.content_region_avail_width();
            let input_width = full_width * (10.0 / 12.0) - inner_spacing_x;
            ui.set_next_item_width(input_width);
            if ui.input_text("##Level filter", &mut state.filter)
                .hint("Filter levels")
                .build()
            {
                filter_levels(&mut state.levels, &state.filter);
                if state.levels.get(state.selected_index)
                    .is_some_and(|level| !level.is_visible)
                {
                    state.selected_index = state.levels.iter()
                        .position(|level| level.is_visible)
                        .unwrap_or(0);
                }
            }

            ui.same_line_with_spacing(0.0, inner_spacing_x);
            open_level |= ui.button_with_size("Open", [-1.0, 0.0]);
        }
        
        // Keyboard controls
        open_level |= ui.is_key_pressed(Key::Enter);
        let mut nudge_selection: isize = 0;
        if ui.is_key_pressed(Key::UpArrow) {
            nudge_selection = -1;
        }
        else if ui.is_key_pressed(Key::DownArrow) {
            nudge_selection = 1;
        }
        else if ui.is_key_pressed(Key::PageUp) {
            nudge_selection = -calc_rows_per_page(ui);
        }
        else if ui.is_key_pressed(Key::PageDown) {
            nudge_selection = calc_rows_per_page(ui);
        }
        if !ui.is_any_item_focused() && !ui.is_any_item_active() {
            if ui.is_key_pressed(Key::Home) {
                nudge_selection = isize::MIN;
            }
            else if ui.is_key_pressed(Key::End) {
                nudge_selection = isize::MAX;
            }
        }
        
        if nudge_selection != 0 {
            let mut filtered_level_indices = Vec::with_capacity(state.levels.len());
            let mut selected_index_after_filter: isize = -1;
            
            for (i, level) in state.levels.iter().enumerate() {
                if level.is_visible {
                    filtered_level_indices.push(i);
                    if i == state.selected_index {
                        selected_index_after_filter = filtered_level_indices.len() as isize - 1;
                    }
                }
            }
            
            let n_filtered_levels = filtered_level_indices.len() as isize;
            let new_selected_index = selected_index_after_filter
                .saturating_add(nudge_selection)
                .clamp(0, (n_filtered_levels - 1).max(0));
            state.selected_index = filtered_level_indices.get(new_selected_index as usize)
                .cloned()
                .unwrap_or(0);
        }
        
        ui.table("##LevelsTable")
            .flags(TableFlags::BORDERS | TableFlags::SCROLL_Y)
            .sizing_policy(TableSizingPolicy::StretchSame)
            .outer_size([-1.0, -1.0])
            .column("Directory").done()
        .build(|ui| {
            for (i, level) in state.levels.iter().enumerate() {
                if !level.is_visible {
                    continue;
                }
                
                let is_selected = state.selected_index == i;
                if is_selected && nudge_selection != 0 {
                    ui.set_scroll_here_y(0.5);
                }
                
                ui.table_next_row();
                ui.table_next_column();
                if ui.selectable_config(&level.display_name)
                    .selected(is_selected)
                    .flags(SelectableFlags::SPAN_ALL_COLUMNS)
                    .build()
                {
                    state.selected_index = i;
                }
                if ui.is_mouse_double_clicked(MouseButton::Left)
                    && ui.is_item_clicked()
                {
                    open_level = true;
                }
            }
        });
        
        open_level
    });
    
    if open_level.unwrap_or(false)
        && let Some(level) = state.levels.get(state.selected_index)
    {
        Some(level.abs_path.clone())
    }
    else {
        None
    }
}

fn calc_rows_per_page(ui: &Ui) -> isize {
    let table_height = ui.content_region_avail_height();
    let row_height = ui.text_line_height() + 2.0 * unsafe { ui.style().cell_padding()[1] };
    (table_height / row_height) as isize
}

fn list_levels(worlds_dir: impl AsRef<Path>) -> anyhow::Result<Vec<LevelListItem>> {
    let mut levels = Vec::new();
    
    for entry in fs::read_dir(worlds_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        
        let level_path = entry.path();
        let level_name = entry.file_name().display().to_string();
        
        let map_path = level_path.join("Map.bin");
        let ini_path = level_path.join("World.ini");
        if !fs::exists(&map_path)? || !fs::exists(&ini_path)? {
            continue;
        }
        
        let level = LevelListItem {
            is_visible: true,
            abs_path: std::path::absolute(level_path)?,
            search_name: level_name.to_lowercase(),
            display_name: level_name,
        };
        levels.push(level);
    }
    
    Ok(levels)
}

fn filter_levels(levels: &mut [LevelListItem], filter: &str) {
    let filter = filter.to_ascii_lowercase();
    for level in levels {
        level.is_visible = level.search_name.contains(&filter);
    }
}
