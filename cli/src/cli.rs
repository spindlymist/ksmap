use std::path::PathBuf;

use clap::{Args, Parser};
use ksmap::drawing::TintStrategy;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// 64-bit RNG seed. Must be between 1 and 16 hexadecimal digits
    #[arg(short = 's', long)]
    pub seed: Option<String>,
    /// The maximum width of a single output image in pixels
    #[arg(short = 'x', long, default_value = "60000")]
    pub max_width: u64,
    /// The maximum height of a single output image in pixels
    #[arg(short = 'y', long, default_value = "48000")]
    pub max_height: u64,
    /// How to divide large maps that don't fit into one image
    #[arg(value_enum, short = 'p', long, default_value = "islands")]
    pub partitioner: PartitionStrategy,
    /// Force the partitioner to be used even if the map fits in one image
    #[arg(short = 'f', long)]
    pub force: bool,
    /// Print the partition list, but don't render the map
    #[arg(long)]
    pub dry_run: bool,
    #[command(flatten)]
    pub islands_args: IslandsArgs,
    #[command(flatten)]
    pub grid_args: GridArgs,
    /// Draw editor icons for invisible objects
    #[arg(long, visible_alias("invis"))]
    pub show_invisible: bool,
    /// Draw objects that are only visible when Juni is nearby
    #[arg(long, visible_alias("prox"))]
    pub show_proximity: bool,
    /// How to handle screen tints.
    #[arg(long, default_value = "ignore")]
    pub tints: TintStrategyCli,
    /// Always pick a random laser phase (red/green) rather than the one with the most lasers
    #[arg(long)]
    pub randomize_lasers: bool,
    /// The minimum alpha value (0-255) for objects that have random opacity.
    /// Helps ensure objects such as ghosts are visible on the map.
    #[arg(long, default_value = "12")]
    pub min_alpha: u8,
    /// How many copies of an object a screen must have to ignore the `--min-alpha` argument.
    /// This allows for more natural variation when an object appears many times on one screen.
    #[arg(long, default_value = "5")]
    pub min_alpha_threshold: u32,
    /// How many game frames to simulate for objects that have random opacity (50 = 1 second).
    #[arg(long, default_value = "150")]
    pub alpha_sim_frames: u32,
    /// Don't use the multithreaded PNG encoder
    #[arg(long)]
    pub single_threaded_encoder: bool,
    /// Path to the KS data directory.
    /// If unspecified, it will be located relative to the level directory
    #[arg(long = "data", help_heading = "Paths")]
    pub data_dir: Option<PathBuf>,
    /// Path to the directory containing object templates
    #[arg(long = "templates", default_value = "Mapper Templates", help_heading = "Paths")]
    pub templates_dir: PathBuf,
    /// Path to the file containing object definitions
    #[arg(long = "definitions", default_value = "mapper_objects.toml", help_heading = "Paths")]
    pub object_definitions: PathBuf,
    /// The file or directory (if there are multiple partitions) to output to.
    /// If unspecified, it will be `Level Author - Level Name`
    #[arg(short, long = "output")]
    pub output_dir: Option<PathBuf>,
    /// Path to the level's directory or Map.bin
    pub level: PathBuf,
}

#[derive(Clone, Copy, Default, clap::ValueEnum)]
pub enum PartitionStrategy {
    /// Divide the map into clusters of screens that are near one another.
    /// Islands that are still too large will be subdivided using a fixed grid
    #[default]
    Islands,
    /// Divide the map into a fixed grid
    Grid,
}

#[derive(Args)]
pub struct IslandsArgs {
    /// The number of empty screens allowed between the screens of an island.
    /// -g sets min and max to the same value
    #[arg(short = 'g', long, default_value = "10", help_heading = "Islands partitioner")]
    pub max_gap: u64,
    /// If an island is too big, it will be subdivided by gradually reducing max gap no lower than this value.
    /// -g sets min and max to the same value
    #[arg(short = 'g', long, default_value = "1", help_heading = "Islands partitioner")]
    pub min_gap: u64,
}

#[derive(Args)]
pub struct GridArgs {
    /// The number of rows to divide the level into.
    /// If unspecified, it will be calculated from max height
    #[arg(short, long, help_heading = "Grid partitioner")]
    pub rows: Option<u64>,
    /// The number of columns to divide the level into.
    /// If unspecified, it will be calculated from max width
    #[arg(short, long, help_heading = "Grid partitioner")]
    pub cols: Option<u64>,
}

#[derive(Clone, Copy, Default, clap::ValueEnum)]
pub enum TintStrategyCli {
    /// Ignore screen tints.
    #[default]
    Ignore,
    /// Apply tints to screens that explicitly have one.
    Explicit,
}

impl Into<TintStrategy> for TintStrategyCli {
    fn into(self) -> TintStrategy {
        match self {
            TintStrategyCli::Ignore => TintStrategy::Ignore,
            TintStrategyCli::Explicit => TintStrategy::Explicit,
        }
    }
}
