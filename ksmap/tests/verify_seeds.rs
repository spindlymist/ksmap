mod paths;

use std::{collections::HashMap, sync::{Arc, LazyLock}};

use image::{ImageReader, RgbaImage};
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
    
    let object_defs = {
        let mut defs = definitions::load_object_defs(DEFINITIONS_PATH.as_path())
            .expect("Object definitions should be valid");
        definitions::insert_custom_obj_defs(&mut defs, &ini);
        Arc::new(defs)
    };
    
    let mut gfx = Graphics::new(
        DATA_DIR.as_path(),
        &level_dir,
        TEMPLATES_DIR.as_path(),
        Arc::clone(&object_defs),
    );
    let assets_used = analysis::list_assets(&screens, &object_defs);
    
    {
        let mut warnings = Vec::new();
        gfx.load_tilesets(&assets_used.tilesets, &mut warnings)
            .expect("IO error while loading tilesets");
        gfx.load_gradients(&assets_used.gradients, &mut warnings)
            .expect("IO error while loading gradients");
        gfx.load_objects(&assets_used.objects, &mut warnings)
            .expect("IO error while loading objects");
        assert!(warnings.is_empty());
    }
    
    let screen_map = ScreenMap::new(screens);
    
    let strategy = GridPartitioner::default();
    let partitions = strategy.partitions(&screen_map);
    assert!(partitions.len() == 1);
    let partition = &partitions[0];
    
    let draw_options = DrawOptions::default();
    let sync_options = SyncOptions::default();
    
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
        
        let actual = drawing::draw_partition(draw_context, partition, None)
            .expect("IO error while drawing map");
        
        let expected_path = SEEDS_DIR.join(format!("{level_name}/{seed}.png"));
        let expected = ImageReader::open(expected_path)
            .expect("IO error while opening reference")
            .decode()
            .expect("IO error or corrupt image while decoding reference")
            .into_rgba8();
        
        match compare_images(&expected, &actual) {
            CompareImagesResult::Same => { }
            CompareImagesResult::SizesDiffer => {
                panic!("Seed {seed} did not match for {level_name}: expected dimensions {:?}, got {:?}",
                    expected.dimensions(),
                    actual.dimensions());
            }
            CompareImagesResult::PixelsDiffer {
                n_pixels,
                max_error,
                max_error_pos,
                max_channel_error,
                max_channel_error_pos,
                sum_of_errors
            } => {
                println!("Level name:    {level_name}");
                println!("Seed:          {seed}");
                println!("# of pixels:   {n_pixels}");
                println!("Max error:     {max_error} at {max_error_pos:?}");
                println!("Sum of errors: {sum_of_errors}");
                for (index, channel) in ['r', 'g', 'b', 'a'].iter().enumerate() {
                    println!("Max error {channel}:   {} at {:?}", max_channel_error[index], max_channel_error_pos[index]);
                }
                panic!("Seed {seed} did not match for {level_name}: {n_pixels} pixels differed (max error: {max_error} sum: {sum_of_errors})");
            }
        }
    }
}

enum CompareImagesResult {
    Same,
    SizesDiffer,
    PixelsDiffer {
        n_pixels: usize,
        max_error: f64,
        max_error_pos: (u32, u32),
        max_channel_error: [u8; 4],
        max_channel_error_pos: [(u32, u32); 4],
        sum_of_errors: f64,
    }
}

fn compare_images(a: &RgbaImage, b: &RgbaImage) -> CompareImagesResult {
    if a.dimensions() != b.dimensions() {
        return CompareImagesResult::SizesDiffer;
    }
    
    let mut n_pixels = 0;
    let mut max_error = 0.0;
    let mut max_error_pos = (0, 0);
    let mut max_channel_error = [0, 0, 0, 0];
    let mut max_channel_error_pos = [(0, 0), (0, 0), (0, 0), (0, 0)];
    let mut sum_of_errors = 0.0;
    
    for (i, (pixel_a, pixel_b)) in a.pixels().zip(b.pixels()).enumerate() {
        if pixel_a != pixel_b {
            n_pixels += 1;
            let x = i as u32 % a.width();
            let y = i as u32 / a.width();
            let channel_errors = [
                pixel_b[0] as f64 - pixel_a[0] as f64,
                pixel_b[1] as f64 - pixel_a[1] as f64,
                pixel_b[2] as f64 - pixel_a[2] as f64,
                pixel_b[3] as f64 - pixel_a[3] as f64,
            ];
            let error = f64::sqrt(
                channel_errors[0].powi(2)
                + channel_errors[1].powi(2)
                + channel_errors[2].powi(2)
                + channel_errors[3].powi(2)
            );
            if error > max_error {
                max_error = error;
                max_error_pos = (x, y);
            }
            for channel in 0..4 {
                let channel_error = channel_errors[channel].abs() as u8;
                if channel_error > max_channel_error[channel] {
                    max_channel_error[channel] = channel_error;
                    max_channel_error_pos[channel] = (x, y);
                }
            }
            sum_of_errors += error;
        }
    }
    
    if n_pixels == 0 {
        CompareImagesResult::Same
    }
    else {
        CompareImagesResult::PixelsDiffer {
            n_pixels,
            max_error,
            max_error_pos,
            max_channel_error,
            max_channel_error_pos,
            sum_of_errors,
        }
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
test_case!(lit_knob_bank_7_black_test, "Lit Knob - Bank 7 Black Test");
test_case!(lit_knob_bank_7_red_test, "Lit Knob - Bank 7 Red Test");
test_case!(lit_knob_bank_7_test, "Lit Knob - Bank 7 Test");
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
test_case!(lit_knob_random_transparency_test, "Lit Knob - Random Transparency Test");
test_case!(lit_knob_shift_test, "Lit Knob - Shift Test");
test_case!(lit_knob_starting_frame_test, "Lit Knob - Starting Frame Test");
test_case!(lit_knob_supported_oco_test, "Lit Knob - Supported OCO Test");
test_case!(lit_knob_umbrella_and_keys_test, "Lit Knob - Umbrella and Keys Test");
test_case!(lit_knob_wrong_co_resolution_test, "Lit Knob - Wrong CO Resolution Test");
test_case!(robin_horizontal_flip_test, "Robin - Horizontal Flip Test");
test_case!(robin_unsupported_oco_test, "Robin - Unsupported OCO Test");
