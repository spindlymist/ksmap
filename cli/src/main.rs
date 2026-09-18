mod cli;
mod timing;

use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use ksmap::partition::{GridPartitioner, IslandsPartitioner, Partition, Partitioner};
use ksmap::seed::MapSeed;
use ksmap::synchronization::{SyncOptions, WorldSync};
use libks::{map_bin, world_ini};

use ksmap::{analysis, definitions};
use ksmap::drawing::{self, DrawContext, DrawOptions};
use ksmap::graphics::Graphics;
use ksmap::screen_map::ScreenMap;

use crate::cli::{Cli, GridArgs, IslandsArgs, LaserStrategy, PartitionStrategy};
use crate::timing::Timespan;

fn main() -> Result<()> {
    let mut total_time = Timespan::begin();
    
    if let Some(exe_dir) = std::env::current_exe()?.parent() {
        std::env::set_current_dir(exe_dir)?;
    }
    let cli = Cli::parse();
    
    let background_color = cli.background_color
        .map(|s| parse_hex_color(&s))
        .transpose()?;

    let seed = match cli.seed.map(MapSeed::try_from) {
        Some(Ok(seed)) => seed,
        Some(Err(err)) => {
            eprintln!("Failed to parse seed. The seed must be 1-16 hex digits (0-9 A-F).");
            return Err(err.into());
        },
        None => MapSeed::random(),
    };
    println!("Seed: {seed}");
    
    let level_dir = if cli.level.is_dir() {
            cli.level
        }
        else {
            cli.level
                .parent()
                .unwrap_or("".as_ref())
                .to_owned()
        };

    let screen_map = time_it!("Loading map", {
        let screens = map_bin::parse_map_file(level_dir.join("Map.bin"))?;
        let screen_map = ScreenMap::new(screens);
        screen_map
    });
    
    if cli.dry_run {
        println!();
        make_partitions(&screen_map,
            cli.max_width,
            cli.max_height,
            cli.partitioner,
            cli.islands_args,
            cli.grid_args,
            cli.force);
        total_time.end();
        println!();
        println!("Finished in {total_time}");
        return Ok(());
    }
    
    let ini = world_ini::load_ini_from_dir(&level_dir)?;
    
    let object_defs = time_it!("Loading definitions", {
        let mut defs = definitions::load_object_defs(cli.object_definitions)?;
        definitions::insert_custom_obj_defs(&mut defs, &ini);
        Arc::new(defs)
    });
    
    let data_dir = cli.data_dir.unwrap_or_else(|| level_dir.join("../../Data"));
    let mut gfx = Graphics::new(
        data_dir,
        &level_dir,
        &cli.templates_dir,
        Arc::clone(&object_defs),
    );
    
    let mut asset_warnings = Vec::new();
    time_it!("Loading assets", {
        let assets_used = analysis::list_assets(&screen_map, &object_defs);
        gfx.load_tilesets(&assets_used.tilesets, &mut asset_warnings)?;
        gfx.load_gradients(&assets_used.gradients, &mut asset_warnings)?;
        gfx.load_objects(&assets_used.objects, &mut asset_warnings)?;
    });
    for warning in asset_warnings {
        println!("    Warning: {warning}");
    }
    
    let world_sync = time_it!("Synchronizing map", {
        let sync_options = SyncOptions {
            maximize_visible_lasers: cli.lasers == LaserStrategy::Maximize,
        };
        WorldSync::new(seed, &screen_map, &object_defs, &sync_options)
    });
    
    println!();
    let partitions = make_partitions(&screen_map,
        cli.max_width,
        cli.max_height,
        cli.partitioner,
        cli.islands_args,
        cli.grid_args,
        cli.force);

    let draw_options = DrawOptions {
        show_invisible: cli.show_invisible,
        show_proximity: cli.show_proximity,
        trans_max_override: drawing::alpha_to_trans(cli.min_alpha),
        trans_max_threshold: cli.min_alpha_threshold,
        trans_frames: cli.alpha_sim_frames,
        tint_strategy: cli.tints.into(),
        ignore_laser_phase: cli.lasers == LaserStrategy::All,
    };
    let draw_context = DrawContext {
        seed,
        screens: &screen_map,
        gfx: &gfx,
        defs: &object_defs,
        ini: &ini,
        world_sync: &world_sync,
        options: draw_options,
    };
    
    let output_dir = cli.output_dir.unwrap_or_else(|| {
        let world_section = ini.section("World");
        let author = world_section.as_ref().and_then(|s| s.get("Author")).unwrap_or("Author");
        let name = world_section.as_ref().and_then(|s| s.get("Name")).unwrap_or("Title");
        format!("{author} - {name}")
            .replace(['<', '>', ':', '"', '/', '\\', '|', '?', '*'], "_")
            .into()
    });
    let output_is_dir = partitions.len() > 1;
    if output_is_dir {
        fs::create_dir_all(&output_dir)?;
    }
    
    println!();
    for (i, partition) in partitions.iter().enumerate() {
        let bounds = partition.bounds();
        println!("{bounds} ({}/{})", i + 1, partitions.len());
        
        let path: &Path =
            if output_is_dir {
                let file_name = format!("{bounds}.png");
                &output_dir.join(file_name)
            }
            else if output_dir.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("png")) {
                &output_dir
            }
            else {
                &output_dir.with_added_extension("png")
            };
        
        if cli.rgb {
            draw_and_export_rgb(draw_context, partition, path, cli.single_threaded_encoder, cli.compression, background_color)?;
        }
        else {
            draw_and_export(draw_context, partition, path, cli.single_threaded_encoder, cli.compression)?;
        }
    }
    println!();
    
    total_time.end();
    println!("Finished in {total_time}");

    Ok(())
}

fn draw_and_export(
    draw_context: DrawContext<'_>,
    partition: &Partition,
    output_path: &Path,
    single_threaded_encoder: bool,
    compression: u8,
) -> Result<()> {
    let canvas = time_it!("    Drawing", {
        drawing::draw_partition(draw_context, &partition)
            .inspect_err(|_| println!(" [failed]"))?
    });
        
    time_it!("    Exporting", {
        if single_threaded_encoder {
            drawing::export_canvas(canvas, output_path, compression)
                .inspect_err(|_| println!(" [failed]"))?
        }
        else {
            drawing::export_canvas_multithreaded(canvas, output_path, compression)
                .inspect_err(|_| println!(" [failed]"))?
        }
    });
    
    Ok(())
}

fn draw_and_export_rgb(
    draw_context: DrawContext<'_>,
    partition: &Partition,
    output_path: &Path,
    single_threaded_encoder: bool,
    compression: u8,
    background_color: Option<[u8; 3]>,
) -> Result<()> {
    let canvas = time_it!("    Drawing", {
        drawing::draw_partition_rgb(draw_context, &partition, background_color)
            .inspect_err(|_| println!(" [failed]"))?
    });
        
    time_it!("    Exporting", {
        if single_threaded_encoder {
            drawing::export_canvas(canvas, output_path, compression)
                .inspect_err(|_| println!(" [failed]"))?
        }
        else {
            drawing::export_canvas_multithreaded(canvas, output_path, compression)
                .inspect_err(|_| println!(" [failed]"))?
        }
    });
    
    Ok(())
}

fn make_partitions(
    screen_map: &ScreenMap,
    max_width: u64,
    max_height: u64,
    partitioner: PartitionStrategy,
    islands_args: IslandsArgs,
    grid_args: GridArgs,
    force: bool,
) -> Vec<Partition> {
    let max_size = (
        u64::max(1, max_width / 600),
        u64::max(1, max_height / 240),
    );
    
    let strategy: Box<dyn Partitioner> = match partitioner {
        PartitionStrategy::Islands => Box::new(IslandsPartitioner {
            max_size,
            gap: islands_args.min_gap..=islands_args.max_gap,
            force: force,
            fallback_to_grid: !islands_args.no_subdivide,
        }),
        PartitionStrategy::Grid => Box::new(GridPartitioner {
            max_size,
            rows: grid_args.rows,
            cols: grid_args.cols,
            force: force,
        }),
    };
    
    let partitions = time_it!("Partitioning:", {
        strategy.partitions(screen_map)
    });
    
    for (i, partition) in partitions.iter().enumerate() {
        let bounds = partition.bounds();
        println!("   {:2}: {:24} {}x{}", i + 1, bounds.to_string(), bounds.width() * 600, bounds.height() * 240);
    }
    
    partitions
}

fn parse_hex_color(s: &str) -> anyhow::Result<[u8; 3]> {
    if !s.is_ascii() {
        anyhow::bail!("Invalid hex color: {s}");
    }
    let s = if s.starts_with('#') { &s[1..] } else { s };
    
    fn parse_hex_channel(s: &str) -> anyhow::Result<u8> {
        u8::from_str_radix(s, 16)
            .map_err(|_| anyhow::anyhow!("Invalid color channel: {s}"))
    }
    
    let (r, g, b) = match s.len() {
        3 => {
            let r = parse_hex_channel(&s[0..1])?;
            let g = parse_hex_channel(&s[1..2])?;
            let b = parse_hex_channel(&s[2..3])?;
            (r << 4 | r, g << 4 | g, b << 4 | b)
        }
        6 => {
            let r = parse_hex_channel(&s[0..2])?;
            let g = parse_hex_channel(&s[2..4])?;
            let b = parse_hex_channel(&s[4..6])?;
            (r, g, b)
        }
        _ => anyhow::bail!("Invalid hex color: {s}")
    };
    
    Ok([r, g, b])
}

#[cfg(test)]
mod test {
    use super::parse_hex_color;
    
    #[test]
    fn parse_hex_color_works() {
        let test_cases = [
            ("#abcdef", [171, 205, 239]),
            ("#ABCDEF", [171, 205, 239]),
            ("abcdef", [171, 205, 239]),
            ("#abc", [170, 187, 204]),
            ("#ABC", [170, 187, 204]),
            ("abc", [170, 187, 204]),
            ("#012345", [1, 35, 69]),
            ("#678", [102, 119, 136]),
        ];
        for (input, expected) in test_cases {
            let actual = parse_hex_color(input).unwrap();
            assert_eq!(actual, expected);
        }
    }
}
