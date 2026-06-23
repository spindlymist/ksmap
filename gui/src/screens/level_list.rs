use imgui_app::Extras;
use imgui_app::dear_imgui_rs::{Condition, Ui, WindowFlags};

pub struct State {
}

pub fn build_ui(ui: &Ui, ex: Extras, state: &mut State) {
    let (width, height) = ex.window.size();
    ui.window("Main")
        .position([0.0, 0.0], Condition::Always)
        .size([width as f32, height as f32], Condition::Always)
        .flags(WindowFlags::NO_TITLE_BAR | WindowFlags::NO_MOVE | WindowFlags::NO_RESIZE)
    .build(|| {
    });
}
