use imgui_app::dear_imgui_rs::{MouseButton, TextureId, Ui};
use ksmap::{screen_map::ScreenMap, partition::Partition};
use libks::ScreenCoord;
use rustc_hash::FxHashMap;

pub struct MapState {
    pub opts: MapOptions,
    pub top_left: (i64, i64),
    pub is_dragging: bool,
    pub bias: (f32, f32),
    pub prev_geom: Option<MapGeometry>,
    pub selected_screen: Option<ScreenCoord>,
    pub screen_textures: FxHashMap<ScreenCoord, TextureId>,
}

impl Default for MapState {
    fn default() -> Self {
        Self {
            opts: MapOptions::default(),
            top_left: (1000, 1000),
            is_dragging: false,
            bias: (0.0, 0.0),
            prev_geom: None,
            selected_screen: None,
            screen_textures: FxHashMap::default(),
        }
    }
}

#[derive(Clone)]
pub struct MapOptions {
    pub zoom_level: i32,
    pub aspect_ratio: f32,
    pub draw_gridlines: bool,
    pub use_textures: bool,
}

impl Default for MapOptions {
    fn default() -> Self {
        Self {
            zoom_level: -10,
            aspect_ratio: 1.0,
            draw_gridlines: true,
            use_textures: false,
        }
    }
}

pub fn build_map(
    ui: &Ui,
    map_state: &mut MapState,
    screens: &ScreenMap,
    selected_partition: Option<&Partition>,
    partition_members: &FxHashMap<ScreenCoord, usize>,
    mut requested_center: Option<(i64, i64)>,
) -> Option<ScreenCoord> {
    let draw_list = ui.get_window_draw_list();
    let map_size = ui.content_region_avail();
    let [map_x_screen, map_y_screen] = ui.get_cursor_screen_pos();
    
    let (cell_width, cell_height) = get_cell_size(&map_state.opts);
    let highlight_thickness = get_line_thickness(&map_state.opts, true);
    // Coordinates need to be adjusted so the lines aren't centered on the given position
    // This will need to be updated for ImGui 1.93
    let highlight_correction = highlight_thickness / 2.0 - 0.5;
    
    // Highlights are normally the same thickness as gridlines, but we have to separate them
    // so highlights are still drawn when gridlines are disabled
    let (line_thickness, line_correction) =
        if map_state.opts.draw_gridlines {
            (highlight_thickness, highlight_correction)
        }
        else {
            (0.0, 0.0)
        };
    
    // Recenter if map was resized
    if requested_center.is_none()
        && let Some(prev_geom) = &map_state.prev_geom
        && map_size != prev_geom.size
    {
        requested_center = Some(map_get_center_screen(prev_geom));
    }
    
    // Move the top left of the map to get the desired center
    if let Some(center) = requested_center {
        map_state.top_left = (
            center.0 as i64 - (map_size[0] / 2.0 / (cell_width + line_thickness)) as i64,
            center.1 as i64 - (map_size[1] / 2.0 / (cell_height + line_thickness)) as i64,
        );
    }
    
    // Pan
    if ui.is_mouse_clicked(MouseButton::Right) && ui.is_window_hovered() {
        map_state.is_dragging = true;
    }
    let pan =
        if map_state.is_dragging {
            ui.mouse_drag_delta(MouseButton::Right).into()
        }
        else {
            (0.0, 0.0)
        };
    
    let geom = calc_map_geometry(
        map_state,
        pan,
        ui.get_content_region_avail().into(),
    );
    map_state.prev_geom = Some(geom.clone());
    
    // When panning stops, we "commit" the current geometry
    if map_state.is_dragging && ui.is_mouse_released(MouseButton::Right) {
        map_state.is_dragging = false;
        map_state.top_left = (geom.x_min, geom.y_min);
        map_state.bias = (geom.origin_x, geom.origin_y);
    }
    
    let cols = (geom.x_max - geom.x_min + 1) as usize;
    let rows = (geom.y_max - geom.y_min + 1) as usize;
    let n_grid_cells = rows * cols;
    
    // Draw grid lines
    if line_thickness > 0.0 {
        let mut x = map_x_screen + geom.origin_x + line_correction;
        for _ in 0..cols {
            draw_list.add_line_v(x, map_y_screen, map_y_screen + map_size[1], GRID_LINE_COLOR, line_thickness);
            x += geom.cell_outer_width;
        }
        let mut y = map_y_screen + geom.origin_y + line_correction;
        for _ in 0..rows {
            draw_list.add_line_h(map_x_screen, map_x_screen + map_size[0], y, GRID_LINE_COLOR, line_thickness);
            y += geom.cell_outer_height;
        }
    }
    
    // Helper functions
    let relative_to_screen_coords = |rel_pos: [f32; 2]| {
        [rel_pos[0] + map_x_screen, rel_pos[1] + map_y_screen]
    };
    let screen_to_relative_coords = |screen_pos: [f32; 2]| {
        [screen_pos[0] - map_x_screen, screen_pos[1] - map_y_screen]
    };
    let draw_rect_relative = |top_left_rel: [f32; 2], bottom_right_rel: [f32; 2], color: [f32; 4], filled: bool| {
        let top_left_screen = relative_to_screen_coords(top_left_rel);
        let bottom_right_screen = relative_to_screen_coords(bottom_right_rel);
        draw_list.add_rect(top_left_screen, bottom_right_screen, color)
            .filled(filled)
            .build();
    };
    let draw_screen = |(x, y)| {
        let cell_pos = calc_cell_pos((x as i64, y as i64), &geom);
        let top_left = [
            cell_pos[0] + line_thickness,
            cell_pos[1] + line_thickness,
        ];
        let bottom_right = [
            top_left[0] + cell_width,
            top_left[1] + cell_height
        ];
        
        if map_state.opts.use_textures {
            match map_state.screen_textures.get(&(x, y)) {
                Some(texture) => {
                    let top_left_abs = relative_to_screen_coords(top_left);
                    let bottom_right_abs = relative_to_screen_coords(bottom_right);
                    draw_list.add_image(*texture, top_left_abs, bottom_right_abs, [0.0, 0.0], [1.0, 1.0], [1.0, 1.0, 1.0]);
                }
                None => {
                    draw_rect_relative(top_left, bottom_right, MAP_COLORS[0], true);
                }
            }
        }
        else {
            let partition_index = partition_members.get(&(x, y)).unwrap();
            let color_index = *partition_index % MAP_COLORS.len();
            let color = MAP_COLORS[color_index];
            
            draw_rect_relative(top_left, bottom_right, color, true);
            
            if highlight_thickness > 0.0 {
                let highlight_color = HIGHLIGHT_COLORS[color_index];
                let mut top_left_screen = relative_to_screen_coords(top_left);
                top_left_screen[0] += highlight_correction;
                top_left_screen[1] += highlight_correction;
                let mut bottom_right_screen = relative_to_screen_coords(bottom_right);
                bottom_right_screen[0] -= highlight_correction;
                bottom_right_screen[1] -= highlight_correction;
                draw_list.add_rect(top_left_screen, bottom_right_screen, highlight_color)
                    .filled(false)
                    .thickness(highlight_thickness)
                    .build();
            }
        }
    };
    let draw_indicator = |(x, y), color| {
        let cell_pos = calc_cell_pos((x, y), &geom);
        let top_left = [
            cell_pos[0] + line_thickness,
            cell_pos[1] + line_thickness,
        ];
        let bottom_right = [
            top_left[0] + cell_width,
            top_left[1] + cell_height,
        ];
        draw_rect_relative(top_left, bottom_right, color, false);
    };
    let draw_coord_string = |(x, y): ScreenCoord| {
        let text = format!("x{}y{}", x, y);
        let text_height = ui.calc_text_size(&text)[1];
        let window_height = ui.window_height();
        ui.set_cursor_pos([5.0, window_height - 5.0 - text_height]);
        ui.text(text);
    };
    
    let set_sampler =
        map_state.opts.use_textures
        && map_state.opts.zoom_level >= 0;
    if set_sampler {
        draw_list.set_sampler_nearest();
    }
    
    // Now, we either iterate over screens (and check if they're on the map), or iterate over map cells
    // (and check if they contain a screen), whichever takes fewer iterations.
    if screens.len() <= n_grid_cells {
        for screen in screens.iter() {
            let (x, y) = screen.position;
            if x as i64 >= geom.x_min
                && x as i64 <= geom.x_max
                && y as i64 >= geom.y_min
                && y as i64 <= geom.y_max
            {
                draw_screen((x, y));
            }
        }
    }
    else {
        let x_min = geom.x_min.max(i32::MIN as i64) as i32;
        let x_max = geom.x_max.min(i32::MAX as i64) as i32;
        let y_min = geom.y_min.max(i32::MIN as i64) as i32;
        let y_max = geom.y_max.min(i32::MAX as i64) as i32;
        for y in y_min..=y_max {
            for x in x_min..=x_max {
                if screens.index_of(&(x, y)).is_some() {
                    draw_screen((x, y));
                }
            }
        }
    }
    
    if set_sampler {
        draw_list.set_sampler_linear();
    }
    
    // Draw partition outline
    if let Some(bounds) = selected_partition.map(|partition| partition.bounds())
        && !bounds.x.is_empty()
        && !bounds.y.is_empty()
    {
        let top_left = calc_cell_pos((bounds.x.start, bounds.y.start), &geom);
        let mut bottom_right = calc_cell_pos((bounds.x.end, bounds.y.end), &geom);
        bottom_right[0] += line_thickness;
        bottom_right[1] += line_thickness;
        draw_rect_relative(top_left, bottom_right, PARTITION_OUTLINE_COLOR, false);
    }
    
    // Hover and selection indicator
    if ui.is_window_hovered() {
        let mouse_pos = screen_to_relative_coords(ui.mouse_pos());
        let hovered_cell = get_hovered_screen_pos(mouse_pos, &geom);
        let hovered_screen_pos =
            if let Ok(x) = i32::try_from(hovered_cell.0)
                && let Ok(y) = i32::try_from(hovered_cell.1)
            {
                Some((x, y))
            }
            else {
                None
            };
        
        if ui.is_mouse_clicked(MouseButton::Left) {
            if let Some(pos) = &hovered_screen_pos
                && screens.index_of(&pos).is_some()
                && map_state.selected_screen != Some(*pos)
            {
                map_state.selected_screen = Some(*pos);
            }
            else {
                map_state.selected_screen = None;
            }
        }
        
        if let Some(pos) = &map_state.selected_screen {
            draw_indicator((pos.0 as i64, pos.1 as i64), SELECTION_INDICATOR_COLOR);
        }
        draw_indicator(hovered_cell, HOVER_INDICATOR_COLOR);
        if let Some(pos) = &hovered_screen_pos {
            draw_coord_string(*pos);
        }
        
        // Zoom
        let wheel_delta = ui.get_mouse_wheel();
        if wheel_delta != 0.0 && !map_state.is_dragging {
            let new_zoom_level = (map_state.opts.zoom_level + wheel_delta as i32).clamp(ZOOM_MIN, ZOOM_MAX);
            if new_zoom_level == map_state.opts.zoom_level {
                return hovered_screen_pos;
            }
            
            map_state.opts.zoom_level = new_zoom_level;
            let (new_cell_width, new_cell_height) = get_cell_size(&map_state.opts);
            let new_line_thickness = get_line_thickness(&map_state.opts, false);
            
            // The general idea here is to keep the point the mouse is hovering over in the same position as we zoom
            // We start by converting the pixel position to a proportion that is agnostic to the zoom level
            let hovered_cell_top_left = calc_cell_pos(hovered_cell, &geom);
            let mouse_pos_within_cell = [
                mouse_pos[0] - hovered_cell_top_left[0],
                mouse_pos[1] - hovered_cell_top_left[1]
            ];
            let mouse_pos_within_cell_proportion = [
                mouse_pos_within_cell[0] / geom.cell_outer_width,
                mouse_pos_within_cell[1] / geom.cell_outer_height,
            ];
            
            // Next, we use that proportion to calculate the pixel offset at the new zoom level
            let new_cell_outer_width = new_cell_width + new_line_thickness;
            let new_cell_outer_height = new_cell_height + new_line_thickness;
            let new_mouse_pos_within_cell = [
                (mouse_pos_within_cell_proportion[0] * new_cell_outer_width).round().min(new_cell_outer_width - 1.0),
                (mouse_pos_within_cell_proportion[1] * new_cell_outer_height).round().min(new_cell_outer_height - 1.0),
            ];
            
            // Working backwards, we can calculate where the top left of the hovered screen is at the new zoom level
            let new_hovered_cell_top_left = [
                mouse_pos[0] - new_mouse_pos_within_cell[0],
                mouse_pos[1] - new_mouse_pos_within_cell[1],
            ];
            
            // Since we know the map coordinates of the hovered screen and the pixel coordinates of its top left corner,
            // we can do some math to figure out the what screen appears in the top left of the map and where its
            // top left corner is.
            let blah_x = f32::ceil(new_hovered_cell_top_left[0] / new_cell_outer_width) as i64;
            let blah_y = f32::ceil(new_hovered_cell_top_left[1] / new_cell_outer_height) as i64;
            let new_top_left_screen = (
                hovered_cell.0 - blah_x,
                hovered_cell.1 - blah_y,
            );
            let new_bias = (
                new_hovered_cell_top_left[0] - blah_x as f32 * new_cell_outer_width,
                new_hovered_cell_top_left[1] - blah_y as f32 * new_cell_outer_height,
            );
            
            map_state.top_left = new_top_left_screen;
            map_state.bias = new_bias;
        }
        
        hovered_screen_pos
    }
    else if let Some(pos) = &map_state.selected_screen {
        draw_indicator((pos.0 as i64, pos.1 as i64), SELECTION_INDICATOR_COLOR);
        draw_coord_string(*pos);
        None
    }
    else {
        None
    }
}

#[derive(Clone)]
pub struct MapGeometry {
    size: [f32; 2],
    x_min: i64,
    y_min: i64,
    x_max: i64,
    y_max: i64,
    origin_x: f32,
    origin_y: f32,
    cell_outer_width: f32,
    cell_outer_height: f32,
}

fn calc_map_geometry(map_state: &MapState, pan: (f32, f32), map_size: (f32, f32)) -> MapGeometry {
    let total_bias = (map_state.bias.0 + pan.0, map_state.bias.1 + pan.1);
    
    let (cell_width, cell_height) = get_cell_size(&map_state.opts);
    let line_thickness = get_line_thickness(&map_state.opts, false);
    let cell_outer_width = cell_width + line_thickness;
    let cell_outer_height = cell_height + line_thickness;
    
    let blah_x = f32::ceil(total_bias.0 / cell_outer_width) as i64;
    let blah_y = f32::ceil(total_bias.1 / cell_outer_height) as i64;
    
    let x_min = map_state.top_left.0 - blah_x;
    let y_min = map_state.top_left.1 - blah_y;
    
    let origin_x = total_bias.0 + (x_min - map_state.top_left.0) as f32 * cell_outer_width;
    let origin_y = total_bias.1 + (y_min - map_state.top_left.1) as f32 * cell_outer_height;
    
    let x_max = x_min + ((map_size.0 - origin_x) / cell_outer_width).floor() as i64;
    let y_max = y_min + ((map_size.1 - origin_y) / cell_outer_height).floor() as i64;
    
    MapGeometry {
        size: map_size.into(),
        x_min,
        y_min,
        x_max,
        y_max,
        cell_outer_width,
        cell_outer_height,
        origin_x,
        origin_y
    }
}

fn calc_cell_pos(screen_pos: (i64, i64), geom: &MapGeometry) -> [f32; 2] {
    let dx = screen_pos.0 as i64 - geom.x_min;
    let dy = screen_pos.1 as i64 - geom.y_min;
    
    let x = geom.origin_x + dx as f32 * geom.cell_outer_width;
    let y = geom.origin_y + dy as f32 * geom.cell_outer_height;
    
    [x, y]
}

fn get_hovered_screen_pos(hover_pos: [f32; 2], geom: &MapGeometry) -> (i64, i64) {
    let offset_from_origin_x = hover_pos[0] - geom.origin_x;
    let offset_from_origin_y = hover_pos[1] - geom.origin_y;
    let screen_x = geom.x_min + (offset_from_origin_x / geom.cell_outer_width).floor() as i64;
    let screen_y = geom.y_min + (offset_from_origin_y / geom.cell_outer_height).floor() as i64;
    
    (screen_x, screen_y)
}

const ZOOM_MIN: i32 = -16;
const ZOOM_MAX: i32 = 5;
const MAG_LEVELS: [f32; 1 + ZOOM_MAX as usize] = [1.0, 2.0, 3.0, 4.0, 6.0, 8.0];

fn get_cell_size(opts: &MapOptions) -> (f32, f32) {
    let scale =
        if opts.zoom_level < 0 {
            2.0f32.powf(0.5 * opts.zoom_level as f32)
        }
        else {
            MAG_LEVELS[opts.zoom_level as usize]
        };
    let height = (240.0 * scale).round().max(1.0);
    let width = (height * opts.aspect_ratio).round().max(1.0);
    (width, height)
}

fn get_line_thickness(opts: &MapOptions, ignore_gridline_option: bool) -> f32 {
    if !opts.draw_gridlines && !ignore_gridline_option {
        return 0.0;
    }
    
    let (cell_width, cell_height) = get_cell_size(opts);
    if cell_height < 5.0 {
        0.0
    }
    else if opts.use_textures {
        1.0
    }
    else {
        (f32::min(cell_width, cell_height) * 0.04).round().max(1.0)
    }
}

pub fn map_get_center_screen(geom: &MapGeometry) -> (i64, i64) {
    let x = geom.x_min + (geom.size[0] / 2.0 / geom.cell_outer_width) as i64;
    let y = geom.y_min + (geom.size[1] / 2.0 / geom.cell_outer_height) as i64;
    (x, y)
}

type Color = [f32; 4];
const GRID_LINE_COLOR: Color = [0.1, 0.1, 0.1, 1.0];
const HOVER_INDICATOR_COLOR: Color = [1.0, 1.0, 1.0, 1.0];
const PARTITION_OUTLINE_COLOR: Color = [1.0, 1.0, 1.0, 1.0];
const SELECTION_INDICATOR_COLOR: Color = [0.98, 0.85, 0.21, 1.0];
pub const MAP_COLORS: [Color; 64] = [
    [0.000, 0.404, 0.529, 1.000],
    [0.761, 0.627, 0.180, 1.000],
    [0.263, 0.490, 0.192, 1.000],
    [0.761, 0.459, 0.027, 1.000],
    [0.290, 0.204, 0.333, 1.000],
    [0.643, 0.188, 0.161, 1.000],
    [0.133, 0.196, 0.506, 1.000],
    [0.373, 0.165, 0.227, 1.000],
    [0.514, 0.624, 0.075, 1.000],
    [0.800, 0.463, 0.584, 1.000],
    [0.373, 0.643, 0.635, 1.000],
    [0.235, 0.235, 0.235, 1.000],
    [0.384, 0.243, 0.173, 1.000],
    [0.271, 0.302, 0.243, 1.000],
    [0.714, 0.663, 0.463, 1.000],
    [0.345, 0.373, 0.427, 1.000],
    [0.141, 0.522, 0.651, 1.000],
    [0.890, 0.753, 0.329, 1.000],
    [0.376, 0.612, 0.310, 1.000],
    [0.894, 0.580, 0.227, 1.000],
    [0.400, 0.310, 0.447, 1.000],
    [0.776, 0.314, 0.275, 1.000],
    [0.227, 0.310, 0.631, 1.000],
    [0.490, 0.271, 0.333, 1.000],
    [0.631, 0.749, 0.255, 1.000],
    [0.933, 0.584, 0.706, 1.000],
    [0.494, 0.769, 0.757, 1.000],
    [0.341, 0.341, 0.341, 1.000],
    [0.502, 0.349, 0.278, 1.000],
    [0.376, 0.412, 0.349, 1.000],
    [0.839, 0.788, 0.584, 1.000],
    [0.455, 0.486, 0.545, 1.000],
    [0.000, 0.290, 0.404, 1.000],
    [0.639, 0.506, 0.000, 1.000],
    [0.145, 0.376, 0.059, 1.000],
    [0.635, 0.341, 0.000, 1.000],
    [0.188, 0.106, 0.227, 1.000],
    [0.510, 0.020, 0.039, 1.000],
    [0.055, 0.075, 0.384, 1.000],
    [0.259, 0.059, 0.129, 1.000],
    [0.400, 0.502, 0.000, 1.000],
    [0.671, 0.349, 0.467, 1.000],
    [0.251, 0.525, 0.514, 1.000],
    [0.137, 0.137, 0.137, 1.000],
    [0.275, 0.141, 0.075, 1.000],
    [0.169, 0.200, 0.145, 1.000],
    [0.592, 0.541, 0.349, 1.000],
    [0.239, 0.267, 0.318, 1.000],
    [0.286, 0.643, 0.776, 1.000],
    [1.000, 0.882, 0.463, 1.000],
    [0.494, 0.737, 0.427, 1.000],
    [1.000, 0.706, 0.365, 1.000],
    [0.514, 0.420, 0.565, 1.000],
    [0.914, 0.435, 0.388, 1.000],
    [0.329, 0.427, 0.761, 1.000],
    [0.612, 0.380, 0.443, 1.000],
    [0.753, 0.878, 0.392, 1.000],
    [1.000, 0.722, 0.831, 1.000],
    [0.620, 0.898, 0.882, 1.000],
    [0.451, 0.451, 0.451, 1.000],
    [0.624, 0.463, 0.388, 1.000],
    [0.490, 0.525, 0.463, 1.000],
    [0.969, 0.918, 0.710, 1.000],
    [0.573, 0.604, 0.667, 1.000],
];
const HIGHLIGHT_COLORS: [Color; 64] = [
    [0.106, 0.498, 0.627, 1.000],
    [0.863, 0.729, 0.302, 1.000],
    [0.353, 0.588, 0.286, 1.000],
    [0.867, 0.557, 0.196, 1.000],
    [0.376, 0.290, 0.424, 1.000],
    [0.749, 0.290, 0.251, 1.000],
    [0.208, 0.286, 0.608, 1.000],
    [0.467, 0.251, 0.314, 1.000],
    [0.608, 0.725, 0.224, 1.000],
    [0.906, 0.561, 0.682, 1.000],
    [0.471, 0.745, 0.733, 1.000],
    [0.322, 0.322, 0.322, 1.000],
    [0.478, 0.325, 0.259, 1.000],
    [0.353, 0.388, 0.325, 1.000],
    [0.816, 0.765, 0.561, 1.000],
    [0.431, 0.463, 0.522, 1.000],
    [0.259, 0.620, 0.749, 1.000],
    [0.996, 0.855, 0.435, 1.000],
    [0.471, 0.710, 0.404, 1.000],
    [1.000, 0.678, 0.337, 1.000],
    [0.490, 0.396, 0.541, 1.000],
    [0.886, 0.412, 0.365, 1.000],
    [0.310, 0.404, 0.733, 1.000],
    [0.588, 0.357, 0.424, 1.000],
    [0.729, 0.851, 0.365, 1.000],
    [1.000, 0.682, 0.808, 1.000],
    [0.592, 0.871, 0.859, 1.000],
    [0.431, 0.431, 0.431, 1.000],
    [0.600, 0.439, 0.365, 1.000],
    [0.467, 0.502, 0.439, 1.000],
    [0.941, 0.890, 0.682, 1.000],
    [0.549, 0.580, 0.639, 1.000],
    [0.000, 0.380, 0.506, 1.000],
    [0.737, 0.604, 0.149, 1.000],
    [0.239, 0.467, 0.169, 1.000],
    [0.737, 0.435, 0.000, 1.000],
    [0.271, 0.184, 0.314, 1.000],
    [0.616, 0.161, 0.141, 1.000],
    [0.118, 0.173, 0.482, 1.000],
    [0.349, 0.145, 0.208, 1.000],
    [0.490, 0.600, 0.000, 1.000],
    [0.773, 0.439, 0.561, 1.000],
    [0.349, 0.620, 0.608, 1.000],
    [0.216, 0.216, 0.216, 1.000],
    [0.365, 0.220, 0.153, 1.000],
    [0.247, 0.282, 0.224, 1.000],
    [0.690, 0.639, 0.439, 1.000],
    [0.322, 0.353, 0.408, 1.000],
    [0.392, 0.741, 0.878, 1.000],
    [1.000, 0.996, 0.812, 1.000],
    [0.592, 0.839, 0.525, 1.000],
    [1.000, 0.839, 0.639, 1.000],
    [0.608, 0.514, 0.663, 1.000],
    [1.000, 0.537, 0.482, 1.000],
    [0.420, 0.522, 0.863, 1.000],
    [0.714, 0.475, 0.537, 1.000],
    [0.855, 0.984, 0.498, 1.000],
    [1.000, 0.878, 0.933, 1.000],
    [0.722, 1.000, 0.988, 1.000],
    [0.545, 0.545, 0.545, 1.000],
    [0.722, 0.557, 0.478, 1.000],
    [0.584, 0.624, 0.553, 1.000],
    [1.000, 0.961, 0.749, 1.000],
    [0.667, 0.702, 0.765, 1.000],
];
