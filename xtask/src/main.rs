mod paths;

use std::{collections::BTreeMap, env, fs, path::{Path, PathBuf}};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, Args};
use ksmap::{
    analysis,
    definitions,
    drawing::{self, DrawContext, DrawOptions, export_canvas_multithreaded},
    graphics::Graphics,
    partition::{GridPartitioner, Partitioner},
    screen_map::ScreenMap,
    seed::MapSeed,
    synchronization::{SyncOptions, WorldSync},
};
use libks::{map_bin, world_ini};
use serde::{Deserialize, Serialize};

use paths::*;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Subcommand, Clone)]
enum Task {
    MakeSeeds(MakeSeedsArgs),
    Render(RenderArgs),
}

#[derive(Args, Clone)]
struct MakeSeedsArgs {
    /// How many seeds per level to create
    #[arg(short, default_value = "3")]
    n: usize,
    #[arg(short, long)]
    /// Pick new random seeds instead of reusing old ones
    replace: bool,
    /// Glob pattern for level directory names (relative to Worlds)
    #[arg(default_value = "*")]
    glob: String,
}

#[derive(Args, Clone)]
struct RenderArgs {
    /// Glob pattern for level directory names (relative to Worlds)
    #[arg(default_value = "*")]
    glob: String,
    /// Path to output directory
    #[arg(default_value = "output")]
    output_dir: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.task {
        Task::MakeSeeds(args) => make_seeds(args),
        Task::Render(args) => render(args),
    }
}

fn list_level_names(glob: String) -> Result<Vec<String>> {
    if glob.contains(['/', '\\']) {
        bail!("Glob pattern should not contain a slash");
    }
    let glob = glob::glob(&glob)?;
    
    let current_dir = env::current_dir()?;
    env::set_current_dir(WORLDS_DIR.as_path())?;
    
    let mut level_names = Vec::<String>::new();
    for path in glob {
        let path = path?;
        if path.is_dir()
            && let Some(level_name) = path.to_str()
        {
            level_names.push(level_name.to_owned());
        }
    }
    
    env::set_current_dir(current_dir)?;
    Ok(level_names)
}

#[derive(Serialize, Deserialize, Default)]
struct SeedIndexEntry {
    seeds: Vec<MapSeed>,
}

fn load_seed_index() -> Result<BTreeMap<String, SeedIndexEntry>> {
    let seed_index = if SEED_INDEX_PATH.exists() {
            let contents = std::fs::read_to_string(SEED_INDEX_PATH.as_path())?;
            toml::from_str(&contents)?
        }
        else {
            BTreeMap::new()
        };
    Ok(seed_index)
}

fn make_seeds(args: MakeSeedsArgs) -> Result<()> {
    let level_names = list_level_names(args.glob)?;
    let mut seed_index = load_seed_index()?;
    
    for level_name in level_names {
        let index_entry = seed_index.entry(level_name.clone())
            .or_insert_with(|| SeedIndexEntry::default());
        let seeds = &mut index_entry.seeds;
        
        if args.replace {
            seeds.clear();
        }
        else {
            seeds.truncate(args.n);
        }
        
        while seeds.len() < args.n {
            seeds.push(MapSeed::random());
        }
        
        let level_dir = WORLDS_DIR.join(&level_name);
        let output_dir = SEEDS_DIR.join(&level_name);
        if output_dir.exists() {
            std::fs::remove_dir_all(&output_dir)?;
        }
        std::fs::create_dir_all(&output_dir)?;
        
        render_seeds(&level_dir, &seeds, &output_dir, &level_name);
    }
    
    let seed_index_serialized = toml::to_string_pretty(&seed_index)?;
    fs::write(SEED_INDEX_PATH.as_path(), seed_index_serialized)?;
    
    Ok(())
}

fn render(args: RenderArgs) -> Result<()> {
    let level_names = list_level_names(args.glob)?;
    let seed_index = load_seed_index()?;
    
    for level_name in level_names {
        let Some(index_entry) = seed_index.get(&level_name) else {
            continue
        };
        
        let level_dir = WORLDS_DIR.join(&level_name);
        let output_dir = args.output_dir.join(&level_name);
        if output_dir.exists() {
            std::fs::remove_dir_all(&output_dir)?;
        }
        std::fs::create_dir_all(&output_dir)?;
        
        render_seeds(&level_dir, &index_entry.seeds, &output_dir, &level_name);
    }
    
    Ok(())
}

fn render_seeds(level_dir: &Path, seeds: &[MapSeed], output_dir: &Path, level_name: &str) {
    let ini = world_ini::load_ini_from_dir(&level_dir)
        .expect("World.ini should be valid");
    let screens = map_bin::parse_map_file(level_dir.join("Map.bin"))
        .expect("Map.bin should be valid");
    
    let mut object_defs = definitions::load_object_defs(DEFINITIONS_PATH.as_path())
        .expect("Object definitions should be valid");
    definitions::insert_custom_obj_defs(&mut object_defs, &ini);
    
    let mut gfx = Graphics::new(
        DATA_DIR.as_path(),
        &level_dir,
        TEMPLATES_DIR.as_path(),
        &object_defs,
    );
    let assets_used = analysis::list_assets(&screens, &object_defs);
    
    gfx.load_tilesets(&assets_used.tilesets)
        .expect("IO error or corrupt image while loading tilesets");
    gfx.load_gradients(&assets_used.gradients)
        .expect("IO error or corrupt image while loading gradients");
    gfx.load_objects(&assets_used.objects)
        .expect("IO error or corrupt image while loading objects");
    
    let screen_map = ScreenMap::new(screens);
    
    let strategy = GridPartitioner::default();
    let partitions = strategy.partitions(&screen_map);
    assert!(partitions.len() == 1);
    let partition = &partitions[0];
    
    let draw_options = DrawOptions {
        editor_only: false,
    };
    let sync_options = SyncOptions {
        maximize_visible_lasers: true,
    };
    
    for seed in seeds.iter().cloned() {
        println!("{seed} {level_name}");
        
        let world_sync = WorldSync::new(seed, &screen_map, &object_defs, &sync_options);
        
        let draw_context = DrawContext {
            seed,
            screens: &screen_map,
            gfx: &gfx,
            defs: &object_defs,
            ini: &ini,
            world_sync: &world_sync,
            options: draw_options,
        };
        
        let canvas = drawing::draw_partition(draw_context, partition)
            .expect("IO error while drawing map");
        
        let output_path = output_dir.join(format!("{seed}.png"));
        export_canvas_multithreaded(canvas, &output_path)
            .expect("Error while exporting map");
    }
}
