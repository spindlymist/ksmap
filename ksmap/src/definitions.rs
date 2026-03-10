mod ini_util;

use std::{fmt::Write, fs, ops::{Deref, DerefMut}, path::Path};

use anyhow::Result;
use libks::map_bin::Tile;
use libks_ini::{Ini, VirtualSection};
use rustc_hash::FxHashMap;
use serde::Deserialize;

use crate::{
    drawing::BlendMode,
    id::{ObjectId, ObjectVariant},
};
use ini_util::{unpack_color, VirtualSectionExt};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ObjectDef {
    #[serde(skip)]
    pub kind: ObjectKind,
    pub path: Option<String>,
    #[serde(flatten)]
    pub base: BaseParams,
    #[serde(flatten)]
    pub sync: SyncParams,
    #[serde(flatten)]
    pub draw: DrawParams,
    #[serde(flatten)]
    pub anim: AnimParams,
    #[serde(default)]
    pub editor_only: bool,
    #[serde(skip)]
    pub replace_colors: Vec<ColorReplacement>,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum ObjectKind {
    #[default]
    Object,
    CustomObject,
    OverrideObject(Tile),
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BaseParams {
    #[serde(default)]
    pub oco_support: OcoSupport,
    pub oco_offset: Option<(i32, i32)>,
    #[serde(default)]
    pub flip_ocos: bool,
    #[serde(default)]
    pub no_oco_black_transparency: bool,
    pub color_base: Option<i32>,
    #[serde(default)]
    pub color_offsets: Vec<i32>,
    pub override_key: Option<String>,
    pub override_anim_range: Option<AnimRange>,
    pub override_anim_speed: Option<u32>,
    #[serde(skip)]
    pub is_overridden: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SyncParams {
    #[serde(default)]
    pub limit: Limit,
    #[serde(default)]
    pub sync_to: AnimSync,
    #[serde(default)]
    pub sync_west: Vec<ObjectId>,
    #[serde(default)]
    pub sync_east: Vec<ObjectId>,
    #[serde(default)]
    pub sync_north: Vec<ObjectId>,
    #[serde(default)]
    pub sync_south: Vec<ObjectId>,
    #[serde(default)]
    pub sync_offset: u32,
    #[serde(default)]
    pub laser_phase: Option<LaserPhase>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DrawParams {
    #[serde(default)]
    pub blend_mode: BlendMode,
    #[serde(default, rename = "transparency_algo")]
    pub trans_algo: TransAlgorithm,
    #[serde(default, flatten)]
    pub trans: TransParams,
    #[serde(default)]
    pub offset: (i32, i32),
    #[serde(default)]
    pub flip: Flip,
    pub flip_variant: Option<ObjectVariant>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AnimParams {
    #[serde(default = "AnimParams::default_frame_size")]
    pub frame_size: (u32, u32),
    #[serde(default)]
    pub anim_from: u32,
    #[serde(default)]
    pub anim_to: u32,
    pub anim_loopback: Option<u32>,
    #[serde(default = "AnimParams::default_anim_speed")]
    pub anim_speed: u32,
    #[serde(default)]
    pub anim_repeat: u32,
}

impl AnimParams {
    const fn default_frame_size() -> (u32, u32) {
        (24, 24)
    }
    
    const fn default_anim_speed() -> u32 {
        1000
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum TransAlgorithm {
    #[default]
    None,
    Firefly,
    Ghost,
    FadeBlock,
    Ray,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct TransParams {
    #[serde(rename = "transparency_init", default)]
    pub init: u8,
    #[serde(rename = "transparency_min", default)]
    pub min: u8,
    #[serde(rename = "transparency_max", default = "TransParams::default_max")]
    pub max: u8,
}

impl Default for TransParams {
    fn default() -> Self {
        Self {
            init: 0,
            min: 0,
            max: Self::default_max(),
        }
    }
}

impl TransParams {
    const fn default_max() -> u8 {
        128
    }
    
    pub fn sanitize(&mut self) {
        self.max = self.max.min(128);
        self.min = self.min.min(self.max);
        self.init = self.init.clamp(self.min, self.max);
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub enum AnimSync {
    #[default]
    None,
    Screen,
    Group,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum OcoSupport {
    #[default]
    Full,
    NoCustomGraphics,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(tag = "pick")]
pub enum Limit {
    #[default]
    None,
    First { n: usize },
    Random { n: usize },
    LogNPlusOne,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Flip {
    #[default]
    Never,
    Random,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum LaserPhase {
    #[default]
    Red,
    Green,
}

#[derive(Debug, Clone)]
pub struct ColorReplacement {
    pub old: [u8; 3],
    pub new: [u8; 3],
    pub is_transparent: bool,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct AnimRange {
    pub from: u32,
    pub to: u32,
}

pub struct ObjectDefs {
    pub defs: FxHashMap<ObjectId, ObjectDef>,
    pub variants: FxHashMap<Tile, Vec<ObjectVariant>>,
}

impl ObjectDefs {
    pub fn variants_of(&self, object: Tile) -> &[ObjectVariant] {
        match self.variants.get(&object) {
            Some(variants) => &variants,
            None => &[],
        }
    }
}

impl Deref for ObjectDefs {
    type Target = FxHashMap<ObjectId, ObjectDef>;

    fn deref(&self) -> &Self::Target {
        &self.defs
    }
}

impl DerefMut for ObjectDefs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.defs
    }
}

pub fn load_object_defs(path: impl AsRef<Path>) -> Result<ObjectDefs> {
    let mut defs = FxHashMap::<ObjectId, ObjectDef>::default();
    let mut variants = FxHashMap::<Tile, Vec<ObjectVariant>>::default();

    let raw = fs::read_to_string(path)?;
    let table: toml::Table = raw.parse()?;

    for (key, value) in table.into_iter() {
        if let toml::Value::Table(table) = value {
            let id = ObjectId::try_from(key)?;
            let mut def: ObjectDef = table.try_into()?;
            def.draw.trans.sanitize();
            
            if id.1 != ObjectVariant::None {
                variants.entry(id.0)
                    .or_insert(Vec::new())
                    .push(id.1);
            }
            
            defs.insert(id, def);
        }
    }

    Ok(ObjectDefs {
        defs,
        variants,
    })
}

pub fn insert_custom_obj_defs(defs: &mut ObjectDefs, ini: &Ini) {
    // KS only supports a single frame size per unique Image property (case insensitive)
    // Even if they refer to the same file, different strings can have different sizes
    // Most custom graphics in the [World] section also lock frame sizes
    // Priority:
    //     - The [World] section has the highest priority
    //     - Lower indices have priority over higher indices
    //     - Bank 255 has priority over bank 254 at the same index
    let mut locked_sizes = FxHashMap::<String, (u32, u32)>::default();
    if let Some(world_section) = ini.section("World") {
        for key in [
            "Coin",
            "Powers",
            "Artifact1",
            "Artifact2",
            "Artifact3",
            "Artifact4",
            "Artifact5",
            "Artifact6",
            "Artifact7",
        ] {
            if let Some(path) = world_section.get(key) {
                let path_lower = path.to_ascii_lowercase();
                locked_sizes.insert(path_lower, (24, 24));
            }
        }
    }
    
    const KEY_PREFIX: &'static str = "custom object ";
    const KEY_LEN_MAX: usize = "custom object b255".len();
    let mut key = String::with_capacity(KEY_LEN_MAX);
    key.push_str(KEY_PREFIX);
    
    for i in 1..=255 {
        key.truncate(KEY_PREFIX.len());
        let _ = write!(key, "{i}");
        let id = ObjectId::from((255, i));
        if let Some(def) = co_def_from_ini(&key, id, ini, defs, &mut locked_sizes) {
            defs.insert(id, def);
        }
        
        key.truncate(KEY_PREFIX.len());
        let _ = write!(key, "b{i}");
        let id = ObjectId::from((254, i));
        if let Some(def) = co_def_from_ini(&key, id, ini, defs, &mut locked_sizes) {
            defs.insert(id, def);
        }
    }
    
    // Handle special graphics overrides for coins, artifacts, and powerups
    // This needs to be done after parsing CO defs so OCOs don't inherit the changes
    if let Some(world_section) = ini.section("World") {
        for def in defs.values_mut() {
            if let Some(override_key) = &def.base.override_key
                && let Some(override_path) = world_section.get(override_key)
                && !override_path.is_empty()
            {
                def.base.is_overridden = true;
                def.path.replace(override_path.to_owned());
                if let Some(anim_range) = def.base.override_anim_range.take() {
                    def.anim.anim_from = anim_range.from;
                    def.anim.anim_loopback = Some(anim_range.from);
                    def.anim.anim_to = anim_range.to;
                }
                if let Some(anim_speed) = def.base.override_anim_speed.take() {
                    def.anim.anim_speed = anim_speed;
                }
            }
        }
    }
}

fn co_def_from_ini(
    key: &str,
    id: ObjectId,
    ini: &Ini,
    defs: &ObjectDefs,
    locked_sizes: &mut FxHashMap<String, (u32, u32)>
) -> Option<ObjectDef> {
    let section = ini.section(key)?;
    let props = parse_co_props(section);
    let tile_width = props.tile_width;
    let tile_height = props.tile_height;
    
    let mut def = match (props.bank, props.object) {
        (_, 0) => {
            create_regular_co_def(props)
        }
        (0..=253, 1..=255) => {
            let bank = props.bank as u8;
            let object = props.object as u8;
            let oco_id = ObjectId::from((bank, object));
            let oco_def = defs.get(&oco_id);
            match oco_def.map(|def| def.base.oco_support) {
                Some(OcoSupport::Full | OcoSupport::NoCustomGraphics) => {
                    create_oco_def(id, oco_id, props, oco_def.unwrap())
                }
                _ => create_botched_oco_def(props)
            }
        }
        _ => create_botched_oco_def(props)
    };
    
    if let Some(def) = &mut def
        && let Some(path) = &def.path
    {
        let path_lower = path.to_ascii_lowercase();
        // It's important that the Tile Width and Tile Height properties are
        // used here, not the frame size from the definition.
        // KS+ indiscriminately loads all CO images and locks the frame size to
        // the values in World.ini even for OCOs that don't use those graphics
        // and inherit a different frame size from the base object.
        let new_size = locked_sizes.entry(path_lower)
            .or_insert((tile_width, tile_height));
        if def.base.oco_support != OcoSupport::NoCustomGraphics {
            def.anim.frame_size = *new_size;
        }
    }
    
    def
}

fn create_regular_co_def(props: CustomObjectProps) -> Option<ObjectDef> {   
    let CustomObjectProps {
        image,
        tile_width,
        tile_height,
        offset_x,
        offset_y,
        anim_from,
        anim_to,
        anim_loopback,
        anim_speed,
        anim_repeat,
        ..
    } = props;
    
    if image == "" {
        return None;
    }
    
    let sync_params = SyncParams {
        limit: Limit::None,
        sync_to: AnimSync::Screen,
        ..Default::default()
    };
    
    let draw_params = DrawParams {
        blend_mode: BlendMode::Over,
        trans_algo: TransAlgorithm::None,
        trans: TransParams::default(),
        offset: (offset_x, offset_y),
        flip: Flip::Never,
        flip_variant: None,
    };
    
    let anim_params = AnimParams {
        frame_size: (tile_width, tile_height),
        anim_from,
        anim_to,
        anim_loopback: Some(anim_loopback),
        anim_speed,
        anim_repeat,
    };

    Some(ObjectDef {
        kind: ObjectKind::CustomObject,
        path: Some(image),
        base: BaseParams::default(),
        sync: sync_params,
        draw: draw_params,
        anim: anim_params,
        editor_only: false,
        replace_colors: Vec::new(),
    })
}

#[inline]
fn create_botched_oco_def(mut props: CustomObjectProps) -> Option<ObjectDef> {
    props.anim_from = 0;
    props.anim_to = 0;
    props.anim_loopback = 0;
    create_regular_co_def(props)
}

fn create_oco_def(id: ObjectId, oco_id: ObjectId, props: CustomObjectProps, def: &ObjectDef) -> Option<ObjectDef> {
    let CustomObjectProps {
        image,
        bank,
        object,
        mut tile_width,
        mut tile_height,
        mut offset_x,
        mut offset_y,
        color,
        ..
    } = props;
    
    assert!(bank < 254);
    assert!(object > 0);
    assert!(def.base.oco_support != OcoSupport::None);
    
    if image == "" && def.base.oco_support != OcoSupport::NoCustomGraphics {
        return None;
    }
    
    let sync_params = {
        let mut sync_north = Vec::new();
        if def.sync.sync_north.contains(&oco_id) {
            sync_north.push(id);
        }
        
        let mut sync_south = Vec::new();
        if def.sync.sync_south.contains(&oco_id) {
            sync_south.push(id);
        }
        
        let mut sync_west = Vec::new();
        if def.sync.sync_west.contains(&oco_id) {
            sync_west.push(id);
        }
        
        let mut sync_east = Vec::new();
        if def.sync.sync_east.contains(&oco_id) {
            sync_east.push(id);
        }
        
        SyncParams {
            limit: def.sync.limit,
            sync_to: def.sync.sync_to,
            sync_west,
            sync_east,
            sync_north,
            sync_south,
            sync_offset: def.sync.sync_offset,
            laser_phase: def.sync.laser_phase,
        }
    };
    
    let draw_params = {
        let base_offset = def.base.oco_offset.unwrap_or(def.draw.offset);
        offset_x += base_offset.0;
        offset_y += base_offset.1;
        
        let flip = if def.base.flip_ocos {
                Flip::Always
            }
            else {
                def.draw.flip
            };
        
        DrawParams {
            blend_mode: BlendMode::Over,
            trans_algo: def.draw.trans_algo,
            trans: def.draw.trans,
            offset: (offset_x, offset_y),
            flip,
            flip_variant: None,
        }
    };
    
    let anim_params = {
        if def.base.oco_support == OcoSupport::NoCustomGraphics {
            tile_width = def.anim.frame_size.0;
            tile_height = def.anim.frame_size.1;
        }
        
        AnimParams {
            frame_size: (tile_width, tile_height),
            ..def.anim
        }
    };
    
    let mut replace_colors = Vec::new();
    if let Some(color_base) = def.base.color_base {
        for offset in [0].iter().chain(def.base.color_offsets.iter()) {
            let old = unpack_color(color_base + offset);
            let new = unpack_color(color + offset);
            let is_transparent = !def.base.no_oco_black_transparency && new == [0, 0, 0];
            replace_colors.push(ColorReplacement {
                old,
                new,
                is_transparent,
            });
        }
    }
    
    Some(ObjectDef {
        kind: ObjectKind::OverrideObject(oco_id.0),
        path: Some(image),
        base: BaseParams {
            oco_support: def.base.oco_support,
            ..Default::default()
        },
        sync: sync_params,
        draw: draw_params,
        anim: anim_params,
        editor_only: def.editor_only,
        replace_colors,
    })
}

struct CustomObjectProps {
    pub image: String,
    pub bank: i32,
    pub object: i32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub anim_from: u32,
    pub anim_to: u32,
    pub anim_loopback: u32,
    pub anim_speed: u32,
    pub anim_repeat: u32,
    pub color: i32,
}

fn parse_co_props(section: VirtualSection<'_>) -> CustomObjectProps {
    let image         = section.get_owned_or_default("image");
    let bank          = section.get_i32_or("bank", 0);
    let object        = section.get_i32_or("object", 0);
    let tile_width    = section.get_i32_or("tile width", 24).max(0) as u32;
    let tile_height   = section.get_i32_or("tile height", 24).max(0) as u32;
    let offset_x      = section.get_i32_or("offset x", 0);
    let offset_y      = section.get_i32_or("offset y", 0);
    let anim_from     = section.get_i32_or("init animfrom", 0).max(0) as u32;
    let anim_to       = section.get_i32_or("init animto", 0).max(0) as u32;
    let anim_loopback = section.get_i32_or("init animloopback", 0).max(0) as u32;
    let anim_speed    = section.get_i32_or("init animspeed", 500).clamp(1, 1000) as u32;
    let anim_repeat   = section.get_i32_or("init animrepeat", 0).max(0) as u32;
    let color         = section.get_i32_or("color", 8);
    
    CustomObjectProps {
        image,
        bank,
        object,
        tile_width,
        tile_height,
        offset_x,
        offset_y,
        anim_from,
        anim_to,
        anim_loopback,
        anim_speed,
        anim_repeat,
        color,
    }
}
