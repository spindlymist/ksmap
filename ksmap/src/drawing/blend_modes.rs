use image::{GenericImage, GenericImageView, Rgba, Pixel};
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
pub fn blend_pixels(bg: &mut Rgba<u8>, fg: Rgba<u8>, blend_mode: BlendMode) {
    match blend_mode {
        BlendMode::Over => blend_over(bg, fg),
        BlendMode::Add => blend_add(bg, fg),
        BlendMode::Sub => blend_sub(bg, fg),
        BlendMode::And => blend_and(bg, fg),
        BlendMode::Or => blend_or(bg, fg),
        BlendMode::Xor => blend_xor(bg, fg),
    }
}

#[inline]
fn blend_over(bg: &mut Rgba<u8>, fg: Rgba<u8>) {
    bg.blend(&fg);
}

#[inline]
fn premul_alpha(pixel: &mut Rgba<u8>) {
    let Rgba([r, g, b, a]) = pixel;
    let alpha_norm = *a as f32 / 255.0;
    *r = (*r as f32 * alpha_norm) as u8;
    *g = (*g as f32 * alpha_norm) as u8;
    *b = (*b as f32 * alpha_norm) as u8;
}

#[inline]
fn blend_add(Rgba(bg): &mut Rgba<u8>, mut fg: Rgba<u8>) {
    premul_alpha(&mut fg);
    let Rgba(fg) = fg;
    bg[0] = bg[0].saturating_add(fg[0]);
    bg[1] = bg[1].saturating_add(fg[1]);
    bg[2] = bg[2].saturating_add(fg[2]);
}

#[inline]
fn blend_sub(Rgba(bg): &mut Rgba<u8>, mut fg: Rgba<u8>) {
    premul_alpha(&mut fg);
    let Rgba(fg) = fg;
    bg[0] = bg[0].saturating_sub(fg[0]);
    bg[1] = bg[1].saturating_sub(fg[1]);
    bg[2] = bg[2].saturating_sub(fg[2]);
}

#[inline]
fn blend_and(Rgba(bg): &mut Rgba<u8>, mut fg: Rgba<u8>) {
    premul_alpha(&mut fg);
    let Rgba(fg) = fg;
    bg[0] &= fg[0];
    bg[1] &= fg[1];
    bg[2] &= fg[2];
}

#[inline]
fn blend_or(Rgba(bg): &mut Rgba<u8>, mut fg: Rgba<u8>) {
    premul_alpha(&mut fg);
    let Rgba(fg) = fg;
    bg[0] |= fg[0];
    bg[1] |= fg[1];
    bg[2] |= fg[2];
}

#[inline]
fn blend_xor(Rgba(bg): &mut Rgba<u8>, mut fg: Rgba<u8>) {
    premul_alpha(&mut fg);
    let Rgba(fg) = fg;
    bg[0] ^= fg[0];
    bg[1] ^= fg[1];
    bg[2] ^= fg[2];
}

/// Adapted from image crate
/// Source: https://github.com/image-rs/image/blob/285496d4fab063645dc4ffafd7ccfa3e06c35052/src/imageops/mod.rs#L219
pub fn overlay<I, J>(bottom: &mut I, top: &J, x: i64, y: i64, blend_mode: BlendMode, alpha: f32)
where
    I: GenericImage<Pixel = Rgba<u8>>,
    J: GenericImageView<Pixel = I::Pixel>,
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
            let mut pixel_top = top.get_pixel(origin_top_x + x, origin_top_y + y);
            pixel_top.0[3] = (pixel_top.0[3] as f32 * alpha) as u8;
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

/// Private function from image crate
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
