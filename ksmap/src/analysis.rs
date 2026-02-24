use std::collections::HashSet;

use libks::map_bin::{AssetId, LayerData, ScreenData};

use crate::definitions::{ObjectDefs, ObjectKind};
use crate::id::ObjectId;

pub struct AssetsUsed {
    pub tilesets: Vec<AssetId>,
    pub gradients: Vec<AssetId>,
    pub objects: Vec<ObjectId>,
}

pub fn list_assets(screens: &[ScreenData], defs: &ObjectDefs) -> AssetsUsed {
    let mut tilesets_seen = [false; 256];
    let mut gradients_seen = [false; 256];
    let mut objects_seen = HashSet::<ObjectId>::new();
    
    for screen in screens {
        let mut uses_tileset_a = false;
        let mut uses_tileset_b = false;
        
        for LayerData(layer) in &screen.layers[..4] {
            for tile in layer {
                uses_tileset_a |= tile.0 == 0 && tile.1 > 0;
                uses_tileset_b |= tile.0 == 1 && tile.1 > 0;
            }
        }
        
        for LayerData(layer) in &screen.layers[4..] {
            for tile in layer {
                if tile.1 == 0 { continue }
                let id = ObjectId::from(tile);
                objects_seen.insert(id.clone());
            }
        }
        
        tilesets_seen[screen.assets.tileset_a as usize] |= uses_tileset_a;
        tilesets_seen[screen.assets.tileset_b as usize] |= uses_tileset_b;
        gradients_seen[screen.assets.gradient as usize] = true;
    }
    
    // Look up original objects for OCOs and add them to the set
    let mut objects_original = HashSet::<ObjectId>::new();
    for id in &objects_seen {
        let Some(def) = defs.get(id) else { continue };
        if let ObjectKind::OverrideObject(tile_original) = &def.kind {
            objects_original.insert(ObjectId::from(tile_original));
        }
    }
    objects_seen.extend(objects_original.into_iter());
    
    let mut objects: Vec<_> = objects_seen.into_iter().collect();
    
    // Look up variants for objects and add them to the list
    for i in 0..objects.len() {
        for variant in defs.variants_of(objects[i].0) {
            objects.push(objects[i].to_variant(*variant));
        }
    }
    
    let mut tilesets = Vec::new();
    for i in 0..256 {
        if tilesets_seen[i] {
            tilesets.push(i as u8);
        }
    }
    
    let mut gradients = Vec::new();
    for i in 0..256 {
        if gradients_seen[i] {
            gradients.push(i as u8);
        }
    }
    
    AssetsUsed {
        tilesets,
        gradients,
        objects,
    }
}

pub fn count_laser_phases(screens: &[ScreenData], defs: &ObjectDefs) -> Vec<[usize; 2]> {
    let mut counts = vec![[0; 2]; screens.len()];
    let laser_objects: Vec<_> = defs.iter()
        .filter_map(|(id, def)| {
            let phase = def.sync.laser_phase?;
            Some((id.clone(), phase))
        })
        .collect();
    
    for (index_screen, screen) in screens.iter().enumerate() {
        for LayerData(layer) in &screen.layers[4..] {
            for tile in layer {
                if tile.1 == 0 { continue }
                for (id, phase) in &laser_objects {
                    if id.0 == *tile {
                        counts[index_screen][*phase as usize] += 1;
                        break;
                    }
                }
            }
        }
    }
    
    counts
}
