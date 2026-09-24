use image::{DynamicImage, ImageBuffer, Pixel, Rgb, RgbImage, Rgba};

pub type OutputImage<P> = ImageBuffer<P, Vec<u8>>;

pub trait KsmapImage {
    fn into_bytes(self) -> Vec<u8>;
    fn from_rgb(image: RgbImage) -> Self;
}

impl KsmapImage for OutputImage<Rgb<u8>> {
    fn into_bytes(self) -> Vec<u8> {
        self.into_raw()
    }
    
    fn from_rgb(image: RgbImage) -> Self {
        image
    }
}

impl KsmapImage for OutputImage<Rgba<u8>> {
    fn into_bytes(self) -> Vec<u8> {
        self.into_raw()
    }
    
    fn from_rgb(image: RgbImage) -> Self {
        DynamicImage::from(image).to_rgba8()
    }
}

pub trait KsmapPixel: Pixel<Subpixel = u8> {
    fn mtpng_color_type() -> mtpng::ColorType;
    fn image_color_type() -> image::ExtendedColorType;
    fn bytes_per_pixel() -> u8;
    fn bit_depth() -> u8 { 8 }
    fn to_repeated_byte(&self) -> Option<u8>;
}

impl KsmapPixel for Rgb<u8> {
    fn mtpng_color_type() -> mtpng::ColorType {
        mtpng::ColorType::Truecolor
    }
    
    fn image_color_type() -> image::ExtendedColorType {
        image::ExtendedColorType::Rgb8
    }
    
    fn bytes_per_pixel() -> u8 {
        3
    }
    
    fn to_repeated_byte(&self) -> Option<u8> {
        if self[0] == self[1] && self[0] == self[2] {
            Some(self[0])
        }
        else {
            None
        }
    }
}

impl KsmapPixel for Rgba<u8> {
    fn mtpng_color_type() -> mtpng::ColorType {
        mtpng::ColorType::TruecolorAlpha
    }
    
    fn image_color_type() -> image::ExtendedColorType {
        image::ExtendedColorType::Rgba8
    }
    
    fn bytes_per_pixel() -> u8 {
        4
    }
    
    fn to_repeated_byte(&self) -> Option<u8> {
        if self[0] == self[1] && self[0] == self[2] && self[0] == self[3] {
            Some(self[0])
        }
        else {
            None
        }
    }
}
