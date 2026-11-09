use imgui_app::dear_imgui_rs::{StyleColor, Ui};

pub fn just_a_square(ui: &Ui, color: [f32; 4], filled: bool, rounded: bool) {
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

pub fn stepped_progress_bar(
    ui: &Ui,
    size: [f32; 2],
    steps: &[f32],
    current_step: usize
) {
    let [width, height] = size;
    let color_bg = ui.style_color(StyleColor::FrameBg);
    let color_finished = ui.style_color(StyleColor::PlotHistogram);
    let color_in_progress = [color_finished[0], color_finished[1], color_finished[2], 0.25];
    let line_color = color_in_progress;
    let rounding = unsafe { ui.style().frame_rounding() };
    
    let draw_list = ui.get_window_draw_list();
    let [x_start, y_start] = ui.cursor_screen_pos();
    let [x_end, y_end] = [x_start + width, y_start + height];
    let top_left = [x_start, y_start];
    let bottom_right = [x_end, y_end];
    
    let finished_pct =
        if current_step == 0 { 0.0 }
        else if current_step >= steps.len() { 1.0 }
        else { steps[current_step - 1] };
    
    if finished_pct < 1.0 {
        // Background
        draw_list.add_rect(top_left, bottom_right, color_bg)
            .rounding(rounding)
            .filled(true)
            .build();
        
        // In progress
        let in_progress_pct = steps[current_step];
        let in_progress_width = (in_progress_pct * width).round();
        let in_progress_bottom_right = [x_start + in_progress_width, y_end];
        draw_list.add_rect(top_left, in_progress_bottom_right, color_in_progress)
            .rounding(rounding)
            .filled(true)
            .build();
    }
    
    // Finished
    if finished_pct > 0.0 {
        let finished_width = (finished_pct * width).round();
        let finished_bottom_right = [x_start + finished_width, y_end];
        draw_list.add_rect(top_left, finished_bottom_right, color_finished)
            .rounding(rounding)
            .filled(true)
            .build();
    }
    
    let mut step_width_prev = 0.0;
    for step in steps.iter().skip(current_step + 1) {
        let step_width = (step * width).round();
        if step_width == 0.0
            || step_width == step_width_prev
            || step_width == width
        {
            continue;
        }
        step_width_prev = step_width;
        draw_list.add_line_v(x_start + step_width, y_start, y_end, line_color, 1.0);
    }
    
    let label = format!("{:.1}%", finished_pct * 100.0);
    let [label_width, label_height] = ui.calc_text_size(&label);
    let label_x = (x_start + (width - label_width) / 2.0).round();
    let label_y = (y_start + (height - label_height) / 2.0).round();
    draw_list.add_text([label_x, label_y], ui.style_color(StyleColor::Text), &label);
    
    ui.dummy(size);
}
