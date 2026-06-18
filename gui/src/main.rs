// #![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use anyhow::Result;
use imgui_app::{dear_imgui_rs, platform_init, imgui_init, renderer_init, run};
use dear_imgui_rs::{Condition, WindowFlags};

fn main() -> Result<()> {
    env_logger::init();
    
    let platform = platform_init("ksmap", (1024, 768))?;
    let renderer = renderer_init(&platform.window, platform.window.size())?;
    let mut imgui = imgui_init(platform, renderer);
    
    // Global styles
    let style = imgui.imgui.style_mut();
    style.set_window_padding([8.0, 8.0]);
    style.set_window_border_size(0.0);
    
    run(imgui, |ui, ex| {
        let (width, height) = ex.window.size();
        ui.window("Main")
            .position([0.0, 0.0], Condition::Always)
            .size([width as f32, height as f32], Condition::Always)
            .flags(WindowFlags::NO_TITLE_BAR | WindowFlags::NO_MOVE | WindowFlags::NO_RESIZE)
        .build(|| {
            
        });
        
        ui.window("Rage")
        .build(|| {});
    });

    Ok(())
}
