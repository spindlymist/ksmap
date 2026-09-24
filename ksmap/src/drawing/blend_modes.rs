use image::{GenericImage, GenericImageView, Pixel, Rgb, Rgba};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendAlgorithm {
    Quality,
    Accurate,
}

pub trait Blend {
    type CustomObjectAlgorithm: Blend;
    fn blend_over(bg: &mut Rgb<u8>, fg: Rgba<u8>);
    fn blend_over_with_opacity(bg: &mut Rgb<u8>, fg: Rgba<u8>, opacity: u8);
    fn blend_add(bg: &mut Rgb<u8>, fg: Rgba<u8>);
    fn blend_sub(bg: &mut Rgb<u8>, fg: Rgba<u8>);
    fn blend_and(bg: &mut Rgb<u8>, fg: Rgba<u8>);
    fn blend_or(bg: &mut Rgb<u8>, fg: Rgba<u8>);
    fn blend_xor(bg: &mut Rgb<u8>, fg: Rgba<u8>);
}

pub struct BlendAlgorithmQuality;
pub struct BlendAlgorithmAccurate;
pub struct BlendAlgorithmAccurateCustomObj;

impl Blend for BlendAlgorithmQuality {
    type CustomObjectAlgorithm = Self;

    /// Adapted from image crate
    /// Source: https://github.com/image-rs/image/blob/ee6ecbf897ce0ad733849a0535f55d6fa6eb237c/src/color.rs
    #[inline(always)]
    fn blend_over(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        match fg[3] {
            0 => { return }
            255 => { *bg = fg.to_rgb(); return }
            _ => {}
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

    #[inline(always)]
    fn blend_over_with_opacity(bg: &mut Rgb<u8>, mut fg: Rgba<u8>, opacity: u8) {
        let opacity_norm = opacity as f32 / 255.0;
        fg[3] = (fg[3] as f32 * opacity_norm + 0.5) as u8;
        Self::blend_over(bg, fg);
    }

    #[inline(always)]
    fn blend_add(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_add::<Self>(bg, fg);
    }

    #[inline(always)]
    fn blend_sub(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_sub::<Self>(bg, fg);
    }

    #[inline(always)]
    fn blend_and(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_and::<Self>(bg, fg);
    }

    #[inline(always)]
    fn blend_or(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_or::<Self>(bg, fg);
    }

    #[inline(always)]
    fn blend_xor(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_xor::<Self>(bg, fg);
    }
}

impl Blend for BlendAlgorithmAccurate {
    type CustomObjectAlgorithm = BlendAlgorithmAccurateCustomObj;

    #[inline(always)]
    fn blend_over(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        match fg[3] {
            0 => { return }
            255 => { *bg = fg.to_rgb(); return }
            _ => {}
        }

        let (bg_r, bg_g, bg_b) = (
            bg[0] as u16,
            bg[1] as u16,
            bg[2] as u16,
        );
        let (fg_r, fg_g, fg_b, fg_a) = (
            fg[0] as u16,
            fg[1] as u16,
            fg[2] as u16,
            fg[3] as u16,
        );
        let (out_r, out_g, out_b) = (
            ((fg_a * fg_r) + (256 - fg_a) * bg_r) / 256,
            ((fg_a * fg_g) + (256 - fg_a) * bg_g) / 256,
            ((fg_a * fg_b) + (256 - fg_a) * bg_b) / 256,
        );

        bg.0 = [out_r as u8, out_g as u8, out_b as u8];
    }

    #[inline(always)]
    fn blend_over_with_opacity(bg: &mut Rgb<u8>, fg: Rgba<u8>, opacity: u8) {
        let mut temp = bg.clone();
        Self::blend_over(&mut temp, [fg[0], fg[1], fg[2], opacity].into());
        Self::blend_over(bg, [temp[0], temp[1], temp[2], fg[3]].into());
    }

    #[inline(always)]
    fn blend_add(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_add::<Self>(bg, fg);
    }

    #[inline(always)]
    fn blend_sub(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_sub::<Self>(bg, fg);
    }

    #[inline(always)]
    fn blend_and(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_and::<Self>(bg, fg);
    }

    #[inline(always)]
    fn blend_or(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_or::<Self>(bg, fg);
    }

    #[inline(always)]
    fn blend_xor(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_xor::<Self>(bg, fg);
    }
}

impl Blend for BlendAlgorithmAccurateCustomObj {
    type CustomObjectAlgorithm = Self;

    #[inline(always)]
    fn blend_over(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        match fg[3] {
            0 => { return }
            255 => { *bg = fg.to_rgb(); return }
            _ => {}
        }

        let (bg_r, bg_g, bg_b) = (
            bg[0] as u16,
            bg[1] as u16,
            bg[2] as u16,
        );
        let (fg_r, fg_g, fg_b, fg_a) = (
            fg[0] as u16,
            fg[1] as u16,
            fg[2] as u16,
            (fg[3] / 2) as u16,
        );
        let (out_r, out_g, out_b) = (
            ((fg_a * fg_r) + (128 - fg_a) * bg_r) / 128,
            ((fg_a * fg_g) + (128 - fg_a) * bg_g) / 128,
            ((fg_a * fg_b) + (128 - fg_a) * bg_b) / 128,
        );

        bg.0 = [out_r as u8, out_g as u8, out_b as u8];
    }

    #[inline(always)]
    fn blend_over_with_opacity(bg: &mut Rgb<u8>, fg: Rgba<u8>, opacity: u8) {
        if opacity == 255 {
            Self::blend_over(bg, fg);
        }
        else {
            // Unused
            let mut temp = bg.clone();
            Self::blend_over(&mut temp, [fg[0], fg[1], fg[2], opacity].into());
            BlendAlgorithmAccurate::blend_over(bg, [temp[0], temp[1], temp[2], fg[3]].into());
        }
    }

    /// Unused
    #[inline(always)]
    fn blend_add(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_add::<BlendAlgorithmAccurate>(bg, fg);
    }

    /// Unused
    #[inline(always)]
    fn blend_sub(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_sub::<BlendAlgorithmAccurate>(bg, fg);
    }

    /// Unused
    #[inline(always)]
    fn blend_and(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_and::<BlendAlgorithmAccurate>(bg, fg);
    }

    /// Unused
    #[inline(always)]
    fn blend_or(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_or::<BlendAlgorithmAccurate>(bg, fg);
    }

    /// Unused
    #[inline(always)]
    fn blend_xor(bg: &mut Rgb<u8>, fg: Rgba<u8>) {
        blend_xor::<BlendAlgorithmAccurate>(bg, fg);
    }
}

#[inline(always)]
pub fn blend_pixels<B: Blend>(bg: &mut Rgb<u8>, fg: Rgba<u8>, blend_mode: BlendMode, opacity: u8) {
    match blend_mode {
        BlendMode::Over => B::blend_over_with_opacity(bg, fg, opacity),
        BlendMode::Add => B::blend_add(bg, fg),
        BlendMode::Sub => B::blend_sub(bg, fg),
        BlendMode::And => B::blend_and(bg, fg),
        BlendMode::Or => B::blend_or(bg, fg),
        BlendMode::Xor => B::blend_xor(bg, fg),
    }
}

#[inline(always)]
fn blend_add<B: Blend>(bg: &mut Rgb<u8>, mut fg: Rgba<u8>) {
    fg[0] = bg[0].saturating_add(fg[0]);
    fg[1] = bg[1].saturating_add(fg[1]);
    fg[2] = bg[2].saturating_add(fg[2]);
    B::blend_over(bg, fg);
}

#[inline(always)]
fn blend_sub<B: Blend>(bg: &mut Rgb<u8>, mut fg: Rgba<u8>) {
    fg[0] = bg[0].saturating_sub(fg[0]);
    fg[1] = bg[1].saturating_sub(fg[1]);
    fg[2] = bg[2].saturating_sub(fg[2]);
    B::blend_over(bg, fg);
}

#[inline(always)]
fn blend_and<B: Blend>(bg: &mut Rgb<u8>, mut fg: Rgba<u8>) {
    fg[0] &= bg[0];
    fg[1] &= bg[1];
    fg[2] &= bg[2];
    B::blend_over(bg, fg);
}

#[inline(always)]
fn blend_or<B: Blend>(bg: &mut Rgb<u8>, mut fg: Rgba<u8>) {
    fg[0] |= bg[0];
    fg[1] |= bg[1];
    fg[2] |= bg[2];
    B::blend_over(bg, fg);
}

#[inline(always)]
fn blend_xor<B: Blend>(bg: &mut Rgb<u8>, mut fg: Rgba<u8>) {
    fg[0] ^= bg[0];
    fg[1] ^= bg[1];
    fg[2] ^= bg[2];
    B::blend_over(bg, fg);
}

/// Adapted from image crate
/// Source: https://github.com/image-rs/image/blob/285496d4fab063645dc4ffafd7ccfa3e06c35052/src/imageops/mod.rs#L219
pub fn overlay<B, I, J>(bottom: &mut I, top: &J, x: i64, y: i64)
where
    B: Blend,
    I: GenericImage<Pixel = Rgb<u8>>,
    J: GenericImageView<Pixel = Rgba<u8>>
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
            B::blend_over(&mut pixel_bot, pixel_top);
            bottom.put_pixel(origin_bot_x + x, origin_bot_y + y, pixel_bot);
        }
    }
}

/// Adapted from image crate
/// Source: https://github.com/image-rs/image/blob/285496d4fab063645dc4ffafd7ccfa3e06c35052/src/imageops/mod.rs#L219
pub fn overlay_ex<B, I, J>(bottom: &mut I, top: &J, x: i64, y: i64, blend_mode: BlendMode, opacity: u8)
where
    B: Blend,
    I: GenericImage<Pixel = Rgb<u8>>,
    J: GenericImageView<Pixel = Rgba<u8>>
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
            blend_pixels::<B>(&mut pixel_bot, pixel_top, blend_mode, opacity);
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
