use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Preferences {
    pub output_dir: Option<PathBuf>,
    #[serde(default)]
    pub use_true_aspect_ratio: bool,
    #[serde(default)]
    pub draw_gridlines: bool,
    #[serde(default)]
    pub show_screen_size: bool,
    #[serde(default)]
    pub generate_mipmaps: bool,
    #[serde(default)]
    pub start_maximized: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            output_dir: None,
            use_true_aspect_ratio: false,
            draw_gridlines: true,
            show_screen_size: false,
            generate_mipmaps: false,
            start_maximized: false,
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
