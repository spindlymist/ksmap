use imgui_app::dear_imgui_rs::{Ui, MouseButton};
use ksmap::{screen_map::ScreenMap, partition::Partition};
use libks::ScreenCoord;
use rustc_hash::FxHashMap;

pub struct MapState {
    pub center: ScreenCoord,
    pub drag_origin: Option<ScreenCoord>,
}

impl Default for MapState {
    fn default() -> Self {
        Self {
            center: (1000, 1000),
            drag_origin: None,
        }
    }
}

pub fn build_map(
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
    
    let cell_width = 10.0f32;
    let cell_height = 10.0f32;
    let line_thickness = 1.0f32;
    let cell_width_outer = cell_width + line_thickness;
    let cell_height_outer = cell_height + line_thickness;

    let [width_avail, height_avail] = ui.get_content_region_avail();
    let cols = (width_avail / (cell_width_outer)).ceil() as i32;
    let rows = (height_avail / (cell_height_outer)).ceil() as i32;
    
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
        let new_x = origin_x - (dx / (cell_width_outer)) as i32;
        let new_y = origin_y - (dy / (cell_height_outer)) as i32;
        map_state.center = (new_x, new_y);
    }
    
    let (x_center, y_center) = map_state.center;
    let x_min = x_center - cols / 2;
    let y_min = y_center - rows / 2;
    let x_max = x_min + cols;
    let y_max = y_min + rows;
    
    let draw_list = ui.get_window_draw_list();
    let [origin_x, origin_y] = ui.get_cursor_screen_pos();
    
    if line_thickness > 0.0 {
        for x_grid in 1..=cols {
            let x_screen = (origin_x - line_thickness) + (x_grid as f32) * (cell_width_outer);
            draw_list.add_line_v(x_screen, origin_y, origin_y + height_avail, [0.1, 0.1, 0.1], line_thickness);
        }
        for y_grid in 1..=rows {
            let y_screen = (origin_y - line_thickness) + (y_grid as f32) * (cell_height_outer);
            draw_list.add_line_h(origin_x, origin_x + width_avail, y_screen, [0.1, 0.1, 0.1], line_thickness);
        }
    }
    
    for y in y_min..y_max {
        for x in x_min..x_max {
            if screens.index_of(&(x, y)).is_none() {
                continue;
            }
            
            let dx = x - x_min;
            let dy = y - y_min;
            
            let top_left = [
                origin_x + (dx as f32) * (cell_width_outer),
                origin_y + (dy as f32) * (cell_height_outer),
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
            origin_x + top_left_dx as f32 * (cell_width_outer) - line_thickness,
            origin_y + top_left_dy as f32 * (cell_height_outer) - line_thickness,
        ];
        let bottom_right = [
            origin_x + bottom_right_dx as f32 * (cell_width_outer),
            origin_y + bottom_right_dy as f32 * (cell_height_outer),
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
    let grid_x = (dx / (cell_width_outer)) as i32;
    let grid_y = (dy / (cell_height_outer)) as i32;
    let screen_x = x_min + grid_x;
    let screen_y = y_min + grid_y;
    
    let hover_top_left = [
        origin_x + grid_x as f32 * (cell_width_outer),
        origin_y + grid_y as f32 * (cell_height_outer),
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
