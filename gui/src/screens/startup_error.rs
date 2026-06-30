use imgui_app::Extras;
use imgui_app::dear_imgui_rs::Ui;

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

pub fn build_ui(ui: &Ui, _ex: Extras, state: &mut State) {
    ui.text(&state.error_message);
}
