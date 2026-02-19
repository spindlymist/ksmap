mod ini_util;

use std::{fmt::Write, fs, ops::{Deref, DerefMut, Range, RangeInclusive}, path::Path};

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
    #[serde(default)]
    pub editor_only: bool,
    #[serde(flatten)]
    pub sync_params: SyncParams,
    #[serde(flatten)]
    pub draw_params: DrawParams,
    #[serde(default)]
    pub offset_combine: OffsetCombine,
    #[serde(default)]
    pub oco_support: OcoSupport,
    #[serde(default)]
    pub limit: Limit,
    pub color_base: Option<i32>,
    #[serde(default)]
    pub color_offsets: Vec<i32>,
    #[serde(skip)]
    pub replace_colors: Vec<([u8; 3], [u8; 3])>,
    pub override_key: Option<String>,
    pub override_frame_range: Option<Range<u32>>,
    #[serde(skip)]
    pub is_overridden: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum ObjectKind {
    #[default]
    Object,
    CustomObject,
    OverrideObject(Tile),
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SyncParams {
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

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub enum AnimSync {
    #[default]
    None,
    Screen,
    Group,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DrawParams {
    #[serde(default)]
    pub blend_mode: BlendMode,
    pub alpha_range: Option<RangeInclusive<u8>>,
    #[serde(default = "DrawParams::default_frame_size")]
    pub frame_size: (u32, u32),
    #[serde(default = "DrawParams::default_frame_range")]
    pub frame_range: Range<u32>,
    #[serde(default)]
    pub offset: (i32, i32),
    #[serde(default)]
    pub flip: Flip,
    #[serde(default)]
    pub flip_ocos: bool,
    pub flip_variant: Option<ObjectVariant>,
}

impl DrawParams {
    const fn default_frame_size() -> (u32, u32) {
        (24, 24)
    }
    
    const fn default_frame_range() -> Range<u32> {
        0..1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum OffsetCombine {
    #[default]
    Add,
    Replace,
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
            let def = table.try_into()?;
            
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
    const KEY_PREFIX: &'static str = "custom object ";
    const KEY_PREFIX_LEN: usize = KEY_PREFIX.len();
    const KEY_LEN_MAX: usize = "custom object b255".len();
    
    let mut key = String::with_capacity(KEY_LEN_MAX);
    key.push_str(KEY_PREFIX);
    
    for i in 1..=255 {
        key.truncate(KEY_PREFIX_LEN);
        write!(key, "{i}").unwrap();
        let id = ObjectId::from((255, i));
        if let Some(def) = ini.section(&key)
            .map(parse_co_props)
            .and_then(|props| create_co_def(id, props, defs))
        {
            defs.insert(id, def);
        }
        
        key.truncate(KEY_PREFIX_LEN);
        write!(key, "b{i}").unwrap();
        let id = ObjectId::from((254, i));
        if let Some(def) = ini.section(&key)
            .map(parse_co_props)
            .and_then(|props| create_co_def(id, props, defs))
        {
            defs.insert(id, def);
        }
    }
    
    // Handle special graphics overrides for coins, artifacts, and powerups
    // This needs to be done after parsing CO defs so OCOs don't inherit the changes
    if let Some(world_section) = ini.section("World") {
        for def in defs.values_mut() {
            if let Some(override_key) = &def.override_key
                && let Some(override_path) = world_section.get(override_key)
                && !override_path.is_empty()
            {
                def.is_overridden = true;
                def.path.replace(override_path.to_owned());
                if let Some(frame_range) = def.override_frame_range.take() {
                    def.draw_params.frame_range = frame_range;
                }
            }
        }
    }
}

fn create_co_def(id: ObjectId, props: CustomObjectProps, defs: &ObjectDefs) -> Option<ObjectDef> {
    match (props.bank, props.object) {
        (_, 0) => {
            create_regular_co_def(props)
        }
        (0..=253, 1..=255) => {
            let bank = props.bank as u8;
            let object = props.object as u8;
            let oco_id = ObjectId::from((bank, object));
            let def = defs.get(&oco_id);
            match def.map(|def| def.oco_support) {
                Some(OcoSupport::Full | OcoSupport::NoCustomGraphics) => {
                    create_oco_def(id, oco_id, props, def.unwrap())
                }
                _ => create_botched_oco_def(props)
            }
        }
        _ => create_botched_oco_def(props)
    }
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
        anim_speed: _, // will be used soon
        anim_repeat,
        ..
    } = props;
    
    if image == "" {
        return None;
    }
    
    let frame_range = if anim_repeat == 0 {
            anim_loopback..(anim_to + 1)
        }
        else {
            anim_to..anim_to + 1
        };
    
    let sync_params = SyncParams {
        sync_to: AnimSync::Screen,
        ..Default::default()
    };
    
    let draw_params = DrawParams {
        blend_mode: BlendMode::Over,
        alpha_range: None,
        frame_size: (tile_width, tile_height),
        frame_range,
        offset: (offset_x, offset_y),
        flip: Flip::Never,
        flip_ocos: false,
        flip_variant: None,
    };

    Some(ObjectDef {
        kind: ObjectKind::CustomObject,
        path: Some(image),
        editor_only: false,
        sync_params,
        draw_params,
        offset_combine: OffsetCombine::Replace,
        oco_support: OcoSupport::None,
        limit: Limit::None,
        color_base: None,
        color_offsets: Vec::new(),
        replace_colors: Vec::new(),
        override_key: None,
        override_frame_range: None,
        is_overridden: false,
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
    assert!(def.oco_support != OcoSupport::None);
    
    if image == "" && def.oco_support != OcoSupport::NoCustomGraphics {
        return None;
    }
    
    let sync_params = {
        let mut sync_north = Vec::new();
        if def.sync_params.sync_north.contains(&oco_id) {
            sync_north.push(id);
        }
        
        let mut sync_south = Vec::new();
        if def.sync_params.sync_south.contains(&oco_id) {
            sync_south.push(id);
        }
        
        let mut sync_west = Vec::new();
        if def.sync_params.sync_west.contains(&oco_id) {
            sync_west.push(id);
        }
        
        let mut sync_east = Vec::new();
        if def.sync_params.sync_east.contains(&oco_id) {
            sync_east.push(id);
        }
        
        SyncParams {
            sync_to: def.sync_params.sync_to,
            sync_west,
            sync_east,
            sync_north,
            sync_south,
            sync_offset: def.sync_params.sync_offset,
            laser_phase: def.sync_params.laser_phase,
        }
    };
    
    let draw_params = {
        if def.oco_support == OcoSupport::NoCustomGraphics {
            tile_width = def.draw_params.frame_size.0;
            tile_height = def.draw_params.frame_size.1;
        }
        
        if def.offset_combine == OffsetCombine::Add {
            offset_x += def.draw_params.offset.0;
            offset_y += def.draw_params.offset.1;
        }
        
        let flip = if def.draw_params.flip_ocos {
                Flip::Always
            }
            else {
                def.draw_params.flip
            };
        
        DrawParams {
            blend_mode: BlendMode::Over,
            alpha_range: def.draw_params.alpha_range.clone(),
            frame_size: (tile_width, tile_height),
            frame_range: def.draw_params.frame_range.clone(),
            offset: (offset_x, offset_y),
            flip,
            flip_ocos: def.draw_params.flip_ocos,
            flip_variant: None,
        }
    };
    
    let mut replace_colors = Vec::new();
    if let Some(color_base) = def.color_base {
        for offset in [0].iter().chain(def.color_offsets.iter()) {
            let old_color = unpack_color(color_base + offset);
            let new_color = unpack_color(color + offset);
            replace_colors.push((old_color, new_color));
        }
    }
    
    Some(ObjectDef {
        kind: ObjectKind::OverrideObject(oco_id.0),
        path: Some(image),
        editor_only: def.editor_only,
        sync_params,
        draw_params,
        offset_combine: def.offset_combine,
        oco_support: def.oco_support,
        limit: def.limit,
        color_base: None,
        color_offsets: Vec::new(),
        replace_colors,
        override_key: None,
        override_frame_range: None,
        is_overridden: false,
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
