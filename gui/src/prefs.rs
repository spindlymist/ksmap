use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Preferences {
    pub output_dir: Option<PathBuf>,
    #[serde(default = "default_allow_overwrite")]
    pub allow_overwrite: bool,
    #[serde(default = "default_use_true_aspect_ratio")]
    pub use_true_aspect_ratio: bool,
    #[serde(default = "default_draw_gridlines")]
    pub draw_gridlines: bool,
    #[serde(default = "default_show_screen_coords")]
    pub show_screen_coords: bool,
    #[serde(default = "default_show_screen_size")]
    pub show_screen_size: bool,
    #[serde(default = "default_generate_mipmaps")]
    pub generate_mipmaps: bool,
    #[serde(default = "default_start_maximized")]
    pub start_maximized: bool,
}

const fn default_allow_overwrite() -> bool { false }
const fn default_use_true_aspect_ratio() -> bool { false }
const fn default_draw_gridlines() -> bool { true }
const fn default_show_screen_coords() -> bool { true }
const fn default_show_screen_size() -> bool { false }
const fn default_generate_mipmaps() -> bool { false }
const fn default_start_maximized() -> bool { false }

impl Default for Preferences {
    fn default() -> Self {
        Self {
            output_dir: None,
            allow_overwrite: default_allow_overwrite(),
            use_true_aspect_ratio: default_use_true_aspect_ratio(),
            draw_gridlines: default_draw_gridlines(),
            show_screen_coords: default_show_screen_coords(),
            show_screen_size: default_show_screen_size(),
            generate_mipmaps: default_generate_mipmaps(),
            start_maximized: default_start_maximized(),
        }
    }
}

pub fn get_default_prefs_path() -> anyhow::Result<PathBuf> {
    let exe_path = std::env::current_exe()?;
    let ksmap_dir = exe_path.parent().unwrap();
    Ok(ksmap_dir.join("ksmap.toml"))
}

pub fn load_preferences_from_path(path: impl AsRef<Path>) -> anyhow::Result<Preferences> {
    let contents = fs::read_to_string(path)?;
    let prefs: Preferences = toml::from_str(&contents)?;
    Ok(prefs)
}

pub fn load_preferences() -> anyhow::Result<Preferences> {
    let path = get_default_prefs_path()?;
    load_preferences_from_path(path)
}

pub fn save_preferences_to_path(path: impl AsRef<Path>, prefs: &Preferences) -> anyhow::Result<()> {
    let contents = toml::to_string_pretty(prefs)?;
    fs::write(path, contents)?;
    Ok(())
}

pub fn save_preferences(prefs: &Preferences) -> anyhow::Result<()> {
    let path = get_default_prefs_path()?;
    save_preferences_to_path(path, prefs)
}
