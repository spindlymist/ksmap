use std::{fs, path::{Path, PathBuf}};

use imgui_app::{Extras, dear_imgui_rs::{MouseButton, SelectableFlags, StyleColor, StyleVar}};
use imgui_app::dear_imgui_rs::{Condition, Ui, TableFlags, TableSizingPolicy, WindowFlags};

pub struct State {
    levels: Vec<LevelListItem>,
    filter: String,
    selected_index: usize,
}

struct LevelListItem {
    abs_path: PathBuf,
    display_name: String,
    search_name: String,
    is_visible: bool,
}

impl State {
    pub fn new(ks_dir: PathBuf) -> Self {
        let worlds_dir = ks_dir.join("Worlds");
        let levels = list_levels(worlds_dir).unwrap();
        Self {
            levels,
            filter: String::new(),
            selected_index: 0,
        }
    }
}

pub fn build_ui(ui: &Ui, ex: Extras, state: &mut State) -> Option<PathBuf> {
    let (width, height) = ex.window.size();
    let open_level = ui.window("Main")
        .position([0.0, 0.0], Condition::Always)
        .size([width as f32, height as f32], Condition::Always)
        .flags(WindowFlags::NO_TITLE_BAR | WindowFlags::NO_MOVE | WindowFlags::NO_RESIZE)
    .build(|| {
        let mut open_level = false;
        
        // Filter
        {
            let spacing_y = unsafe { ui.style().item_spacing()[1] };
            let _spacing = ui.push_style_var(StyleVar::ItemSpacing([spacing_y, spacing_y]));
            
            let mut hint_color = ui.style_color(StyleColor::Text);
            hint_color[3] *= 0.25;
            let _hint_color_token = ui.push_style_color(StyleColor::TextDisabled, hint_color);
            
            let full_width = ui.content_region_avail_width();
            let input_width = full_width * (10.0 / 12.0) - spacing_y;
            ui.set_next_item_width(input_width);
            if ui.input_text("##Level filter", &mut state.filter)
                .hint("Filter levels")
                .build()
            {
                filter_levels(&mut state.levels, &state.filter);
            }

            ui.same_line();
            open_level |= ui.button_with_size("Open", [-1.0, 0.0]);
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
                
                ui.table_next_row();
                ui.table_next_column();
                if ui.selectable_config(&level.display_name)
                    .selected(state.selected_index == i)
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

fn list_levels(worlds_dir: impl AsRef<Path>) -> anyhow::Result<Vec<LevelListItem>> {
    let mut levels = Vec::new();
    
    for entry in fs::read_dir(worlds_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        
        let level_path = entry.path();
        let level_name = entry.file_name().to_string_lossy().into_owned();
        
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
