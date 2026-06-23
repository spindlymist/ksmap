use imgui_app::ImguiExt;
use imgui_app::dear_imgui_rs::Ui;

pub trait UiExt {
    fn text_aligned_right<S: AsRef<str>>(&self, text: S);
    fn text_aligned_center<S: AsRef<str>>(&self, text: S);
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
}
