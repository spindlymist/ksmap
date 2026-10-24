use imgui_app::Extras;
use imgui_app::dear_imgui_rs::{Condition, Ui, WindowFlags};

use crate::prefs::Preferences;

pub struct State {
    error_message: String
}

impl State {
    pub fn new(error: anyhow::Error) -> Self {
        Self {
            error_message: error.to_string(),
        }
    }
}

pub fn build_ui(ui: &Ui, ex: &mut Extras, state: &mut State, _prefs: &mut Preferences) {
    let (width, height) = ex.window.size();
    ui.window("Main")
        .position([0.0, 0.0], Condition::Always)
        .size([width as f32, height as f32], Condition::Always)
        .flags(WindowFlags::NO_TITLE_BAR | WindowFlags::NO_MOVE | WindowFlags::NO_RESIZE)
    .build(|| {
        ui.text(&state.error_message);
    });
}
