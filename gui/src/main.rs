#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod ui_extensions;
mod screens {
    pub mod startup_error;
    pub mod level_list;
    pub mod level_map;
    pub mod loading;
}
mod map_widget;
mod name_pattern;
mod format_bytes;
mod tooltips;

use std::{fs, path::{Path, PathBuf}};

use anyhow::{Result, Context};
use imgui_app::{imgui_init, platform_init, renderer_init, run};
use imgui_app::dear_imgui_rs::{ConfigFlags, StyleColor};
use libks::editions::is_ks_dir;

use libks_ini::edit::Ini;
use screens::*;

struct App {
    ks_dir: Option<PathBuf>,
    new_title: Option<String>,
    screen: Screen,
}

enum Screen {
    StartupError(startup_error::State),
    LevelList(level_list::State),
    Loading(loading::State),
    LevelMap(level_map::State),
}

const APP_NAME: &'static str = "ksmap";

fn main() -> Result<()> {
    env_logger::init();
    let platform = platform_init(APP_NAME, (1740, 980))?;
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
        
        let [r, g, b, _] = style.color(StyleColor::PopupBg);
        style.set_color(StyleColor::PopupBg, [r, g, b, 1.0]);
        
        let [r, g, b, _] = style.color(StyleColor::WindowBg);
        style.set_color(StyleColor::WindowBg, [r, g, b, 1.0]);
    }
    
    let mut app = init_app();
    
    run(imgui, |ui, mut ex| {
        match &mut app.screen {
            Screen::StartupError(state) => startup_error::build_ui(ui, &mut ex, state),
            Screen::LevelList(state) => match level_list::build_ui(ui, &mut ex, state) {
                Some(level_dir) => {
                    let state = loading::State::new(app.ks_dir.clone(), level_dir);
                    app.screen = Screen::Loading(state);
                }
                None => {}
            }
            Screen::Loading(state) => match loading::build_ui(ui, &mut ex, state) {
                Some(loading::Task::ShowLevelMap {
                    level_dir,
                    render_state,
                    partition_state,
                }) => {
                    app.new_title = Some(get_window_title_for_level(&level_dir, &render_state.ini));
                    let state = level_map::State::new(level_dir, render_state, partition_state, app.ks_dir.is_some());
                    app.screen = Screen::LevelMap(state);
                }
                Some(loading::Task::ShowLevelList) => {
                    if let Some(ks_dir) = &app.ks_dir {
                        app.new_title = Some(APP_NAME.to_string());
                        let state = level_list::State::new(ks_dir);
                        app.screen = Screen::LevelList(state);
                    }
                }
                None => {}
            }
            Screen::LevelMap(state) => match level_map::build_ui(ui, &mut ex, state) {
                Some(level_map::Task::ShowLevelList) => {
                    if let Some(ks_dir) = &app.ks_dir {
                        level_map::on_close_screen(ui, &mut ex, state);
                        app.new_title = Some(APP_NAME.to_string());
                        let state = level_list::State::new(ks_dir);
                        app.screen = Screen::LevelList(state);
                    }
                }
                Some(level_map::Task::Exit) => {
                    return imgui_app::Task::Exit;
                }
                None => {}
            }
        }
        
        if let Some(title) = app.new_title.take() {
            let _ = ex.window.set_title(&title);
        }
        
        imgui_app::Task::None
    });

    Ok(())
}

fn init_app() -> App {
    let arg = std::env::args().nth(1).map(interpret_path_arg);
    let mut ks_dir: Option<PathBuf> = None;
    
    let screen = match arg {
        Some(Ok(PathArg::WorldPath(world_dir))) => {
            let maybe_ks_dir = world_dir.join("../..");
            if is_ks_dir(&maybe_ks_dir) {
                ks_dir = Some(maybe_ks_dir);
            }
            else if let Ok(Some(path)) = find_ks() {
                ks_dir = Some(path);
            }
            let state = loading::State::new(ks_dir.clone(), world_dir);
            Screen::Loading(state)
        }
        Some(Ok(PathArg::KsPath(path))) => {
            let state = level_list::State::new(&path);
            ks_dir = Some(path);
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
            Ok(Some(path)) => {
                let state = level_list::State::new(&path);
                ks_dir = Some(path);
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
        ks_dir,
        new_title: None,
        screen
    }
}

fn get_window_title_for_level(level_dir: &Path, ini: &Ini) -> String {
    if let (Some(author), Some(name)) = get_level_author_and_name(ini) {
        format!("{author} - {name} | {APP_NAME}")
    }
    else if let Some(dir_name) = level_dir.file_name() {
        format!("{} | {APP_NAME}", dir_name.display())
    }
    else {
        return APP_NAME.to_owned();
    }
}

pub fn get_level_author_and_name(ini: &Ini) -> (Option<&str>, Option<&str>) {
    let Some(world_section) = ini.section("World") else {
        return (None, None);
    };
    let author = world_section.get("Author");
    let level_name = world_section.get("Name");
    (author, level_name)
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
