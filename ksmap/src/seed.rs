use std::{fmt::Display, hash::{Hash, Hasher}};

use rand::prelude::*;
use rustc_hash::FxHasher;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MapSeed {
    pub seed: u64,
}

/// DO NOT change the discriminants of this enum.
/// It will break the backwards compatibility of seeds.
#[derive(Debug, Hash)]
pub enum RngStep {
    Default = 0,
    // Synchronization
    ScreenAnimationTime = 1,
    GroupAnimationTime = 2,
    LaserPhases = 3,
    Limiters = 4,
    FxGenerator = 10,
    // Drawing
    Frame = 5,
    Offset = 6,
    Flip = 7,
    Alpha = 8,
    ElementalVariant = 9,
}

pub struct SeedHasher(FxHasher);

impl MapSeed {
    pub fn random() -> Self {
        let seed = rand::rng().next_u64();        
        Self { seed }
    }
    
    pub fn hasher(&self, step: RngStep) -> SeedHasher {
        let mut hasher = FxHasher::with_seed(self.seed as usize);
        hasher.write_u8(step as u8);
        SeedHasher(hasher)
    }
}

impl Into<String> for MapSeed {
    fn into(self) -> String {
        self.to_string()
    }
}

impl Display for MapSeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:0>16X}", self.seed)
    }
}

impl TryFrom<&str> for MapSeed {
    type Error = std::num::ParseIntError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        u64::from_str_radix(&value, 16)
            .map(|seed| MapSeed { seed })
    }
}

impl TryFrom<String> for MapSeed {
    type Error = std::num::ParseIntError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        MapSeed::try_from(value.as_str())
    }
}

impl SeedHasher {
    pub fn into_rng(self) -> SmallRng {
        let seed = self.0.finish();
        SmallRng::seed_from_u64(seed)
    }
    
    pub fn write<H: Hash>(mut self, value: H) -> Self {
        value.hash(&mut self.0);
        self
    }
    
    pub fn next_u32(self) -> u32 {
        self.into_rng().next_u32()
    }
    
    pub fn next_u64(self) -> u64 {
        self.into_rng().next_u64()
    }
    
    pub fn random<T>(self) -> T
    where
        rand::distr::StandardUniform: Distribution<T>
    {
        self.into_rng().random()
    }
}
