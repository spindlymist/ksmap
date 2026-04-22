use std::fmt::{self, Display};

use libks::map_bin::Tile;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct ObjectId(pub Tile, pub ObjectVariant);

impl ObjectId {
    pub fn into_variant(mut self, variant: ObjectVariant) -> Self {
        self.1 = variant;
        self
    }

    pub fn to_variant(&self, variant: ObjectVariant) -> Self {
        Self(self.0, variant)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
#[serde(try_from = "String")]
pub enum ObjectVariant {
    #[default]
    None,
    Left,
    Glow,
    Spot,
    Floor,
    Circle,
    Square,
    A,
    B,
    C,
    D,
    Placeholder,
}

#[derive(Debug, thiserror::Error)]
pub enum ObjectIdParseError {
    #[error("Invalid ObjectId: missing bank/object separator")]
    MissingSeparator(String),
    #[error("Invalid ObjectId: failed to parse bank or object index")]
    InvalidIndex(String),
    #[error(transparent)]
    ObjectVariantParse(#[from] ObjectVariantParseError),
}

#[derive(Debug, thiserror::Error)]
pub enum ObjectVariantParseError {
    #[error("Unknown object variant: {0}")]
    UnknownVariant(String),
}

impl Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.1 {
            ObjectVariant::None => write!(f, "{}-{}", self.0.0, self.0.1),
            _ => write!(f, "{}-{} {}", self.0.0, self.0.1, self.1),
        }
    }
}

impl From<(u8, u8)> for ObjectId {
    fn from(value: (u8, u8)) -> Self {
        Self(Tile(value.0, value.1), ObjectVariant::None)
    }
}

impl From<Tile> for ObjectId {
    fn from(value: Tile) -> Self {
        Self(value, ObjectVariant::None)
    }
}

impl From<&Tile> for ObjectId {
    fn from(value: &Tile) -> Self {
        Self(*value, ObjectVariant::None)
    }
}

impl TryFrom<&str> for ObjectId {
    type Error = ObjectIdParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let (bank_and_index, variant) = match value.split_once(' ') {
            Some((id, variant)) => (id, ObjectVariant::try_from(variant)?),
            None => (value, ObjectVariant::None),
        };

        let Some((bank, index)) = bank_and_index.split_once('-') else {
            return Err(ObjectIdParseError::MissingSeparator(bank_and_index.to_owned()));
        };

        let bank = match str::parse::<u8>(bank) {
            Ok(bank) => bank,
            Err(_) => return Err(ObjectIdParseError::InvalidIndex(bank.to_owned())),
        };

        let index = match str::parse::<u8>(index) {
            Ok(index) => index,
            Err(_) => return Err(ObjectIdParseError::InvalidIndex(index.to_owned())),
        };

        Ok(ObjectId(Tile(bank, index), variant))
    }
}

impl TryFrom<String> for ObjectId {
    type Error = ObjectIdParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl Display for ObjectVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ObjectVariant::None => "",
            ObjectVariant::Left => "Left",
            ObjectVariant::Glow => "Glow",
            ObjectVariant::Spot => "Spot",
            ObjectVariant::Floor => "Floor",
            ObjectVariant::Circle => "Circle",
            ObjectVariant::Square => "Square",
            ObjectVariant::A => "A",
            ObjectVariant::B => "B",
            ObjectVariant::C => "C",
            ObjectVariant::D => "D",
            ObjectVariant::Placeholder => "Placeholder",
        };
        f.write_str(s)
    }
}

impl TryFrom<&str> for ObjectVariant {
    type Error = ObjectVariantParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let variant = match value {
            "" => ObjectVariant::None,
            "Left" => ObjectVariant::Left,
            "Glow" => ObjectVariant::Glow,
            "Spot" => ObjectVariant::Spot,
            "Floor" => ObjectVariant::Floor,
            "Circle" => ObjectVariant::Circle,
            "Square" => ObjectVariant::Square,
            "A" => ObjectVariant::A,
            "B" => ObjectVariant::B,
            "C" => ObjectVariant::C,
            "D" => ObjectVariant::D,
            "Placeholder" => ObjectVariant::Placeholder,
            _ => return Err(ObjectVariantParseError::UnknownVariant(value.to_owned())),
        };
        Ok(variant)
    }
}

impl TryFrom<String> for ObjectVariant {
    type Error = ObjectVariantParseError;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        ObjectVariant::try_from(value.as_str())
    }
}
