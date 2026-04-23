// #![windows_subsystem = "windows"]

use anyhow::Result;
use imgui_app::{imgui, platform_init, imgui_init, renderer_init, run};
use imgui::{Condition, StyleVar};

fn main() -> Result<()> {
    env_logger::init();
    
    let platform = platform_init("ksmap", (1024, 768))?;
    let renderer = renderer_init(&platform.window, platform.window.size())?;
    let imgui = imgui_init(platform, renderer);
    
    run(imgui, |ui, ex| {
        let (width, height) = ex.window.size();
        let _window_padding = ui.push_style_var(StyleVar::WindowPadding([8.0, 8.0]));
        let _window = ui.window("Main")
            .position([0.0, 0.0], Condition::Always)
            .size([width as f32, height as f32], Condition::Always)
            .title_bar(false)
            .movable(false)
            .resizable(false)
            .begin();
    });

    Ok(())
}
