use imgui_app::ImguiExt;
use imgui_app::dear_imgui_rs::{StyleVar, Ui};

pub trait UiExt {
    fn text_aligned_right<S: AsRef<str>>(&self, text: S);
    fn text_aligned_center<S: AsRef<str>>(&self, text: S);
    fn checkbox_small<S: AsRef<str>>(&self, label: S, checked: &mut bool) -> bool;
    fn calc_text_width<S: AsRef<str>>(&self, text: S) -> f32;
}

impl UiExt for Ui {
    fn text_aligned_right<S: AsRef<str>>(&self, text: S) {
        let [width, _] = self.calc_text_size(text.as_ref());
        self.align_next_item_right(width);
        self.text(text);
    }
    
    fn text_aligned_center<S: AsRef<str>>(&self, text: S) {
        let [width, _] = self.calc_text_size(text.as_ref());
        self.align_next_item_center(width);
        self.text(text);
    }
    
    fn checkbox_small<S: AsRef<str>>(&self, label: S, checked: &mut bool) -> bool {
        self.set_cursor_pos_y(self.cursor_pos_y() + unsafe { self.style().frame_padding()[1] });
        let _padding = self.push_style_var(StyleVar::FramePadding([0.0, 0.0]));
        self.checkbox(label, checked)
    }
    
    fn calc_text_width<S: AsRef<str>>(&self, text: S) -> f32 {
        self.calc_text_size(text.as_ref())[0]
    }
}
