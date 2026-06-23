// #![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod ui_extensions;
mod screens {
    pub mod level_list;
    pub mod level_map;
}
use screens::*;

use anyhow::Result;
use imgui_app::{imgui_init, platform_init, renderer_init, run};
use imgui_app::dear_imgui_rs::{Condition, ConfigFlags, WindowFlags};

struct App {
    screen: Screen,
}

enum Screen {
    LevelList(level_list::State),
    LevelMap(level_map::State),
}

fn main() -> Result<()> {
    env_logger::init();
    
    let platform = platform_init("ksmap", (1280, 768))?;
    let renderer = renderer_init(&platform.window, platform.window.size())?;
    let mut imgui = imgui_init(platform, renderer);
    
    // Enable docking
    {
        let io = imgui.imgui.io_mut();
        io.set_config_flags(io.config_flags() | ConfigFlags::DOCKING_ENABLE);
    }
    
    // Global styles
    {
        let style = imgui.imgui.style_mut();
        style.set_window_padding([8.0, 8.0]);
        style.set_window_border_size(0.0);
    }
    
    let mut app = init_app();
    
    run(imgui, |ui, ex| {
        match &mut app.screen {
            Screen::LevelList(state) => level_list::build_ui(ui, ex, state),
            Screen::LevelMap(state) => level_map::build_ui(ui, ex, state),
        }
    });

    Ok(())
}

fn init_app() -> App {
    let level_dir = "D:/Dropbox/Nifflas/Knytt Stories/Worlds/Nifflas - A Strange Dream";
    // let level_dir = "D:/Dropbox/Nifflas/Knytt Stories/Worlds/Robin O'Connell - Alexandra's Birthday";
    App {
        screen: Screen::LevelMap(level_map::State::new(level_dir.into()))
    }
}
