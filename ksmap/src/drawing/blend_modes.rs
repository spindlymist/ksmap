use image::{GenericImage, GenericImageView, Rgb, Rgba};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum BlendMode {
    #[default]
    Over,
    Add,
    Sub,
    And,
    Or,
    Xor,
}

#[inline]
pub fn blend_pixels(bg: &mut Rgb<u8>, fg: Rgba<u8>, blend_mode: BlendMode) {
    match blend_mode {
        BlendMode::Over => blend_over(bg, fg),
        BlendMode::Add => blend_add(bg, fg),
        BlendMode::Sub => blend_sub(bg, fg),
        BlendMode::And => blend_and(bg, fg),
        BlendMode::Or => blend_or(bg, fg),
        BlendMode::Xor => blend_xor(bg, fg),
    }
}

/// Adapted from image crate
/// Source: https://github.com/image-rs/image/blob/ee6ecbf897ce0ad733849a0535f55d6fa6eb237c/src/color.rs
fn blend_over(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
    if fg[3] == 0 {
        return;
    }
    if fg[3] == 255 {
        bg[0] = fg[0];
        bg[1] = fg[1];
        bg[2] = fg[2];
        return;
    }

    // Convert to 0.0..=1.0
    let (bg_r, bg_g, bg_b) = (
        bg[0] as f32 / 255.0,
        bg[1] as f32 / 255.0,
        bg[2] as f32 / 255.0,
    );
    let (fg_r, fg_g, fg_b, fg_a) = (
        fg[0] as f32 / 255.0,
        fg[1] as f32 / 255.0,
        fg[2] as f32 / 255.0,
        fg[3] as f32 / 255.0,
    );

    // Premultiply channels by their alpha to simplify calculations
    let (fg_r_a, fg_g_a, fg_b_a) = (fg_r * fg_a, fg_g * fg_a, fg_b * fg_a);

    // Standard formula for src-over alpha compositing
    let (out_r, out_g, out_b) = (
        fg_r_a + bg_r * (1.0 - fg_a),
        fg_g_a + bg_g * (1.0 - fg_a),
        fg_b_a + bg_b * (1.0 - fg_a),
    );

    // Convert back to 0..=255
    bg[0] = (out_r * 255.0 + 0.5) as u8;
    bg[1] = (out_g * 255.0 + 0.5) as u8;
    bg[2] = (out_b * 255.0 + 0.5) as u8;
}

fn blend_add(bg: &mut Rgb<u8>, mut fg: Rgba<u8>) {
    fg[0] = bg[0].saturating_add(fg[0]);
    fg[1] = bg[1].saturating_add(fg[1]);
    fg[2] = bg[2].saturating_add(fg[2]);
    blend_over(bg, fg);
}

fn blend_sub(bg: &mut Rgb<u8>, mut fg: Rgba<u8>) {
    fg[0] = bg[0].saturating_sub(fg[0]);
    fg[1] = bg[1].saturating_sub(fg[1]);
    fg[2] = bg[2].saturating_sub(fg[2]);
    blend_over(bg, fg);
}

fn blend_and(bg: &mut Rgb<u8>, mut fg: Rgba<u8>) {
    fg[0] &= bg[0];
    fg[1] &= bg[1];
    fg[2] &= bg[2];
    blend_over(bg, fg);
}

fn blend_or(bg: &mut Rgb<u8>, mut fg: Rgba<u8>) {
    fg[0] |= bg[0];
    fg[1] |= bg[1];
    fg[2] |= bg[2];
    blend_over(bg, fg);
}

fn blend_xor(bg: &mut Rgb<u8>, mut fg: Rgba<u8>) {
    fg[0] ^= bg[0];
    fg[1] ^= bg[1];
    fg[2] ^= bg[2];
    blend_over(bg, fg);
}

/// Adapted from image crate
/// Source: https://github.com/image-rs/image/blob/285496d4fab063645dc4ffafd7ccfa3e06c35052/src/imageops/mod.rs#L219
pub fn overlay<I, J>(bottom: &mut I, top: &J, x: i64, y: i64)
where
    I: GenericImage<Pixel = Rgb<u8>>,
    J: GenericImageView<Pixel = Rgba<u8>>,
{
    let OverlayBounds {
        origin_bot_x,
        origin_bot_y,
        origin_top_x,
        origin_top_y,
        x_range,
        y_range,
    } = overlay_bounds_ext(bottom.dimensions(), top.dimensions(), x, y);
    for y in 0..y_range {
        for x in 0..x_range {
            let mut pixel_bot = bottom.get_pixel(origin_bot_x + x, origin_bot_y + y);
            let pixel_top = top.get_pixel(origin_top_x + x, origin_top_y + y);
            blend_over(&mut pixel_bot, pixel_top);
            bottom.put_pixel(origin_bot_x + x, origin_bot_y + y, pixel_bot);
        }
    }
}

/// Adapted from image crate
/// Source: https://github.com/image-rs/image/blob/285496d4fab063645dc4ffafd7ccfa3e06c35052/src/imageops/mod.rs#L219
pub fn overlay_ex<I, J>(bottom: &mut I, top: &J, x: i64, y: i64, blend_mode: BlendMode, alpha: u8)
where
    I: GenericImage<Pixel = Rgb<u8>>,
    J: GenericImageView<Pixel = Rgba<u8>>,
{
    let alpha_norm = alpha as f32 / 255.0;
    let OverlayBounds {
        origin_bot_x,
        origin_bot_y,
        origin_top_x,
        origin_top_y,
        x_range,
        y_range,
    } = overlay_bounds_ext(bottom.dimensions(), top.dimensions(), x, y);
    for y in 0..y_range {
        for x in 0..x_range {
            let mut pixel_bot = bottom.get_pixel(origin_bot_x + x, origin_bot_y + y);
            let mut pixel_top = top.get_pixel(origin_top_x + x, origin_top_y + y);
            pixel_top.0[3] = (pixel_top.0[3] as f32 * alpha_norm + 0.5) as u8;
            blend_pixels(&mut pixel_bot, pixel_top, blend_mode);
            bottom.put_pixel(origin_bot_x + x, origin_bot_y + y, pixel_bot);
        }
    }
}

#[derive(Default)]
struct OverlayBounds {
    origin_bot_x: u32,
    origin_bot_y: u32,
    origin_top_x: u32,
    origin_top_y: u32,
    x_range: u32,
    y_range: u32,   
}

/// Adapted from image crate
/// Source: https://github.com/image-rs/image/blob/285496d4fab063645dc4ffafd7ccfa3e06c35052/src/imageops/mod.rs#L170
fn overlay_bounds_ext(
    (bottom_width, bottom_height): (u32, u32),
    (top_width, top_height): (u32, u32),
    x: i64,
    y: i64,
) -> OverlayBounds {
    // Return a predictable value if the two images don't overlap at all.
    if x > i64::from(bottom_width)
        || y > i64::from(bottom_height)
        || x.saturating_add(i64::from(top_width)) <= 0
        || y.saturating_add(i64::from(top_height)) <= 0
    {
        return OverlayBounds::default();
    }

    // Find the maximum x and y coordinates in terms of the bottom image.
    let max_x = x.saturating_add(i64::from(top_width));
    let max_y = y.saturating_add(i64::from(top_height));

    // Clip the origin and maximum coordinates to the bounds of the bottom image.
    // Casting to a u32 is safe because both 0 and `bottom_{width,height}` fit
    // into 32-bits.
    let max_inbounds_x = max_x.clamp(0, i64::from(bottom_width)) as u32;
    let max_inbounds_y = max_y.clamp(0, i64::from(bottom_height)) as u32;
    let origin_bottom_x = x.clamp(0, i64::from(bottom_width)) as u32;
    let origin_bottom_y = y.clamp(0, i64::from(bottom_height)) as u32;

    // The range is the difference between the maximum inbounds coordinates and
    // the clipped origin. Unchecked subtraction is safe here because both are
    // always positive and `max_inbounds_{x,y}` >= `origin_{x,y}` due to
    // `top_{width,height}` being >= 0.
    let x_range = max_inbounds_x - origin_bottom_x;
    let y_range = max_inbounds_y - origin_bottom_y;

    // If x (or y) is negative, then the origin of the top image is shifted by -x (or -y).
    let origin_top_x = x.saturating_mul(-1).clamp(0, i64::from(top_width)) as u32;
    let origin_top_y = y.saturating_mul(-1).clamp(0, i64::from(top_height)) as u32;

    OverlayBounds {
        origin_bot_x: origin_bottom_x,
        origin_bot_y: origin_bottom_y,
        origin_top_x,
        origin_top_y,
        x_range,
        y_range,
    }
}
