mod paths;

use std::{collections::HashMap, sync::LazyLock};

use image::ImageReader;
use ksmap::{
    analysis,
    definitions,
    drawing::{self, DrawContext, DrawOptions},
    graphics::Graphics,
    partition::{GridPartitioner, Partitioner},
    screen_map::ScreenMap,
    seed::MapSeed,
    synchronization::{SyncOptions, WorldSync},
};
use libks::{map_bin, world_ini};
use serde::Deserialize;

use paths::*;

#[derive(Deserialize)]
struct SeedIndexEntry {
    seeds: Vec<MapSeed>,
}

static SEED_INDEX: LazyLock<HashMap<String, SeedIndexEntry>> = LazyLock::new(|| {
    let contents = std::fs::read_to_string(SEED_INDEX_PATH.as_path())
        .expect("IO error while reading seed index");
    toml::from_str(&contents)
        .expect("index.toml should be valid")
});

fn verify_seeds(level_name: &str, seeds: &[MapSeed]) {
    let level_dir = WORLDS_DIR.join(level_name);
    
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
        
        let actual = drawing::draw_partition(draw_context, partition)
            .expect("IO error while drawing map");
        
        let expected_path = SEEDS_DIR.join(format!("{level_name}/{seed}.png"));
        let expected = ImageReader::open(expected_path)
            .expect("IO error while opening reference")
            .decode()
            .expect("IO error or corrupt image while decoding reference")
            .into_rgba8();
        
        assert!(expected == actual, "Seed {seed} did not match for {level_name}");
    }
}

macro_rules! test_case {
    ($test_name:ident, $level_name:literal) => {
        #[test]
        fn $test_name() {
            let entry = SEED_INDEX.get($level_name).expect("Test was missing from index");
            verify_seeds($level_name, &entry.seeds);
        }
    }
}

test_case!(lit_knob_3_20_and_14_1_test, "Lit Knob - 3-20 & 14-1 Test");
test_case!(lit_knob_bank7_black_test, "Lit Knob - Bank 7 Black Test");
test_case!(lit_knob_bank7_red_test, "Lit Knob - Bank 7 Red Test");
test_case!(lit_knob_bank7_test, "Lit Knob - Bank 7 Test");
test_case!(lit_knob_bubble_test, "Lit Knob - Bubble Test");
test_case!(lit_knob_co_frame_size_test, "Lit Knob - CO Frame Size Test");
test_case!(lit_knob_co_speed_test, "Lit Knob - CO Speed Test");
test_case!(lit_knob_combination_test, "Lit Knob - Combination Test");
test_case!(lit_knob_custom_collectables_test, "Lit Knob - Custom Collectables Test");
test_case!(lit_knob_duplicate_co_ini_test, "Lit Knob - Duplicate CO Ini Test");
test_case!(lit_knob_global_sync_test, "Lit Knob - Global Sync Test");
test_case!(lit_knob_laser_visibility_test, "Lit Knob - Laser Visibility Test");
test_case!(lit_knob_layer_test, "Lit Knob - Layer Test");
test_case!(lit_knob_object_test, "Lit Knob - Object Test");
test_case!(lit_knob_out_of_range_test, "Lit Knob - Out of Range Test");
test_case!(lit_knob_shift_test, "Lit Knob - Shift Test");
test_case!(lit_knob_starting_frame_test, "Lit Knob - Starting Frame Test");
test_case!(lit_knob_supported_oco_test, "Lit Knob - Supported OCO Test");
test_case!(lit_knob_umbrella_and_keys_test, "Lit Knob - Umbrella and Keys Test");
test_case!(lit_knob_wrong_co_resolution_test, "Lit Knob - Wrong CO Resolution Test");
test_case!(robin_horizontal_flip_test, "Robin - Horizontal Flip Test");
test_case!(robin_unsupported_oco_test, "Robin - Unsupported OCO Test");
