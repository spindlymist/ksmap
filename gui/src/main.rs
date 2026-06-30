// #![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod ui_extensions;
mod screens {
    pub mod startup_error;
    pub mod level_list;
    pub mod level_map;
}
mod map_widget;

use std::{fs, path::{Path, PathBuf}};

use anyhow::{Result, Context};
use imgui_app::{imgui_init, platform_init, renderer_init, run};
use imgui_app::dear_imgui_rs::ConfigFlags;
use libks::editions::is_ks_dir;

use screens::*;

struct App {
    screen: Screen,
}

enum Screen {
    StartupError(startup_error::State),
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
        style.set_frame_rounding(2.0);
    }
    
    let mut app = init_app();
    
    run(imgui, |ui, ex| {
        match &mut app.screen {
            Screen::StartupError(state) => startup_error::build_ui(ui, ex, state),
            Screen::LevelList(state) => match level_list::build_ui(ui, ex, state) {
                Some(level_dir) => {
                    let state = level_map::State::new(level_dir);
                    app.screen = Screen::LevelMap(state);
                }
                None => {}
            }
            Screen::LevelMap(state) => level_map::build_ui(ui, ex, state),
        }
    });

    Ok(())
}

fn init_app() -> App {
    let arg = std::env::args().nth(1).map(interpret_path_arg);
    let screen = match arg {
        Some(Ok(PathArg::WorldPath(world_dir))) => {
            let state = level_map::State::new(world_dir);
            Screen::LevelMap(state)
        }
        Some(Ok(PathArg::KsPath(ks_path))) => {
            let state = level_list::State::new(ks_path);
            Screen::LevelList(state)
        }
        Some(Ok(PathArg::Unrecognized)) => {
            let err = anyhow::anyhow!("The path argument you provided could not be recognized. The path argument must \
                be a .bin file, level directory, or KS directory.");
            let state = startup_error::State::new(err);
            Screen::StartupError(state)
        }
        Some(Err(err)) => {
            let state = startup_error::State::new(err);
            Screen::StartupError(state)
        }
        None => match find_ks() {
            Ok(Some(ks_path)) => {
                let state = level_list::State::new(ks_path);
                Screen::LevelList(state)
            }
            Ok(None) => {
                let err = anyhow::anyhow!("To use this program, place it in your KS directory or one of its \
                    subdirectories, such as 3rd Party Tools. Alternatively, you can drag a .bin, level directory, \
                    or KS directory onto it.");
                let state = startup_error::State::new(err);
                Screen::StartupError(state)
            }
            Err(err) => {
                let state = startup_error::State::new(err);
                Screen::StartupError(state)
            }
        }
    };
    
    App {
        screen
    }
}

enum PathArg {
    KsPath(PathBuf),
    WorldPath(PathBuf),
    Unrecognized,
}

fn interpret_path_arg(arg: String) -> Result<PathArg> {
    let arg = PathBuf::from(arg);
    let meta = fs::metadata(&arg)?;
    
    if meta.is_dir() {
        if is_level_dir(&arg)? {
            return Ok(PathArg::WorldPath(arg));
        }
        else if is_ks_dir(&arg) {
            return Ok(PathArg::KsPath(arg));
        }
    }
    else if meta.is_file() {
        let parent = arg.parent()
            .unwrap_or(".".as_ref())
            .to_owned();
        if is_level_dir(&parent)? {
            return Ok(PathArg::WorldPath(parent));
        }
    }
    
    Ok(PathArg::Unrecognized)
}

fn find_ks() -> Result<Option<PathBuf>> {
    let mut maybe_ks_dir = std::env::current_dir()
        .context("Failed to get current directory")?;
        
    while !is_ks_dir(&maybe_ks_dir) {
        if !maybe_ks_dir.pop() {
            // Reached root without finding KS
            return Ok(None);
        }
    }
    
    Ok(Some(maybe_ks_dir.to_owned()))
}

fn is_level_dir(path: impl AsRef<Path>) -> std::io::Result<bool> {
    Ok(
        is_file(path.as_ref().join("Map.bin"))?
        && is_file(path.as_ref().join("World.ini"))?
    )
}

fn is_file(path: impl AsRef<Path>) -> std::io::Result<bool> {
    match fs::metadata(path) {
        Ok(meta) => Ok(meta.is_file()),
        Err(err) => match err.kind() {
            std::io::ErrorKind::NotFound => Ok(false),
            _ => Err(err)
        }
    }
}
