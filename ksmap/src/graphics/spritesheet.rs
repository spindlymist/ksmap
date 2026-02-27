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
    pub tick_offset: u32,
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
                .unwrap_or(params.anim_from)
                .min(params.anim_to);
        
        let frame_range =
            if params.anim_repeat == 1 {
                anim_to..(anim_to + 1)
            }
            else {
                anim_loopback..(anim_to + 1)
            };
        let n_frames = frame_range.end - frame_range.start;
        
        let ticks_per_frame = 1000u32.div_ceil(params.anim_speed);
        let tick_offset =
            if anim_from != anim_loopback {
                let n_frames_first_rep = anim_to - anim_from + 1;
                n_frames_first_rep * ticks_per_frame
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
            tick_offset,
        }
    }
    
    pub fn frame(&self, i: u32) -> SubImage<&RgbaImage> {
        let frame_x = (i % self.frames_per_row) * self.frame_width;
        let frame_y = (i / self.frames_per_row) * self.frame_height;
        imageops::crop_imm(&self.image, frame_x, frame_y, self.frame_width, self.frame_height)
    }
    
    pub fn frame_at_time(&self, t: u32) -> SubImage<&RgbaImage> {
        let game_frame = t as u64 + (self.tick_offset / self.ticks_per_frame) as u64;
        let anim_frame = (game_frame % self.n_frames as u64) as u32;
        let i = self.frame_range.start + anim_frame;
        self.frame(i)
    }
    
    pub fn random_frame(&self, rng: &mut impl Rng) -> SubImage<&RgbaImage> {
        let i = rng.random_range(self.frame_range.clone());
        self.frame(i)
    }
}
