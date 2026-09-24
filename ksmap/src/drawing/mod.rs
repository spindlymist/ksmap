use std::{fs, marker::PhantomData, ops::RangeInclusive, path::Path};

use anyhow::{anyhow, Result};
use image::{GenericImage, ImageEncoder, RgbImage, RgbaImage, codecs::png::PngEncoder, imageops};
use rand::prelude::*;
use libks::{ScreenCoord, map_bin::{LayerData, ScreenData, Tile}};
use libks_ini::edit::{Ini, LogicalSection};

use crate::{
    definitions::{AnimSync, Flip, ObjectDef, ObjectDefs, ObjectKind, TransAlgorithm, Visibility},
    graphics::{Gradient, Graphics, spritesheet::Spritesheet},
    id::{ObjectId, ObjectVariant},
    ini_util::{LogicalSectionExt, unpack_color},
    partition::{Bounds, Partition},
    screen_map::ScreenMap,
    seed::{MapSeed, RngStep},
    synchronization::{ScreenSync, WorldSync}
};

mod transparency;
pub use transparency::{trans_to_alpha, alpha_to_trans};

mod blend_modes;
use blend_modes::Blend;
pub use blend_modes::{
    BlendAlgorithm,
    BlendMode,
};

mod pixel;
use pixel::{OutputImage, KsmapImage, KsmapPixel};

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
    pub gfx: &'a Graphics,
    pub defs: &'a ObjectDefs,
    pub ini: &'a Ini,
    pub world_sync: &'a WorldSync,
    pub options: DrawOptions,
}

#[repr(C)]
struct ScreenContext<'a, B: Blend> {
    sync: ScreenSync,
    image: RgbImage,
    ini_section: Option<LogicalSection<'a>>,
    seed: MapSeed,
    tileset_a: Option<&'a RgbaImage>,
    tileset_b: Option<&'a RgbaImage>,
    gradient: Option<&'a Gradient>,
    gfx: &'a Graphics,
    defs: &'a ObjectDefs,
    opts: DrawOptions,
    screen_pos: ScreenCoord,
    layer: u8,
    blend_algorithm: PhantomData<B>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawOptions {
    pub blend_algorithm: BlendAlgorithm,
    pub show_invisible: bool,
    pub show_proximity: bool,
    /// Overrides the maximum transparency for objects that have random opacity to ensure they are visible.
    /// 0 is fully opaque and 128 is fully transparent.
    pub trans_max_override: u8,
    /// The number of object instances (per screen) required to ignore the transparency override.
    pub trans_max_threshold: u32,
    /// The number of frames to simulate for transparency.
    pub trans_frames: u32,
    pub tint_strategy: TintStrategy,
    pub ignore_laser_phase: bool,
}

impl Default for DrawOptions {
    fn default() -> Self {
        Self {
            blend_algorithm: BlendAlgorithm::Quality,
            show_invisible: false,
            show_proximity: false,
            trans_max_override: 122,
            trans_max_threshold: 5,
            trans_frames: 150,
            tint_strategy: TintStrategy::Ignore,
            ignore_laser_phase: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[inline(always)]
pub fn draw_partition(ctx: DrawContext, partition: &Partition, background: Option<[u8; 4]>) -> Result<RgbaImage> {
    let background = background.unwrap_or([0, 0, 0, 0]);
    draw_partition_generic(ctx, partition, background.into())
}

#[inline(always)]
pub fn draw_partition_rgb(ctx: DrawContext, partition: &Partition, background: Option<[u8; 3]>) -> Result<RgbImage> {
    let background = background.unwrap_or([0, 0, 0]);
    draw_partition_generic(ctx, partition, background.into())
}

fn draw_partition_generic<P>(
    ctx: DrawContext,
    partition: &Partition,
    background: P
) -> Result<OutputImage<P>>
where
    P: KsmapPixel,
    OutputImage<P>: KsmapImage
{
    let bounds = partition.bounds();
    let mut canvas: OutputImage<P> = make_canvas(&bounds, background)?;
    for pos in partition {
        let Some(index_screen) = ctx.screens.index_of(pos) else { continue };
        let screen = &ctx.screens[index_screen];
        let screen_image = draw_screen(ctx.seed, screen, index_screen, ctx.gfx, ctx.defs, ctx.ini, ctx.options, ctx.world_sync);
        let screen_image = OutputImage::<P>::from_rgb(screen_image);
        let canvas_x: u32 = ((screen.position.0 as i64 - bounds.x.start) * 600).try_into().unwrap();
        let canvas_y: u32 = ((screen.position.1 as i64 - bounds.y.start) * 240).try_into().unwrap();
        canvas.copy_from(&screen_image, canvas_x, canvas_y)?;
    }
    Ok(canvas)
}

fn make_canvas<P: KsmapPixel>(bounds: &Bounds, background: P) -> Result<OutputImage<P>> {
    let (width, height) = bounds.size();

    let Ok(Some(width)) = u32::try_from(width)
        .map(|width| width.checked_mul(600))
        else {
            return Err(anyhow!("Partition is too large: {bounds}"));
        };

    let Ok(Some(height)) = u32::try_from(height)
        .map(|height| height.checked_mul(240))
        else {
            return Err(anyhow!("Partition is too large: {bounds}"));
        };

    let Some(n_bytes) = (width as usize).checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(P::bytes_per_pixel() as usize))
        else {
            return Err(anyhow!("Partition is too large: {bounds}"));
        };
    
    let mut buffer = Vec::<u8>::new();
    match buffer.try_reserve_exact(n_bytes) {
        Ok(_) => {
            unsafe { buffer.set_len(n_bytes); }
            let image = match background.to_repeated_byte() {
                Some(byte) => {
                    buffer.fill(byte);
                    OutputImage::<P>::from_vec(width, height, buffer)
                        .expect("Buffer should be the correct size.")
                }
                None => {
                    let mut image = OutputImage::<P>::from_vec(width, height, buffer)
                        .expect("Buffer should be the correct size.");
                    for pixel in image.pixels_mut() {
                        *pixel = background;
                    }
                    image
                }
            };
            Ok(image)
        }
        Err(_) => Err(anyhow!("Not enough memory available for partition: {bounds}"))
    }
}

pub fn export_canvas<P>(canvas: OutputImage<P>, path: &Path, compression_level: u8) -> Result<()>
where
    P: KsmapPixel,
    OutputImage<P>: KsmapImage
{
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;

    let encoder = PngEncoder::new_with_quality(
        file,
        image::codecs::png::CompressionType::Level(compression_level),
        Default::default(),
    );

    let width = canvas.width();
    let height = canvas.height();
    let buf = canvas.into_bytes();

    encoder.write_image(&buf, width, height, P::image_color_type())?;

    Ok(())
}

pub fn export_canvas_multithreaded<P>(canvas: OutputImage<P>, path: &Path, compression_level: u8) -> Result<()>
where
    P: KsmapPixel,
    OutputImage<P>: KsmapImage
{
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    let writer = std::io::BufWriter::new(file);
    
    let width = canvas.width();
    let height = canvas.height();
    let data = canvas.into_bytes();
    
    let mut header = mtpng::Header::new();
    header.set_size(width, height)?;
    header.set_color(P::mtpng_color_type(), P::bit_depth())?;
    
    let mut options = mtpng::encoder::Options::new();
    options.set_compression_level(compression_level.try_into().unwrap_or_default())?;

    let mut encoder = mtpng::encoder::Encoder::new(writer, &options);
    encoder.write_header(&header)?;
    encoder.write_image_rows(&data)?;
    encoder.finish()?;

    Ok(())
}

#[inline(always)]
pub fn draw_screen(
    seed: MapSeed,
    screen: &ScreenData,
    index_screen: usize,
    gfx: &Graphics,
    defs: &ObjectDefs,
    ini: &Ini,
    opts: DrawOptions,
    world_sync: &WorldSync,
) -> RgbImage {
    match opts.blend_algorithm {
        BlendAlgorithm::Quality => draw_screen_generic::<blend_modes::BlendAlgorithmQuality>(
            seed,
            screen,
            index_screen,
            gfx,
            defs,
            ini,
            opts,
            world_sync,
        ),
        BlendAlgorithm::Accurate => draw_screen_generic::<blend_modes::BlendAlgorithmAccurate>(
            seed,
            screen,
            index_screen,
            gfx,
            defs,
            ini,
            opts,
            world_sync,
        ),
    }
}

fn draw_screen_generic<B: Blend>(
    seed: MapSeed,
    screen: &ScreenData,
    index_screen: usize,
    gfx: &Graphics,
    defs: &ObjectDefs,
    ini: &Ini,
    opts: DrawOptions,
    world_sync: &WorldSync,
) -> RgbImage {
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
        image: RgbImage::from_pixel(600, 240, [255, 255, 255].into()),
        tileset_a: gfx.tileset(screen.assets.tileset_a),
        tileset_b: gfx.tileset(screen.assets.tileset_b),
        gradient: gfx.gradient(screen.assets.gradient),
        gfx,
        defs,
        ini_section,
        sync,
        opts,
        blend_algorithm: PhantomData::<B>
    };
    
    // Believe it or not, KS renders the gradient and tiles twice in alternating fashion. This appears to be a bug in
    // the MMF runtime when the display mode is set to Standard. Normally, it is unnoticeable because the gradient is
    // opaque, so the first two layers are fully obscured. However, when the gradient has transparent pixels, it
    // becomes significant.
    let render_reps = match ctx.gradient {
        Some(grad) if grad.has_transparency => 2,
        _ => 1,
    };
    for _ in 0..render_reps {
        // Draw gradient
        if let Some(gradient) = ctx.gradient {
            // The number of times the gradient repeats is rounded down. As a result, if 600 cannot be evenly divided by
            // the gradient's width, it will not cover the entire background. If the gradient's width is more than 600,
            // nothing will be drawn.
            let gradient_reps = 600 / gradient.image.width();
            for i in 0..gradient_reps {
                let x = i * gradient.image.width();
                blend_modes::overlay::<B, _, _>(&mut ctx.image, gradient.image.as_ref(), x as i64, 0);
            }
        }
        
        // Draw tile layers
        draw_tile_layer(&mut ctx, &screen.layers[0]);
        draw_tile_layer(&mut ctx, &screen.layers[1]);
        if !is_overlay {
            draw_tile_layer(&mut ctx, &screen.layers[2]);
        }
        draw_tile_layer(&mut ctx, &screen.layers[3]);
    }

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

    ctx.image
}

fn draw_tile_layer<B: Blend>(ctx: &mut ScreenContext<'_, B>, layer: &LayerData) {
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
        blend_modes::overlay::<B, _, _>(&mut ctx.image, &*tile_img, screen_x as i64, screen_y as i64);
    }
}

fn draw_object_layer<B: Blend>(ctx: &mut ScreenContext<'_, B>, layer: &LayerData) {
    for (i, tile) in layer.0.iter().enumerate() {
        if tile.1 == 0 { continue }

        let actual_id = ObjectId::from(tile);
        let Some(object_def) = ctx.defs.get(&actual_id) else { continue };
        let proxy_id = match object_def.kind {
            ObjectKind::OverrideObject(tile_original) => ObjectId::from(tile_original),
            _ => ObjectId::from(tile),
        };
        let mut curs = Cursor {
            i,
            actual_id,
            proxy_id,
        };
        
        let is_limited = ctx.sync.limiters.get_mut(&curs.proxy_id)
            .is_some_and(|limiter| !limiter.increment());
        if is_limited {
            continue;
        }
        
        let is_invisible = match object_def.draw.visibility {
            Visibility::Never => !ctx.opts.show_invisible,
            Visibility::Proximity => {
                // Hack for 19-46
                if ctx.opts.show_proximity {
                    false
                }
                else if let Some(placeholder) = object_def.draw.placeholder_variant {
                    curs.proxy_id = curs.proxy_id.into_variant(placeholder);
                    curs.actual_id = curs.proxy_id;
                    false
                }
                else {
                    true
                }
            }
            Visibility::Always => false,
        };
        if is_invisible {
            continue;
        }
        
        let is_out_of_phase =
            !ctx.opts.ignore_laser_phase
            && object_def.sync.laser_phase
            .as_ref()
            .is_some_and(|phase| {
                *phase != ctx.sync.group.laser_phase
            });
        if is_out_of_phase {
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
            Tile(254.., _) => {
                let ctx_temp = change_blend_algorithm::<_, B::CustomObjectAlgorithm>(ctx);
                draw_object(ctx_temp, curs.i, curs.actual_id);
            }
            _ => draw_object(ctx, curs.i, curs.actual_id),
        }
    }
}

/// Alters the phantom data of `ctx` to use a different blending algorithm.
fn change_blend_algorithm<'a, 'b, In, Out>(ctx: &'a mut ScreenContext<'b, In>) -> &'a mut ScreenContext<'b, Out>
where
    In: Blend,
    Out: Blend
{
    // SAFETY: The only thing changed here is the type of the PhantomData
    // PhantomData does not affect layout in repr(c)
    unsafe { &mut *(ctx as *mut ScreenContext<'b, In> as *mut ScreenContext<'b, Out>) }
}

fn draw_object<B: Blend>(
    ctx: &mut ScreenContext<'_, B>,
    at_index: usize,
    object: ObjectId,
) {
    draw_object_with_offset(ctx, at_index, object, (0, 0));
}

fn draw_object_with_offset<B: Blend>(
    ctx: &mut ScreenContext<'_, B>,
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

fn draw_spritesheet<B: Blend>(
    ctx: &mut ScreenContext<'_, B>,
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
    
    // A bit awkward. The flipped image must live till the end of the function because frame is only a reference.
    let flipped =
        if flip {
            let mut flipped = frame.to_image();
            imageops::flip_horizontal_in_place(&mut flipped);
            Some(flipped)
        }
        else {
            None
        };
    if let Some(flipped) = flipped.as_ref() {
        frame = imageops::crop_imm(flipped, 0, 0, flipped.width(), flipped.height());
    }
    
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
    
    let opacity =
        if def.draw.trans_algo == TransAlgorithm::None {
            255
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
    
    blend_modes::overlay_ex::<B, _, _>(&mut ctx.image, &*frame, final_x, final_y, def.draw.blend_mode, opacity);
}

fn draw_shift<B: Blend>(ctx: &mut ScreenContext<'_, B>, curs: Cursor, vis_prop: &str, type_prop: &str) {
    let is_invisible = ctx.ini_section
        .as_ref()
        .and_then(|section| section.get(vis_prop))
        .map(|value| value.eq_ignore_ascii_case("false"))
        .unwrap_or(false);

    if is_invisible {
        if ctx.opts.show_invisible {
            // Draw editor icon
            draw_object(ctx, curs.i, curs.proxy_id);
        }
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

fn draw_with_glow<B: Blend>(ctx: &mut ScreenContext<'_, B>, curs: Cursor) {
    draw_object(ctx, curs.i, curs.proxy_id.to_variant(ObjectVariant::Glow));
    draw_object(ctx, curs.i, curs.actual_id);
}

fn draw_elemental<B: Blend>(ctx: &mut ScreenContext<'_, B>, curs: Cursor) {
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

fn draw_with_random_offset<B: Blend>(ctx: &mut ScreenContext<'_, B>, curs: Cursor, range: RangeInclusive<i32>) {
    let mut rng = ctx.seed.hasher(RngStep::Offset)
        .write(ctx.screen_pos)
        .write(ctx.layer)
        .write(curs.i)
        .into_rng();
    let offset_x = rng.random_range(range.clone());
    let offset_y = rng.random_range(range);
    draw_object_with_offset(ctx, curs.i, curs.actual_id, (offset_x, offset_y));
}

fn apply_tint<B: Blend>(ctx: &mut ScreenContext<'_, B>){
    if ctx.opts.tint_strategy == TintStrategy::Ignore {
        return;
    }
    
    let Some(section) = ctx.ini_section.as_ref() else { return };
    
    let tint = section.get_i32_or("Tint", 0);
    if tint <= 0 {
        return;
    }
    
    let [r, g, b] = unpack_color(tint);
    let mut opacity = 255u8;
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
            opacity = trans_to_alpha(tint_trans as u8);
            BlendMode::Over
        }
    };
    
    for pixel in ctx.image.pixels_mut() {
        blend_modes::blend_pixels::<B>(pixel, [r, g, b, 255].into(), blend_mode, opacity);
    }
}
