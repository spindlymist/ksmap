use std::{ops::Range, rc::Rc};

use image::{RgbaImage, SubImage, imageops};
use rand::Rng;

use crate::definitions::AnimParams;

pub struct Spritesheet {
    pub image: Rc<RgbaImage>,
    pub n_frames: u32,
    pub frames_per_row: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    pub frame_range: Range<u32>,
    pub ticks_per_frame: u32,
    pub frames_offset: u32,
}

impl Spritesheet {
    pub fn new(image: Rc<RgbaImage>, params: &AnimParams) -> Self {
        let frame_width = params.frame_size.0.min(image.width());
        let frame_height = params.frame_size.1.min(image.height());
        
        let frames_per_row = image.width() / frame_width;
        let n_rows = image.height() / frame_height;
        let n_frames_total = frames_per_row * n_rows;
        
        let anim_to = params.anim_to.min(n_frames_total - 1);
        let anim_from = params.anim_from.min(anim_to);
        let anim_loopback = params.anim_loopback
                .unwrap_or(anim_from)
                .min(anim_to);
        
        let mut frame_range =
            if params.anim_repeat == 1 {
                anim_to..(anim_to + 1)
            }
            else {
                anim_loopback..(anim_to + 1)
            };
        if frame_range.is_empty() {
            // This shouldn't happen, but Rng::random_range will panic
            // if the range is empty
            frame_range.end += 1;
        }
        let n_frames = frame_range.end - frame_range.start;
        
        let ticks_per_frame = 1000u32.div_ceil(params.anim_speed);
        let frames_offset =
            if anim_from != anim_loopback {
                anim_to - anim_from + 1
            }
            else {
                0
            };
        
        Self {
            image,
            n_frames,
            frames_per_row,
            frame_width,
            frame_height,
            frame_range,
            ticks_per_frame,
            frames_offset,
        }
    }
    
    pub fn frame(&self, i: u32) -> SubImage<&RgbaImage> {
        let frame_x = (i % self.frames_per_row) * self.frame_width;
        let frame_y = (i / self.frames_per_row) * self.frame_height;
        imageops::crop_imm(&self.image, frame_x, frame_y, self.frame_width, self.frame_height)
    }
    
    pub fn frame_at_time(&self, t: u32) -> SubImage<&RgbaImage> {
        let offset = ((t / self.ticks_per_frame) - self.frames_offset) % self.n_frames;
        let i = self.frame_range.start + offset;
        self.frame(i)
    }
    
    pub fn random_frame(&self, rng: &mut impl Rng) -> SubImage<&RgbaImage> {
        let i = rng.random_range(self.frame_range.clone());
        self.frame(i)
    }
}
