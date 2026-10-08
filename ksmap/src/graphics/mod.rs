pub mod spritesheet;
mod png_decoder;

use std::{
    collections::hash_map::Entry,
    fs::File,
    io::{self, BufReader},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Result;
use image::{DynamicImage, ExtendedColorType, ImageError, Pixel, Rgba, RgbaImage, error::UnsupportedErrorKind};
use libks::map_bin::AssetId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    definitions::{ObjectDef, ObjectDefs, ObjectKind, OcoSupport},
    id::{ObjectId, ObjectVariant},
    ini_util::unpack_color,
};
use spritesheet::Spritesheet;

pub struct Graphics {
    object_defs: Arc<ObjectDefs>,
    inner: GraphicsInner,
}

struct GraphicsInner {
    paths: Paths,
    cache: FxHashMap<(PathBuf, MagicColor), Option<ImageInfo>>,
    tilesets: FxHashMap<AssetId, Arc<RgbaImage>>,
    gradients: FxHashMap<AssetId, Gradient>,
    objects: FxHashMap<ObjectId, Spritesheet>,
    particles: FxHashMap<Particle, Spritesheet>,
}

struct Paths {
    data_tilesets: PathBuf,
    data_gradients: PathBuf,
    editor_objects: PathBuf,
    level_tilesets: PathBuf,
    level_gradients: PathBuf,
    custom_objects: PathBuf,
    templates: PathBuf,
}

struct ImageInfo {
    image: Arc<RgbaImage>,
    has_alpha_channel: bool,
}

impl Clone for ImageInfo {
    fn clone(&self) -> Self {
        Self {
            image: Arc::clone(&self.image),
            has_alpha_channel: self.has_alpha_channel,
        }
    }
}

pub struct Gradient {
    pub image: Arc<RgbaImage>,
    pub has_transparency: bool,
}

#[derive(PartialEq, Eq, Hash)]
struct Particle {
    id: ObjectId,
    color: i32,
}

struct ColorReplacement {
    pub old: [u8; 3],
    pub new: [u8; 3],
    pub is_transparent: bool,
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

#[derive(thiserror::Error, Debug)]
pub enum LoadImageError {
    #[error("An IO error occurred while reading `{}`: {source}", path.to_string_lossy())]
    Io {
        source: std::io::Error,
        path: PathBuf,
    },
}

#[derive(thiserror::Error, Debug)]
pub enum LoadImageWarning {
    #[error("`{}` could not be decoded and was ignored. Reason: {source}", path.to_string_lossy())]
    FailedToDecode {
        source: image::ImageError,
        path: PathBuf,
    },
    #[error("`{}` was ignored. Reason: KS does not support indexed PNGs with bit depth {bit_depth}.", path.to_string_lossy())]
    UnsupportedIndexedBitDepth {
        path: PathBuf,
        bit_depth: u8,
    },
}

type MaybeImageRc = Option<Arc<RgbaImage>>;

impl Graphics {
    pub fn new(
        data_dir: impl AsRef<Path>,
        level_dir: impl AsRef<Path>,
        templates_dir: impl Into<PathBuf>,
        object_defs: Arc<ObjectDefs>,
    ) -> Self {
        let paths = Paths::new(
            data_dir,
            level_dir,
            templates_dir.into(),
        );
        let inner = GraphicsInner {
            paths,
            cache: FxHashMap::default(),
            tilesets: FxHashMap::default(),
            gradients: FxHashMap::default(),
            objects: FxHashMap::default(),
            particles: FxHashMap::default(),
        };

        Self {
            object_defs,
            inner,
        }
    }
    
    pub fn tileset(&self, id: AssetId) -> Option<&RgbaImage> {
        self.inner.tilesets.get(&id)
            .map(Arc::as_ref)
    }

    pub fn gradient(&self, id: AssetId) -> Option<&Gradient> {
        self.inner.gradients.get(&id)
    }

    pub fn object(&self, id: &ObjectId) -> Option<&Spritesheet> {
        self.inner.objects.get(&id)
    }
    
    pub fn particle(&self, id: &ObjectId, color: i32) -> Option<&Spritesheet> {
        self.inner.particles.get(&Particle {
            id: *id,
            color
        })
    }
    
    pub fn load_tilesets(&mut self, ids: &[AssetId], warnings: &mut Vec<LoadImageWarning>) -> Result<()> {
        for id in ids {
            if let Some(image) = self.inner.load_tileset(*id, warnings)? {
                self.inner.tilesets.insert(*id, image);
            }
        }
        Ok(())
    }
    
    pub fn load_gradients(&mut self, ids: &[AssetId], warnings: &mut Vec<LoadImageWarning>) -> Result<()> {
        for id in ids {
            if let Some(gradient) = self.inner.load_gradient(*id, warnings)? {
                self.inner.gradients.insert(*id, gradient);
            }
        }
        Ok(())
    }
    
    pub fn load_objects(&mut self, ids: &[ObjectId], warnings: &mut Vec<LoadImageWarning>) -> Result<()> {
        let object_defs = self.object_defs.as_ref();
        let mut particles = FxHashSet::<Particle>::default();
        
        for id in ids {
            let Some(def) = object_defs.get(&id) else { continue };
            self.inner.load_object(id, def, &self.object_defs, warnings)?;
            
            // Particles may need to be loaded/recolored even if they aren't used directly
            if let Some(particle_id) = def.sync.fx_particle {
                particles.insert(Particle {
                    id: particle_id,
                    color: def.base.color_base
                });
            }
        }
        
        for particle in particles {
            let Some(def) = object_defs.get(&particle.id) else { continue };
            self.inner.load_object(&particle.id, def, &self.object_defs, warnings)?;
            
            let Some(base_spritesheet) = self.inner.objects.get(&particle.id) else { continue };
            let recolored_image = recolor_image(
                &base_spritesheet.image,
                def.base.color_base,
                &def.base.color_offsets,
                particle.color,
                !def.base.no_oco_black_transparency
            );
            let recolored_spritesheet = Spritesheet {
                image: recolored_image,
                ..base_spritesheet.clone()
            };
            
            self.inner.particles.insert(particle, recolored_spritesheet);
        }
        
        Ok(())
    }
}

impl GraphicsInner {
    fn load_image(
        &mut self,
        path: PathBuf,
        magic_color: MagicColor,
        warnings: &mut Vec<LoadImageWarning>,
    ) -> Result<Option<ImageInfo>, LoadImageError> {
        let cached_image = match self.cache.entry((path, magic_color)) {
            Entry::Occupied(entry) => {
                return match entry.get() {
                    Some(info) => Ok(Some(info.clone())),
                    None => Ok(None),
                };
            },
            Entry::Vacant(entry) => entry,
        };
        let (path, magic_color) = cached_image.key();
        
        let file = match File::open(path) {
            Ok(file) => file,
            Err(err) => match err.kind() {
                io::ErrorKind::NotFound => {
                    cached_image.insert(None);
                    return Ok(None);
                },
                _ => return Err(LoadImageError::Io {
                    source: err,
                    path: path.clone(),
                }),
            }
        };
        let reader = BufReader::new(file);
        let image = match png_decoder::PngDecoder::new(reader)
            .and_then(DynamicImage::from_decoder)
        {
            Ok(val) => val,
            Err(err) => {
                if let ImageError::Unsupported(err_unsupported) = &err
                    && let UnsupportedErrorKind::Color(color_type) = err_unsupported.kind()
                    && let ExtendedColorType::Unknown(bit_depth) = color_type
                {
                    warnings.push(LoadImageWarning::UnsupportedIndexedBitDepth {
                        path: path.clone(),
                        bit_depth,
                    });
                }
                else {
                    warnings.push(LoadImageWarning::FailedToDecode {
                        source: err,
                        path: path.clone(),
                    });
                }
                return Ok(None);
            }
        };

        let has_alpha = image.has_alpha();
        let mut image = image.into_rgba8();

        if !has_alpha || magic_color.force {
            for pixel in image.pixels_mut() {
                if *pixel == magic_color.rgba {
                    pixel.0 = [0, 0, 0, 0];
                }
            }
        }
        
        let info = ImageInfo {
            image: Arc::new(image),
            has_alpha_channel: has_alpha,
        };
        cached_image.insert(Some(info.clone()));

        Ok(Some(info))
    }

    fn load_tileset(
        &mut self,
        id: AssetId,
        warnings: &mut Vec<LoadImageWarning>,
    ) -> Result<MaybeImageRc, LoadImageError> {
        let suffix = format!("Tileset{id}.png");
        
        let level_path = self.paths.level_tilesets.join(&suffix);
        if let Some(info) = self.load_image(level_path, MagicColor::MAGENTA, warnings)? {
            let image = self.preprocess_tileset(info);
            return Ok(Some(image));
        }
        
        let data_path = self.paths.data_tilesets.join(&suffix);
        if let Some(info) = self.load_image(data_path, MagicColor::MAGENTA, warnings)? {
            let image = self.preprocess_tileset(info);
            return Ok(Some(image));
        }
        
        Ok(None)
    }

    fn preprocess_tileset(&mut self, info: ImageInfo) -> Arc<RgbaImage> {
        let image = info.image;

        // If a tileset is undersized such that it has partially incomplete tiles, and it has no alpha channel, the
        // remaining pixels of those tiles (beyond the borders of the image) will become black. If the image has an
        // alpha channel, they will become transparent instead. Tiles that would lie wholly outside the borders of
        // the image will be completely transparent regardless of whether the image has an alpha channel.
        if info.has_alpha_channel {
            return image;
        }
        
        let width_original = image.width().min(384);
        let width_remainder = width_original % 24;
        let mut width_padding = 0;
        if width_original < 384 && width_remainder > 0 {
            width_padding = 24 - width_remainder;
        }
        
        let height_original = image.height().min(192);
        let height_remainder = height_original % 24;
        let mut height_padding = 0;
        if height_original < 192 && height_remainder > 0 {
            height_padding = 24 - height_remainder;
        }
        
        if width_padding > 0 || height_padding > 0 {
            let width_new = width_original + width_padding;
            let height_new = height_original + height_padding;
            let processed = resize_image_canvas(image.as_ref(), width_new, height_new, Rgba([0, 0, 0, 255]));
            Arc::new(processed)
        }
        else {
            image
        }
    }

    fn load_gradient(
        &mut self,
        id: AssetId,
        warnings: &mut Vec<LoadImageWarning>,
    ) -> Result<Option<Gradient>, LoadImageError> {
        let suffix = format!("Gradient{id}.png");
        
        let level_path = self.paths.level_gradients.join(&suffix);
        if let Some(info) = self.load_image(level_path, MagicColor::MAGENTA, warnings)? {
            let image = self.preprocess_gradient(info);
            return Ok(Some(image));
        }
        
        let data_path = self.paths.data_gradients.join(&suffix);
        if let Some(info) = self.load_image(data_path, MagicColor::MAGENTA, warnings)? {
            let image = self.preprocess_gradient(info);
            return Ok(Some(image));
        }
        
        Ok(None)
    }

    fn preprocess_gradient(&mut self, info: ImageInfo) -> Gradient {
        let mut image = info.image;
        
        // If a gradient is less than 240 pixels tall, and the source image lacked an alpha channel, then the gradient
        // is padded with black pixels. Otherwise, it's padded with transparent pixels
        if image.height() < 240 {
            let alpha = if info.has_alpha_channel { 0 } else { 255 };
            let processed = resize_image_canvas(image.as_ref(), image.width(), 240, Rgba([0, 0, 0, alpha]));
            image = Arc::new(processed);
        }
        
        let has_transparency = image.pixels().any(|pixel| pixel.alpha() < 255);
        
        Gradient {
            image,
            has_transparency,
        }
    }
    
    fn load_object(
        &mut self,
        id: &ObjectId,
        def: &ObjectDef,
        object_defs: &ObjectDefs,
        warnings: &mut Vec<LoadImageWarning>
    ) -> Result<()> {
        if self.objects.contains_key(id) {
            return Ok(());
        }
        
        let image = match def.kind {
            ObjectKind::Object => self.load_stock_object(id, def, warnings)?,
            ObjectKind::CustomObject => self.load_custom_object(def, warnings)?,
            ObjectKind::OverrideObject(_) => self.load_override_object(id, def, object_defs, warnings)?,
        };
        
        let Some(image) = image else { return Ok(()) };
        let spritesheet = Spritesheet::new(image, &def.anim);
        self.objects.insert(*id, spritesheet);
        
        Ok(())
    }

    fn load_stock_object(
        &mut self,
        id: &ObjectId,
        def: &ObjectDef,
        warnings: &mut Vec<LoadImageWarning>,
    ) -> Result<MaybeImageRc, LoadImageError> {
        if def.base.is_overridden {
            return self.load_custom_object(def, warnings);
        }
        
        let ObjectId(tile, variant) = id;
        let suffix = match def.path.as_ref() {
            Some(path) => path,
            None => match variant {
                ObjectVariant::None => &format!("Bank{}/Object{}.png", tile.0, tile.1),
                _ => &format!("Bank{}/Object{}_{}.png", tile.0, tile.1, variant),
            },
        };
        
        let templates_path = self.paths.templates.join(suffix);
        if let Some(info) = self.load_image(templates_path, MagicColor::FORCE_MAGENTA, warnings)? {
            return Ok(Some(info.image));
        }
        
        let data_path = self.paths.editor_objects.join(suffix);
        if let Some(info) = self.load_image(data_path, MagicColor::FORCE_MAGENTA, warnings)? {
            return Ok(Some(info.image));
        }
        
        Ok(None)
    }

    fn load_custom_object(
        &mut self,
        def: &ObjectDef,
        warnings: &mut Vec<LoadImageWarning>,
    ) -> Result<MaybeImageRc, LoadImageError> {
        let Some(path) = def.path.as_ref() else { return Ok(None) };
        
        // PathBuf::join sees /xyz as an absolute path and replaces the base path
        // KS joins paths by simple string concatenation, so leading slashes are meaningless
        let path = match path.char_indices()
            .skip_while(|(_, ch)| *ch == '/' || *ch == '\\')
            .next()
        {
            Some((i, _)) => &path[i..],
            None => return Ok(None),
        };
        
        self.load_image(self.paths.custom_objects.join(path), MagicColor::BLACK, warnings)
            .map(|maybe_info| maybe_info.map(|info| info.image))
    }

    fn load_override_object(
        &mut self,
        id: &ObjectId,
        def: &ObjectDef,
        object_defs: &ObjectDefs,
        warnings: &mut Vec<LoadImageWarning>,
    ) -> Result<MaybeImageRc, LoadImageError> {
        match def.base.oco_support {
            OcoSupport::Full | OcoSupport::None => {
                self.load_custom_object(def, warnings)
            }
            OcoSupport::NoCustomGraphics => {
                let ObjectKind::OverrideObject(original_id) = def.kind else {
                    return Ok(None);
                };
                let Some(original_def) = object_defs.get(&original_id) else {
                    return Ok(None);
                };
                self.load_stock_object(&original_id, original_def, warnings)
            }
            OcoSupport::Invisible => {
                self.load_stock_object(&id, def, warnings)
            }
        }
    }
}

fn resize_image_canvas(image: &RgbaImage, new_width: u32, new_height: u32, fill: Rgba<u8>) -> RgbaImage {
    let mut new_image = RgbaImage::from_pixel(new_width, new_height, fill);
    
    let common_width = u32::min(image.width(), new_width);
    let common_height = u32::min(image.height(), new_height);
    
    for y in 0..common_height {
        for x in 0..common_width {
            new_image.put_pixel(x, y, *image.get_pixel(x, y));
        }
    }
    
    new_image
}

fn recolor_image(
    image: &Arc<RgbaImage>,
    color_base: i32,
    color_offsets: &[i32],
    color_new: i32,
    black_is_transparent: bool
) -> Arc<RgbaImage> {
    if color_new == color_base {
        return Arc::clone(image);
    }
    
    let mut replace_colors = Vec::with_capacity(color_offsets.len() + 1);
    for offset in [0].iter().chain(color_offsets.iter()) {
        let old = unpack_color(color_base + offset);
        let new = unpack_color(color_new + offset);
        let is_transparent = black_is_transparent && new == [0, 0, 0];
        replace_colors.push(ColorReplacement {
            old,
            new,
            is_transparent,
        });
    }
    
    let mut recolored_image = RgbaImage::clone(image);
    for Rgba(pixel) in recolored_image.pixels_mut() {
        for ColorReplacement { old, new, is_transparent } in &replace_colors {
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
    
    Arc::new(recolored_image)
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
