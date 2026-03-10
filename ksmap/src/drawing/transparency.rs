use rand::{Rng, rngs::SmallRng};

use crate::definitions::{TransAlgorithm, TransParams};

pub fn trans_to_alpha(trans: i32) -> f32 {
    (128 - trans) as f32 / 128.0
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
pub fn simulate(algo: TransAlgorithm, rng: &mut SmallRng, params: &TransParams, frames: u32) -> f32 {
    match algo {
        TransAlgorithm::Firefly => sim_firefly(rng, params, frames),
        TransAlgorithm::Ghost => sim_ghost(rng, params, frames),
        TransAlgorithm::FadeBlock => sim_fade_block(rng, params, frames),
        TransAlgorithm::Ray => sim_light_ray(rng, params, frames),
        TransAlgorithm::None => 1.0,
    }
}

pub fn sim_firefly(rng: &mut SmallRng, params: &TransParams, frames: u32) -> f32 {
    let trans_min = params.min as i32;
    let trans_max = params.max as i32;
    let mut trans = params.init as i32;
    
    for _ in 0..frames {
        let delta = rng.random_range(0..41) - 20;
        trans = (trans + delta).clamp(trans_min, trans_max);
    }
    
    trans_to_alpha(trans)
}

pub fn sim_ghost(rng: &mut SmallRng, params: &TransParams, frames: u32) -> f32 {
    let trans_min = params.min as i32;
    let trans_max = params.max as i32;
    let mut trans = params.init as i32;
    
    for _ in 0..frames {
        let delta = rng.random_range(0..10) - 5;
        trans = (trans + delta).clamp(trans_min, trans_max);
    }
    
    trans_to_alpha(trans)
}

pub fn sim_fade_block(rng: &mut SmallRng, params: &TransParams, frames: u32) -> f32 {
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
        let delta = 40.0 * f32::sin(cycle_rad);
        delta as i32
    };
    let trans = (88 + delta).clamp(trans_min, trans_max);
    
    trans_to_alpha(trans)
}

pub fn sim_light_ray(rng: &mut SmallRng, params: &TransParams, frames: u32) -> f32 {
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
    
    trans_to_alpha(trans)
}
