use std::{path::PathBuf, sync::LazyLock};

pub static DATA_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
        .join("Knytt Stories/Data")
});

pub static WORLDS_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
        .join("Knytt Stories/Worlds")
});

pub static TEMPLATES_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
        .join("ksmap_data/templates")
});

pub static SEEDS_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
        .join("test_seeds")
});

pub static DEFINITIONS_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
        .join("ksmap_data/object_definitions.toml")
});

pub static SEED_INDEX_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    SEEDS_DIR.join("index.toml")
});
