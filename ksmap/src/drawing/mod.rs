use std::{fs, ops::RangeInclusive, path::Path};

use anyhow::{anyhow, Result};
use image::{codecs::png::PngEncoder, imageops, GenericImage, ImageEncoder, RgbaImage};
use rand::prelude::*;
use libks::{ScreenCoord, map_bin::{LayerData, ScreenData, Tile}};
use libks_ini::{Ini, VirtualSection};

use crate::{
    definitions::{AnimSync, Flip, ObjectDef, ObjectDefs, ObjectKind, TransAlgorithm},
    graphics::{Graphics, spritesheet::Spritesheet},
    id::{ObjectId, ObjectVariant},
    ini_util::{unpack_color, VirtualSectionExt},
    partition::{Bounds, Partition},
    screen_map::ScreenMap,
    seed::{MapSeed, RngStep},
    synchronization::{ScreenSync, WorldSync}
};

mod transparency;
pub use transparency::{trans_to_alpha, alpha_to_trans};

mod blend_modes;
pub use blend_modes::BlendMode;

pub fn tileset_index_to_pixels(i: u8) -> (u32, u32) {
    let x = (i % 16) as u32 * 24;
    let y = (i / 16) as u32 * 24;
    (x, y)
}

pub fn screen_index_to_pixels(i: u8) -> (u32, u32) {
    let x = (i % 25) as u32 * 24;
    let y = (i / 25) as u32 * 24;
    (x, y)
}

#[derive(Clone, Copy)]
pub struct DrawContext<'a> {
    pub seed: MapSeed,
    pub screens: &'a ScreenMap,
    pub gfx: &'a Graphics<'a>,
    pub defs: &'a ObjectDefs,
    pub ini: &'a Ini,
    pub world_sync: &'a WorldSync,
    pub options: DrawOptions,
}

struct ScreenContext<'a> {
    seed: MapSeed,
    screen_pos: ScreenCoord,
    layer: u8,
    image: RgbaImage,
    tileset_a: Option<&'a RgbaImage>,
    tileset_b: Option<&'a RgbaImage>,
    gfx: &'a Graphics<'a>,
    defs: &'a ObjectDefs,
    ini_section: Option<VirtualSection<'a>>,
    sync: ScreenSync,
    opts: DrawOptions,
}

#[derive(Clone, Copy)]
pub struct DrawOptions {
    pub editor_only: bool,
    /// Overrides the maximum transparency for objects that have random opacity to ensure they are visible.
    /// 0 is fully opaque and 128 is fully transparent.
    pub trans_max_override: u8,
    /// The number of object instances (per screen) required to ignore the transparency override.
    pub trans_max_threshold: u32,
    /// The number of frames to simulate for transparency.
    pub trans_frames: u32,
    pub tint_strategy: TintStrategy,
}

impl Default for DrawOptions {
    fn default() -> Self {
        Self {
            editor_only: false,
            trans_max_override: 122,
            trans_max_threshold: 5,
            trans_frames: 150,
            tint_strategy: TintStrategy::Ignore,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TintStrategy {
    Ignore,
    Explicit,
}

#[derive(Debug, Clone)]
struct Cursor {
    i: usize,
    actual_id: ObjectId,
    proxy_id: ObjectId,
}

pub fn draw_partition(ctx: DrawContext, partition: &Partition) -> Result<RgbaImage> {        
    let bounds = partition.bounds();
    let mut canvas = make_canvas(&bounds)?;
    for pos in partition {
        let Some(index_screen) = ctx.screens.index_of(pos) else { continue };
        let screen = &ctx.screens[index_screen];
        match draw_screen(ctx.seed, screen, index_screen, ctx.gfx, ctx.defs, ctx.ini, ctx.options, ctx.world_sync) {
            Ok(screen_image) => {
                let canvas_x: u32 = ((screen.position.0 as i64 - bounds.x.start) * 600).try_into().unwrap();
                let canvas_y: u32 = ((screen.position.1 as i64 - bounds.y.start) * 240).try_into().unwrap();
                canvas.copy_from(&screen_image, canvas_x, canvas_y)?;
            },
            Err(err) => return Err(err),
        }
    }
    Ok(canvas)
}

fn make_canvas(bounds: &Bounds) -> Result<RgbaImage> {
    let (width, height) = bounds.size();

    let Ok(Some(width)) = u32::try_from(width)
        .map(|width| width.checked_mul(600))
    else {
        return Err(anyhow!("Partition is too large: {bounds}"));
    };

    let Ok(Some(height)) = u32::try_from(height)
        .map(|height| height.checked_mul(240))
    else {
        return Err(anyhow!("Partition {bounds} is too large"));
    };
    
    Ok(RgbaImage::new(width, height))
}

pub fn export_canvas(canvas: RgbaImage, path: &Path) -> Result<()> {
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;

    let encoder = PngEncoder::new_with_quality(
        file,
        image::codecs::png::CompressionType::Best,
        Default::default(),
    );

    let width = canvas.width();
    let height = canvas.height();
    let buf = canvas.into_vec();

    encoder.write_image(&buf, width, height, image::ExtendedColorType::Rgba8)?;

    Ok(())
}

pub fn export_canvas_multithreaded(canvas: RgbaImage, path: &Path) -> Result<()> {
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    let writer = std::io::BufWriter::new(file);
    
    let width = canvas.width();
    let height = canvas.height();
    let data = canvas.into_raw();
    
    let mut header = mtpng::Header::new();
    header.set_size(width, height)?;
    header.set_color(mtpng::ColorType::TruecolorAlpha, 8)?;
    
    let mut options = mtpng::encoder::Options::new();
    options.set_compression_level(mtpng::CompressionLevel::High)?;

    let mut encoder = mtpng::encoder::Encoder::new(writer, &options);
    encoder.write_header(&header)?;
    encoder.write_image_rows(&data)?;
    encoder.finish()?;

    Ok(())
}

pub fn draw_screen(
    seed: MapSeed,
    screen: &ScreenData,
    index_screen: usize,
    gfx: &Graphics,
    defs: &ObjectDefs,
    ini: &Ini,
    opts: DrawOptions,
    world_sync: &WorldSync,
) -> Result<RgbaImage> {
    let ini_section = ini.section(&format!("x{}y{}", screen.position.0, screen.position.1));
    let is_overlay = ini_section
        .as_ref()
        .is_some_and(|section| {
            section.get("Overlay")
                .unwrap_or("")
                .eq_ignore_ascii_case("True")
        });

    // Create context
    let group = world_sync.groups[index_screen];
    let sync = ScreenSync::new(seed, screen, defs, group, opts.trans_max_override, opts.trans_max_threshold);
    let mut ctx = ScreenContext {
        seed,
        screen_pos: screen.position,
        layer: 0,
        image: RgbaImage::new(600, 240),
        tileset_a: gfx.tileset(screen.assets.tileset_a),
        tileset_b: gfx.tileset(screen.assets.tileset_b),
        gfx,
        defs,
        ini_section,
        sync,
        opts,
    };
    
    // Draw gradient
    if let Some(gradient) = ctx.gfx.gradient(screen.assets.gradient) {
        imageops::tile(&mut ctx.image, gradient);
    }
    
    // Draw tile layers
    draw_tile_layer(&mut ctx, &screen.layers[0]);
    draw_tile_layer(&mut ctx, &screen.layers[1]);
    if !is_overlay {
        draw_tile_layer(&mut ctx, &screen.layers[2]);
    }
    draw_tile_layer(&mut ctx, &screen.layers[3]);

    // Draw object layers
    ctx.layer = 4;
    draw_object_layer(&mut ctx, &screen.layers[4]);
    ctx.layer = 5;
    draw_object_layer(&mut ctx, &screen.layers[5]);
    ctx.layer = 6;
    draw_object_layer(&mut ctx, &screen.layers[6]);
    if is_overlay {
        draw_tile_layer(&mut ctx, &screen.layers[2]);
    }
    ctx.layer = 7;
    draw_object_layer(&mut ctx, &screen.layers[7]);
    
    apply_tint(&mut ctx);

    Ok(ctx.image)
}

fn draw_tile_layer(ctx: &mut ScreenContext, layer: &LayerData) {
    for (i, tile) in layer.0.iter().enumerate() {
        if tile.1 == 0 {
            continue;
        }

        let Some(tileset) = (match tile.0 {
            0 => ctx.tileset_a,
            1 => ctx.tileset_b,
            _ => None,
        }) else {
            continue;
        };

        let (tile_x, tile_y) = tileset_index_to_pixels(tile.1);        
        let (screen_x, screen_y) = screen_index_to_pixels(i as u8);
        
        let tile_img = imageops::crop_imm(tileset, tile_x, tile_y, 24, 24);
        imageops::overlay(&mut ctx.image, &*tile_img, screen_x as i64, screen_y as i64);
    }
}

fn draw_object_layer(ctx: &mut ScreenContext, layer: &LayerData) {
    for (i, tile) in layer.0.iter().enumerate() {
        if tile.1 == 0 { continue }

        let actual_id = ObjectId::from(tile);
        let object_def = ctx.defs.get(&actual_id);
        let proxy_id = match object_def.map(|def| &def.kind) {
            Some(ObjectKind::OverrideObject(tile_original)) => ObjectId::from(tile_original),
            _ => ObjectId::from(tile),
        };
        let curs = Cursor {
            i,
            actual_id,
            proxy_id,
        };

        if ctx.sync.limiters.get_mut(&curs.proxy_id)
            .is_some_and(|limiter| !limiter.increment())
        {
            continue;
        }
        if !ctx.opts.editor_only
            && object_def.is_some_and(|object| object.editor_only)
        {
            continue;
        }
        if let Some(def) = object_def
            && let Some(phase) = &def.sync.laser_phase
            && *phase != ctx.sync.group.laser_phase
        {
            continue;
        }

        match curs.proxy_id.0 {
            Tile(0, 14) => draw_shift(ctx, curs, "ShiftVisible(A)", "ShiftType(A)"),
            Tile(0, 15) => draw_shift(ctx, curs, "ShiftVisible(B)", "ShiftType(B)"),
            Tile(0, 16) => draw_shift(ctx, curs, "ShiftVisible(C)", "ShiftType(C)"),
            Tile(0, 32) => draw_shift(ctx, curs, "TrigVisible(A)", "TrigType(A)"),
            Tile(0, 33) => draw_shift(ctx, curs, "TrigVisible(B)", "TrigType(B)"),
            Tile(0, 34) => draw_shift(ctx, curs, "TrigVisible(C)", "TrigType(C)"),
            Tile(1, 5 | 10 | 12 | 22) => draw_with_glow(ctx, curs),
            Tile(2, 18 | 19) => draw_elemental(ctx, curs),
            Tile(8, 10) => draw_with_random_offset(ctx, curs, -6..=6),
            Tile(8, 15) => draw_with_random_offset(ctx, curs, -12..=12),
            _ => draw_object(ctx, curs.i, curs.actual_id),
        }
    }
}

#[inline]
fn draw_object(
    ctx: &mut ScreenContext,
    at_index: usize,
    object: ObjectId,
) {
    draw_object_with_offset(ctx, at_index, object, (0, 0));
}

#[inline]
fn draw_object_with_offset(
    ctx: &mut ScreenContext,
    at_index: usize,
    mut id: ObjectId,
    offset: (i32, i32),
) {
    let def = match ctx.defs.get(&id) {
        Some(def) => def,
        None => &ObjectDef::default(),
    };

    let mut flip = match def.draw.flip {
        Flip::Never => false,
        Flip::Random => {
            ctx.seed.hasher(RngStep::Flip)
                .write(ctx.screen_pos)
                .write(ctx.layer)
                .write(at_index)
                .random()
        }
        Flip::Always => true,
    };
    if flip && let Some(variant) = def.draw.flip_variant {
        id = id.into_variant(variant);
        flip = false;
        // Should technically fetch the variant def here but it doesn't matter for any existing object
    }
    let Some(obj_image) = ctx.gfx.object(&id) else { return };
    
    let anim_t = match &def.sync.sync_to {
        AnimSync::None => None,
        AnimSync::Screen => Some(ctx.sync.anim_t),
        AnimSync::Group => Some(ctx.sync.group.anim_t),
    };
    draw_spritesheet(ctx, at_index as u8, id, &def, anim_t, obj_image, offset, flip);
}

fn draw_spritesheet(
    ctx: &mut ScreenContext,
    at_index: u8,
    id: ObjectId,
    def: &ObjectDef,
    anim_t: Option<u32>,
    spritesheet: &Spritesheet,
    offset: (i32, i32),
    flip: bool,
) {
    let mut frame = match anim_t {
        Some(t) => spritesheet.frame_at_time(t),
        None => {
            let mut rng_frame = ctx.seed.hasher(RngStep::Frame)
                .write(ctx.screen_pos)
                .write(ctx.layer)
                .write(at_index)
                .into_rng();
            spritesheet.random_frame(&mut rng_frame)
        }
    };
    let (screen_x, screen_y) = screen_index_to_pixels(at_index);
    let (offset_x, offset_y) = def.draw.offset;

    let final_x =
        (screen_x + 12) as i64
        + (offset_x + offset.0) as i64
        - (spritesheet.frame_width / 2) as i64;
    let final_y =
        (screen_y + 12) as i64
        + (offset_y + offset.1) as i64
        - (spritesheet.frame_height / 2) as i64;
    
    let flipped = if flip {
            let mut flipped = frame.to_image();
            imageops::flip_horizontal_in_place(&mut flipped);
            Some(flipped)
        }
        else {
            None
        };
    frame = match flipped.as_ref() {
        Some(flipped) => imageops::crop_imm(flipped, 0, 0, flipped.width(), flipped.height()),
        None => frame,
    };
    
    let alpha =
        if def.draw.trans_algo == TransAlgorithm::None {
            1.0
        }
        else {
            let mut rng_alpha = ctx.seed.hasher(RngStep::Alpha)
                .write(ctx.screen_pos)
                .write(ctx.layer)
                .write(at_index)
                .into_rng();
            let params = ctx.sync.trans_overrides.get(&id)
                .unwrap_or(&def.draw.trans);
            transparency::simulate(def.draw.trans_algo, &mut rng_alpha, params, ctx.opts.trans_frames)
        };
    
    blend_modes::overlay(&mut ctx.image, &*frame, final_x, final_y, def.draw.blend_mode, alpha);
}

fn draw_shift(ctx: &mut ScreenContext, curs: Cursor, vis_prop: &str, type_prop: &str) {
    let shift_visible = !ctx.ini_section
        .as_ref()
        .and_then(|section| section.get(vis_prop))
        .unwrap_or("True")
        .eq_ignore_ascii_case("False");

    if !shift_visible {
        return;
    }

    let shift_type = match ctx.ini_section
        .as_ref()
        .and_then(|section| section.get(type_prop))
        .unwrap_or("0")
    {
        "0" => ObjectVariant::Spot,
        "1" => ObjectVariant::Floor,
        "2" => ObjectVariant::Circle,
        "3" => ObjectVariant::Square,
        _ => ObjectVariant::Spot,
    };

    draw_object(ctx, curs.i, curs.proxy_id.into_variant(shift_type));
}

fn draw_with_glow(ctx: &mut ScreenContext, curs: Cursor) {
    draw_object(ctx, curs.i, curs.proxy_id.to_variant(ObjectVariant::Glow));
    draw_object(ctx, curs.i, curs.actual_id);
}

fn draw_elemental(ctx: &mut ScreenContext, curs: Cursor) {
    let mut rng = ctx.seed.hasher(RngStep::ElementalVariant)
        .write(ctx.screen_pos)
        .write(ctx.layer)
        .write(curs.i)
        .into_rng();
    let variant = [ObjectVariant::A, ObjectVariant::B, ObjectVariant::C, ObjectVariant::D]
        .choose(&mut rng)
        .unwrap();

    draw_object(ctx, curs.i, curs.proxy_id.into_variant(*variant));
}

fn draw_with_random_offset(ctx: &mut ScreenContext, curs: Cursor, range: RangeInclusive<i32>) {
    let mut rng = ctx.seed.hasher(RngStep::Offset)
        .write(ctx.screen_pos)
        .write(ctx.layer)
        .write(curs.i)
        .into_rng();
    let offset_x = rng.random_range(range.clone());
    let offset_y = rng.random_range(range);
    draw_object_with_offset(ctx, curs.i, curs.actual_id, (offset_x, offset_y));
}


fn apply_tint(ctx: &mut ScreenContext) {
    if ctx.opts.tint_strategy == TintStrategy::Ignore {
        return;
    }
    
    let Some(section) = ctx.ini_section.as_ref() else { return };
    
    let tint = section.get_i32_or("Tint", 0);
    if tint <= 0 {
        return;
    }
    
    let [r, g, b] = unpack_color(tint);
    let mut a: u8 = 255;
    let blend_mode = match section.get("TintInk")
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "add" => BlendMode::Add,
        "sub" => BlendMode::Sub,
        "and" => BlendMode::And,
        "or" => BlendMode::Or,
        "xor" => BlendMode::Xor,
        _ => {
            let tint_trans = section.get_i32_or("TintTrans", 46) % 128;
            a = (trans_to_alpha(tint_trans) * 255.0) as u8;
            BlendMode::Over
        }
    };
    let tint_final = image::Rgba([r, g, b, a]);
    
    for pixel in ctx.image.pixels_mut() {
        blend_modes::blend_pixels(pixel, tint_final, blend_mode);
    }
}
