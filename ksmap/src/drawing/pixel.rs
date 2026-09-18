use image::{ImageBuffer, Rgb, Rgba};

use super::blend_modes::BlendWithRgba8;

pub type OutputImage<P> = ImageBuffer<P, Vec<u8>>;

pub trait KsmapImage {
    fn into_bytes(self) -> Vec<u8>;
}

impl KsmapImage for OutputImage<Rgb<u8>> {
    fn into_bytes(self) -> Vec<u8> {
        self.into_raw()
    }
}

impl KsmapImage for OutputImage<Rgba<u8>> {
    fn into_bytes(self) -> Vec<u8> {
        self.into_raw()
    }
}

pub trait KsmapPixel: BlendWithRgba8 {
    fn zero() -> Self::Subpixel { 0 }
    fn white() -> Self;
    fn mtpng_color_type() -> mtpng::ColorType;
    fn image_color_type() -> image::ExtendedColorType;
    fn bytes_per_pixel() -> u8;
    fn bit_depth() -> u8 { 8 }
}

impl KsmapPixel for Rgb<u8> {
    fn zero() -> Self::Subpixel {
        0
    }
    
    fn white() -> Self {
        Self([255, 255, 255])
    }
    
    fn mtpng_color_type() -> mtpng::ColorType {
        mtpng::ColorType::Truecolor
    }
    
    fn image_color_type() -> image::ExtendedColorType {
        image::ExtendedColorType::Rgb8
    }
    
    fn bytes_per_pixel() -> u8 {
        3
    }
}

impl KsmapPixel for Rgba<u8> {
    fn zero() -> Self::Subpixel {
        0
    }
    
    fn white() -> Self {
        Self([255, 255, 255, 255])
    }
    
    fn mtpng_color_type() -> mtpng::ColorType {
        mtpng::ColorType::TruecolorAlpha
    }
    
    fn image_color_type() -> image::ExtendedColorType {
        image::ExtendedColorType::Rgba8
    }
    
    fn bytes_per_pixel() -> u8 {
        4
    }
}
