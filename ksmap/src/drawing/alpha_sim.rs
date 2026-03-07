use rand::{Rng, rngs::SmallRng};

fn trans_to_alpha(trans: i32) -> f32 {
    (128 - trans) as f32 / 128.0
}

pub fn sim_firefly(rng: &mut SmallRng, frames: u32, trans_max: u8) -> f32 {
    let trans_max = trans_max as i32;
    let mut trans = 100;
    
    for _ in 0..frames {
        let delta = rng.random_range(0..41) - 20;
        trans = (trans + delta).clamp(0, trans_max);
    }
    
    trans_to_alpha(trans)
}

pub fn sim_ghost(rng: &mut SmallRng, frames: u32, trans_min: u8, trans_max: u8) -> f32 {
    let trans_min = trans_min as i32;
    let trans_max = trans_max as i32;
    let mut trans = 100;
    
    for _ in 0..frames {
        let delta = rng.random_range(0..10) - 5;
        trans = (trans + delta).clamp(trans_min, trans_max);
    }
    
    trans_to_alpha(trans)
}

pub fn sim_fade_block(rng: &mut SmallRng, frames: u32, trans_max: u8) -> f32 {
    let trans_max = trans_max as i32;
    let mut trans = 100;
    let mut cycle = rng.random_range(0..180);
    
    for _ in 0..frames {
        let delta = {
            let cycle_rad = (cycle as f32).to_radians();
            let delta = 40.0 * f32::sin(cycle_rad);
            delta as i32
        };
        trans = (88 + delta).min(trans_max);
        
        cycle += rng.random_range(0..5) - 1;
        if cycle >= 180 {
            cycle -= 180;
        }
        else if cycle < 0 {
            cycle += 180;
        }
    }
    
    trans_to_alpha(trans)
}

pub fn sim_light_ray(rng: &mut SmallRng, frames: u32, trans_max: u8) -> f32 {
    let trans_max = trans_max as i32;
    let mut trans = 115;
    let mut fadeout = 0;
    
    for _ in 0..frames {
        let range_max = 7 + fadeout;
        let delta = rng.random_range(0..range_max) - 3;
        trans = (trans + delta).clamp(108, trans_max);
        fadeout = rng.random_range(0..2);
    }
    
    trans_to_alpha(trans)
}
