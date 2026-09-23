use rand::{RngExt, rngs::SmallRng};

use crate::definitions::{TransAlgorithm, TransParams};

pub fn trans_to_alpha(trans: u8) -> u8 {
    (128 - trans as u8).saturating_mul(2)
}

pub fn alpha_to_trans(alpha: u8) -> u8 {
    if alpha == 255 {
        0
    }
    else {
        128 - (alpha / 2)
    }
}

#[inline]
pub fn simulate(algo: TransAlgorithm, rng: &mut SmallRng, params: &TransParams, frames: u32) -> u8 {
    let trans = match algo {
        TransAlgorithm::Firefly => sim_firefly(rng, params, frames),
        TransAlgorithm::Ghost => sim_ghost(rng, params, frames),
        TransAlgorithm::FadeBlock => sim_fade_block(rng, params, frames),
        TransAlgorithm::Ray => sim_light_ray(rng, params, frames),
        TransAlgorithm::None => 128,
    };
    trans_to_alpha(trans)
}

pub fn sim_firefly(rng: &mut SmallRng, params: &TransParams, frames: u32) -> u8 {
    let trans_min = params.min as i32;
    let trans_max = params.max as i32;
    let mut trans = params.init as i32;
    
    for _ in 0..frames {
        let delta = rng.random_range(0..41) - 20;
        trans = (trans + delta).clamp(trans_min, trans_max);
    }
    
    trans as u8
}

pub fn sim_ghost(rng: &mut SmallRng, params: &TransParams, frames: u32) -> u8 {
    let trans_min = params.min as i32;
    let trans_max = params.max as i32;
    let mut trans = params.init as i32;
    
    for _ in 0..frames {
        let delta = rng.random_range(0..10) - 5;
        trans = (trans + delta).clamp(trans_min, trans_max);
    }
    
    trans as u8
}

pub fn sim_fade_block(rng: &mut SmallRng, params: &TransParams, frames: u32) -> u8 {
    let trans_min = params.min as i32;
    let trans_max = params.max as i32;
    let mut cycle = rng.random_range(0..180);
    
    for _ in 0..frames {
        cycle += rng.random_range(0..5) - 1;
        if cycle >= 180 {
            cycle -= 180;
        }
        else if cycle < 0 {
            cycle += 180;
        }
    }
    
    let delta = {
        let cycle_rad = (cycle as f32).to_radians();
        let delta = (trans_max - trans_min) as f32 * f32::sin(cycle_rad);
        delta as i32
    };
    let trans = (trans_min + delta).clamp(0, 128);
    
    trans as u8
}

pub fn sim_light_ray(rng: &mut SmallRng, params: &TransParams, frames: u32) -> u8 {
    let trans_min = params.min as i32;
    let trans_max = params.max as i32;
    let mut trans = params.init as i32;
    let mut fadeout = 0;
    let mut timer = 50;
    
    for _ in 0..frames {
        let range_max = 7 + fadeout;
        let delta = rng.random_range(0..range_max) - 3;
        trans = (trans + delta).clamp(trans_min, trans_max);
        
        timer -= 1;
        if timer == 0 {
            fadeout = rng.random_range(0..2);
            timer = 50;
        }
    }
    
    trans as u8
}
