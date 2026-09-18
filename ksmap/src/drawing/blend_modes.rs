use image::{GenericImage, GenericImageView, Pixel, Rgb, Rgba};
use serde::Deserialize;

use crate::drawing::pixel::KsmapPixel;

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

pub trait BlendWithRgba8: Pixel<Subpixel = u8> {
    fn blend_over(&mut self, fg: Rgba<u8>);
    fn blend_add(&mut self, fg: Rgba<u8>);
    fn blend_sub(&mut self, fg: Rgba<u8>);
    fn blend_and(&mut self, fg: Rgba<u8>);
    fn blend_or(&mut self, fg: Rgba<u8>);
    fn blend_xor(&mut self, fg: Rgba<u8>);
}

#[inline]
pub fn blend_pixels<P: KsmapPixel>(bg: &mut P, fg: Rgba<u8>, blend_mode: BlendMode) {
    match blend_mode {
        BlendMode::Over => bg.blend_over(fg),
        BlendMode::Add => bg.blend_add(fg),
        BlendMode::Sub => bg.blend_sub(fg),
        BlendMode::And => bg.blend_and(fg),
        BlendMode::Or => bg.blend_or(fg),
        BlendMode::Xor => bg.blend_xor(fg),
    }
}

impl BlendWithRgba8 for Rgb<u8> {
    /// Adapted from image crate
    /// Source: https://github.com/image-rs/image/blob/ee6ecbf897ce0ad733849a0535f55d6fa6eb237c/src/color.rs
    #[inline]
    fn blend_over(&mut self, fg: Rgba<u8>) {
        // http://stackoverflow.com/questions/7438263/alpha-compositing-algorithm-blend-modes#answer-11163848

        if fg.0[3] == 0 {
            return;
        }
        if fg.0[3] == 255 {
            self.0[0] = fg.0[0];
            self.0[1] = fg.0[1];
            self.0[2] = fg.0[2];
            return;
        }

        // First, as we don't know what type our pixel is, we have to convert to floats between 0.0 and 1.0
        const MAX_T: f32 = 255.0;
        let (bg_r, bg_g, bg_b, bg_a) = (self.0[0], self.0[1], self.0[2], 255);
        let (fg_r, fg_g, fg_b, fg_a) = (fg.0[0], fg.0[1], fg.0[2], fg.0[3]);
        let (bg_r, bg_g, bg_b, bg_a) = (
            bg_r as f32 / MAX_T,
            bg_g as f32 / MAX_T,
            bg_b as f32 / MAX_T,
            bg_a as f32 / MAX_T,
        );
        let (fg_r, fg_g, fg_b, fg_a) = (
            fg_r as f32 / MAX_T,
            fg_g as f32 / MAX_T,
            fg_b as f32 / MAX_T,
            fg_a as f32 / MAX_T,
        );

        // Work out what the final alpha level will be
        let alpha_final = bg_a + fg_a - bg_a * fg_a;
        if alpha_final == 0.0 {
            return;
        }

        // We premultiply our channels by their alpha, as this makes it easier to calculate
        let (bg_r_a, bg_g_a, bg_b_a) = (bg_r * bg_a, bg_g * bg_a, bg_b * bg_a);
        let (fg_r_a, fg_g_a, fg_b_a) = (fg_r * fg_a, fg_g * fg_a, fg_b * fg_a);

        // Standard formula for src-over alpha compositing
        let (out_r_a, out_g_a, out_b_a) = (
            fg_r_a + bg_r_a * (1.0 - fg_a),
            fg_g_a + bg_g_a * (1.0 - fg_a),
            fg_b_a + bg_b_a * (1.0 - fg_a),
        );

        // Unmultiply the channels by our resultant alpha channel
        let (out_r, out_g, out_b) = (
            out_r_a / alpha_final,
            out_g_a / alpha_final,
            out_b_a / alpha_final,
        );

        // Cast back to our initial type on return
        self.0[0] = (MAX_T * out_r) as u8;
        self.0[1] = (MAX_T * out_g) as u8;
        self.0[2] = (MAX_T * out_b) as u8;
    }
    
    #[inline]
    fn blend_add(&mut self, mut fg: Rgba<u8>) {
        premul_alpha(&mut fg);
        let Rgba([fg_r, fg_g, fg_b, _]) = fg;
        let Rgb([bg_r, bg_g, bg_b]) = self;
        *bg_r = bg_r.saturating_add(fg_r);
        *bg_g = bg_g.saturating_add(fg_g);
        *bg_b = bg_b.saturating_add(fg_b);
    }

    #[inline]
    fn blend_sub(&mut self, mut fg: Rgba<u8>) {
        premul_alpha(&mut fg);
        let Rgba([fg_r, fg_g, fg_b, _]) = fg;
        let Rgb([bg_r, bg_g, bg_b]) = self;
        *bg_r = bg_r.saturating_sub(fg_r);
        *bg_g = bg_g.saturating_sub(fg_g);
        *bg_b = bg_b.saturating_sub(fg_b);
    }

    #[inline]
    fn blend_and(&mut self, mut fg: Rgba<u8>) {
        premul_alpha(&mut fg);
        let Rgba([fg_r, fg_g, fg_b, _]) = fg;
        let Rgb([bg_r, bg_g, bg_b]) = self;
        *bg_r &= fg_r;
        *bg_g &= fg_g;
        *bg_b &= fg_b;
    }

    #[inline]
    fn blend_or(&mut self, mut fg: Rgba<u8>) {
        premul_alpha(&mut fg);
        let Rgba([fg_r, fg_g, fg_b, _]) = fg;
        let Rgb([bg_r, bg_g, bg_b]) = self;
        *bg_r |= fg_r;
        *bg_g |= fg_g;
        *bg_b |= fg_b;
    }

    #[inline]
    fn blend_xor(&mut self, mut fg: Rgba<u8>) {
        premul_alpha(&mut fg);
        let Rgba([fg_r, fg_g, fg_b, _]) = fg;
        let Rgb([bg_r, bg_g, bg_b]) = self;
        *bg_r ^= fg_r;
        *bg_g ^= fg_g;
        *bg_b ^= fg_b;
    }
}

impl BlendWithRgba8 for Rgba<u8> {
    /// Adapted from image crate
    /// Source: https://github.com/image-rs/image/blob/ee6ecbf897ce0ad733849a0535f55d6fa6eb237c/src/color.rs
    #[inline]
    fn blend_over(&mut self, fg: Rgba<u8>) {
        // http://stackoverflow.com/questions/7438263/alpha-compositing-algorithm-blend-modes#answer-11163848

        if fg.0[3] == 0 {
            return;
        }
        if fg.0[3] == 255 {
            *self = fg;
            return;
        }

        // First, as we don't know what type our pixel is, we have to convert to floats between 0.0 and 1.0
        const MAX_T: f32 = 255.0;
        let (bg_r, bg_g, bg_b, bg_a) = (self.0[0], self.0[1], self.0[2], self.0[3]);
        let (fg_r, fg_g, fg_b, fg_a) = (fg.0[0], fg.0[1], fg.0[2], fg.0[3]);
        let (bg_r, bg_g, bg_b, bg_a) = (
            bg_r as f32 / MAX_T,
            bg_g as f32 / MAX_T,
            bg_b as f32 / MAX_T,
            bg_a as f32 / MAX_T,
        );
        let (fg_r, fg_g, fg_b, fg_a) = (
            fg_r as f32 / MAX_T,
            fg_g as f32 / MAX_T,
            fg_b as f32 / MAX_T,
            fg_a as f32 / MAX_T,
        );

        // Work out what the final alpha level will be
        let alpha_final = bg_a + fg_a - bg_a * fg_a;
        if alpha_final == 0.0 {
            return;
        }

        // We premultiply our channels by their alpha, as this makes it easier to calculate
        let (bg_r_a, bg_g_a, bg_b_a) = (bg_r * bg_a, bg_g * bg_a, bg_b * bg_a);
        let (fg_r_a, fg_g_a, fg_b_a) = (fg_r * fg_a, fg_g * fg_a, fg_b * fg_a);

        // Standard formula for src-over alpha compositing
        let (out_r_a, out_g_a, out_b_a) = (
            fg_r_a + bg_r_a * (1.0 - fg_a),
            fg_g_a + bg_g_a * (1.0 - fg_a),
            fg_b_a + bg_b_a * (1.0 - fg_a),
        );

        // Unmultiply the channels by our resultant alpha channel
        let (out_r, out_g, out_b) = (
            out_r_a / alpha_final,
            out_g_a / alpha_final,
            out_b_a / alpha_final,
        );

        // Cast back to our initial type on return
        self.0[0] = (MAX_T * out_r) as u8;
        self.0[1] = (MAX_T * out_g) as u8;
        self.0[2] = (MAX_T * out_b) as u8;
        self.0[3] = (MAX_T * alpha_final + 0.5) as u8;
    }
    
    #[inline]
    fn blend_add(&mut self, mut fg: Rgba<u8>) {
        premul_alpha(&mut fg);
        let Rgba([fg_r, fg_g, fg_b, _]) = fg;
        let Rgba([bg_r, bg_g, bg_b, _]) = self;
        *bg_r = bg_r.saturating_add(fg_r);
        *bg_g = bg_g.saturating_add(fg_g);
        *bg_b = bg_b.saturating_add(fg_b);
    }

    #[inline]
    fn blend_sub(&mut self, mut fg: Rgba<u8>) {
        premul_alpha(&mut fg);
        let Rgba([fg_r, fg_g, fg_b, _]) = fg;
        let Rgba([bg_r, bg_g, bg_b, _]) = self;
        *bg_r = bg_r.saturating_sub(fg_r);
        *bg_g = bg_g.saturating_sub(fg_g);
        *bg_b = bg_b.saturating_sub(fg_b);
    }

    #[inline]
    fn blend_and(&mut self, mut fg: Rgba<u8>) {
        premul_alpha(&mut fg);
        let Rgba([fg_r, fg_g, fg_b, _]) = fg;
        let Rgba([bg_r, bg_g, bg_b, _]) = self;
        *bg_r &= fg_r;
        *bg_g &= fg_g;
        *bg_b &= fg_b;
    }

    #[inline]
    fn blend_or(&mut self, mut fg: Rgba<u8>) {
        premul_alpha(&mut fg);
        let Rgba([fg_r, fg_g, fg_b, _]) = fg;
        let Rgba([bg_r, bg_g, bg_b, _]) = self;
        *bg_r |= fg_r;
        *bg_g |= fg_g;
        *bg_b |= fg_b;
    }

    #[inline]
    fn blend_xor(&mut self, mut fg: Rgba<u8>) {
        premul_alpha(&mut fg);
        let Rgba([fg_r, fg_g, fg_b, _]) = fg;
        let Rgba([bg_r, bg_g, bg_b, _]) = self;
        *bg_r ^= fg_r;
        *bg_g ^= fg_g;
        *bg_b ^= fg_b;
    }
}

#[inline]
fn premul_alpha(pixel: &mut Rgba<u8>) {
    let Rgba([r, g, b, a]) = pixel;
    let alpha_norm = *a as f32 / 255.0;
    *r = (*r as f32 * alpha_norm) as u8;
    *g = (*g as f32 * alpha_norm) as u8;
    *b = (*b as f32 * alpha_norm) as u8;
}

/// Adapted from image crate
/// Source: https://github.com/image-rs/image/blob/285496d4fab063645dc4ffafd7ccfa3e06c35052/src/imageops/mod.rs#L219
pub fn overlay<P, I, J>(bottom: &mut I, top: &J, x: i64, y: i64)
where
    P: KsmapPixel,
    I: GenericImage<Pixel = P>,
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
            pixel_bot.blend_over(pixel_top);
            bottom.put_pixel(origin_bot_x + x, origin_bot_y + y, pixel_bot);
        }
    }
}

/// Adapted from image crate
/// Source: https://github.com/image-rs/image/blob/285496d4fab063645dc4ffafd7ccfa3e06c35052/src/imageops/mod.rs#L219
pub fn overlay_ex<P, I, J>(bottom: &mut I, top: &J, x: i64, y: i64, blend_mode: BlendMode, alpha: f32)
where
    P: KsmapPixel,
    I: GenericImage<Pixel = P>,
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
