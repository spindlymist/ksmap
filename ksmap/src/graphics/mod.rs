use std::{
    collections::hash_map::Entry,
    fs::OpenOptions,
    io::{self, BufReader},
    path::{Path, PathBuf},
    rc::Rc,
};

use anyhow::{Context, Result};
use image::{DynamicImage, Rgba, RgbaImage};
use libks::map_bin::AssetId;
use rustc_hash::FxHashMap;

use crate::{
    definitions::{ColorReplacement, ObjectDef, ObjectDefs, ObjectKind, OcoSupport},
    id::{ObjectId, ObjectVariant},
};

mod png_decoder;

type MaybeImage = Option<Rc<RgbaImage>>;

pub struct Graphics<'a> {
    paths: Paths,
    object_defs: &'a ObjectDefs,
    cache: FxHashMap<(PathBuf, MagicColor), MaybeImage>,
    tilesets: FxHashMap<AssetId, Rc<RgbaImage>>,
    gradients: FxHashMap<AssetId, Rc<RgbaImage>>,
    objects: FxHashMap<ObjectId, Rc<RgbaImage>>,
}

pub struct Paths {
    data_tilesets: PathBuf,
    data_gradients: PathBuf,
    editor_objects: PathBuf,
    level_tilesets: PathBuf,
    level_gradients: PathBuf,
    custom_objects: PathBuf,
    templates: PathBuf,
}

impl Paths {
    pub fn new(data_dir: impl AsRef<Path>, level_dir: impl AsRef<Path>, templates_dir: PathBuf) -> Self {
        Self {
            data_tilesets: data_dir.as_ref().join("Tilesets"),
            data_gradients: data_dir.as_ref().join("Gradients"),
            editor_objects: data_dir.as_ref().join("Objects"),
            level_tilesets: level_dir.as_ref().join("Tilesets"),
            level_gradients: level_dir.as_ref().join("Gradients"),
            custom_objects: level_dir.as_ref().join("Custom Objects"),
            templates: templates_dir,
        }
    }
}

impl<'a> Graphics<'a> {
    pub fn new(
        data_dir: impl AsRef<Path>,
        level_dir: impl AsRef<Path>,
        templates_dir: impl AsRef<Path>,
        object_defs: &'a ObjectDefs,
    ) -> Self {
        let paths = Paths::new(
            data_dir.as_ref().to_owned(),
            level_dir.as_ref().to_owned(),
            templates_dir.as_ref().to_owned(),
        );

        Self {
            paths,
            object_defs,
            cache: FxHashMap::default(),
            tilesets: FxHashMap::default(),
            gradients: FxHashMap::default(),
            objects: FxHashMap::default(),
        }
    }
    
    pub fn tileset(&self, id: AssetId) -> Option<&RgbaImage> {
        self.tilesets.get(&id)
            .map(Rc::as_ref)
    }

    pub fn gradient(&self, id: AssetId) -> Option<&RgbaImage> {
        self.gradients.get(&id)
            .map(Rc::as_ref)
    }

    pub fn object(&self, id: &ObjectId) -> Option<&RgbaImage> {
        self.objects.get(&id)
            .map(Rc::as_ref)
    }
    
    pub fn load_tilesets(&mut self, ids: &[AssetId]) -> Result<()> {
        for id in ids {
            if let Some(image) = self.load_tileset(*id)? {
                self.tilesets.insert(*id, image);
            }
        }
        Ok(())
    }
    
    pub fn load_gradients(&mut self, ids: &[AssetId]) -> Result<()> {
        for id in ids {
            if let Some(image) = self.load_gradient(*id)? {
                self.gradients.insert(*id, image);
            }
        }
        Ok(())
    }
    
    pub fn load_objects(&mut self, ids: &[ObjectId]) -> Result<()> {
        for id in ids {
            let def = self.object_defs.get(id);
            let image = match def.map(|def| &def.kind) {
                Some(ObjectKind::Object) | None => self.load_stock_object(id, def)?,
                Some(ObjectKind::CustomObject) => self.load_custom_object(def.unwrap())?,
                Some(ObjectKind::OverrideObject(_)) => self.load_override_object(def.unwrap())?,
            };
            if let Some(image) = image {
                self.objects.insert(id.clone(), image);
            }
        }
        Ok(())
    }
    
    fn load_image(&mut self, path: PathBuf, magic_color: MagicColor) -> Result<MaybeImage> {
        let cached_image = match self.cache.entry((path, magic_color)) {
            Entry::Occupied(entry) => {
                return Ok(entry.get()
                    .as_ref()
                    .map(Rc::clone))
            },
            Entry::Vacant(entry) => {
                entry
            },
        };
        let (path, magic_color) = cached_image.key();
        
        let file = match OpenOptions::new().read(true).open(path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                cached_image.insert(None);
                return Ok(None);
            },
            Err(err) => Err(err)?,
        };
        let decoder = png_decoder::PngDecoder::new(BufReader::new(file))
            .with_context(|| format!("Error while decoding {path:?}"))?;
        let image = DynamicImage::from_decoder(decoder)
            .with_context(|| format!("Error while decoding {path:?}"))?;

        let is_24_bpp = matches!(image, DynamicImage::ImageRgb8(_));
        let mut image = image.into_rgba8();

        if is_24_bpp || magic_color.force {
            for pixel in image.pixels_mut() {
                if *pixel == magic_color.rgba {
                    pixel.0 = [0, 0, 0, 0];
                }
            }
        }
        
        let image_rc = Rc::new(image);
        cached_image.insert(Some(Rc::clone(&image_rc)));

        Ok(Some(image_rc))
    }

    fn load_tileset(&mut self, id: AssetId) -> Result<MaybeImage> {
        let suffix = format!("Tileset{id}.png");
        
        let image = self.load_image(self.paths.level_tilesets.join(&suffix), MagicColor::MAGENTA)?;
        if image.is_some() {
            return Ok(image);
        }
        
        let image = self.load_image(self.paths.data_tilesets.join(&suffix), MagicColor::MAGENTA)?;
        if image.is_some() {
            return Ok(image);
        }
        
        Ok(None)
    }

    fn load_gradient(&mut self, id: AssetId) -> Result<MaybeImage> {
        let suffix = format!("Gradient{id}.png");
        
        let image = self.load_image(self.paths.level_gradients.join(&suffix), MagicColor::MAGENTA)?;
        if image.is_some() {
            return Ok(image);
        }
        
        let image = self.load_image(self.paths.data_gradients.join(&suffix), MagicColor::MAGENTA)?;
        if image.is_some() {
            return Ok(image);
        }
        
        Ok(None)
    }

    fn load_stock_object(
        &mut self,
        id: &ObjectId,
        def: Option<&ObjectDef>,
    ) -> Result<MaybeImage> {
        if let Some(def) = def && def.is_overridden {
            return self.load_custom_object(def);
        }
        
        let ObjectId(tile, variant) = id;
        let suffix = match def.and_then(|def| def.path.as_ref()) {
            Some(path) => path,
            None => match variant {
                ObjectVariant::None => &format!("Bank{}/Object{}.png", tile.0, tile.1),
                _ => &format!("Bank{}/Object{}_{}.png", tile.0, tile.1, variant),
            },
        };
        
        let image = self.load_image(self.paths.templates.join(suffix), MagicColor::FORCE_MAGENTA)?;
        if image.is_some() {
            return Ok(image);
        }
        
        let image = self.load_image(self.paths.editor_objects.join(suffix), MagicColor::FORCE_MAGENTA)?;
        if image.is_some() {
            return Ok(image);
        }
        
        Ok(None)
    }

    fn load_custom_object(&mut self, def: &ObjectDef) -> Result<MaybeImage> {
        let Some(path) = def.path.as_ref() else { return Ok(None) };
        self.load_image(self.paths.custom_objects.join(path), MagicColor::BLACK)
    }

    fn load_override_object(&mut self, def: &ObjectDef) -> Result<MaybeImage> {
        let image = match def.oco_support {
            OcoSupport::NoCustomGraphics => {
                let ObjectKind::OverrideObject(original_tile) = def.kind else {
                    return Ok(None);
                };
                let original_id = ObjectId::from(original_tile);
                let Some(original_def) = self.object_defs.get(&original_id) else {
                    return Ok(None);
                };
                self.load_stock_object(&original_id, Some(original_def))?
            }
            _ => self.load_custom_object(def)?
        };
            
        if def.replace_colors.is_empty() {
            return Ok(image);
        }
        let Some(image) = image else {
            return Ok(image);
        };
        
        let mut transformed_image = (*image).clone();

        for Rgba(pixel) in transformed_image.pixels_mut() {
            for ColorReplacement { old, new, is_transparent } in &def.replace_colors {
                if pixel[..3] == old[..3] {
                    pixel[0] = new[0];
                    pixel[1] = new[1];
                    pixel[2] = new[2];
                    if *is_transparent {
                        pixel[3] = 0;
                    }
                    break;
                }
            }
        }
        
        Ok(Some(Rc::new(transformed_image)))
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct MagicColor {
    rgba: Rgba<u8>,
    force: bool,
}

impl MagicColor {
    const BLACK: MagicColor = MagicColor {
        rgba: Rgba([0, 0, 0, 255]),
        force: false,
    };
    const MAGENTA: MagicColor = MagicColor {
        rgba: Rgba([255, 0, 255, 255]),
        force: false,
    };
    const FORCE_MAGENTA: MagicColor = MagicColor {
        rgba: Rgba([255, 0, 255, 255]),
        force: true,
    };
}
