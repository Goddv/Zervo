//! Small origin-keyed thumbnails shared by History and Favourites.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use image::{ImageEncoder as _, ImageReader, RgbaImage};
use sha2::{Digest as _, Sha256};

const SIZE: u32 = 48;
const MAX_DISK_ENTRIES: usize = 512;
const MAX_MEMORY_ENTRIES: usize = 192;

#[derive(Default)]
pub(crate) struct Store {
    textures: RefCell<HashMap<String, Option<TextureHandle>>>,
}

impl Store {
    pub fn get(&self, ctx: &Context, address: &str) -> Option<TextureHandle> {
        let origin = origin(address)?;
        if let Some(texture) = self.textures.borrow().get(&origin) {
            return texture.clone();
        }
        let texture = load(&origin).map(|image| upload(ctx, &image));
        self.cache(origin, texture.clone());
        texture
    }

    pub fn remember(&self, address: &str, image: &RgbaImage, texture: TextureHandle) {
        let Some(origin) = origin(address) else {
            return;
        };
        if let Some(path) = icon_path(&origin) {
            let mut encoded = Vec::new();
            if image::codecs::png::PngEncoder::new(&mut encoded)
                .write_image(
                    image.as_raw(),
                    image.width(),
                    image.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .is_ok()
                && std::fs::read(&path).is_ok_and(|current| current == encoded)
            {
                self.cache(origin, Some(texture));
                return;
            }
            let new_entry = !path.exists();
            if !encoded.is_empty()
                && let Err(error) = crate::store::write_private(&path, &encoded)
            {
                log::warn!("Could not save a favicon: {error}");
            } else if new_entry && let Some(dir) = path.parent() {
                trim(dir);
            }
        }
        self.cache(origin, Some(texture));
    }

    pub fn clear(&mut self) {
        self.textures.get_mut().clear();
        if let Some(path) = directory()
            && let Err(error) = std::fs::remove_dir_all(path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            log::warn!("Could not clear saved favicons: {error}");
        }
    }

    fn cache(&self, origin: String, texture: Option<TextureHandle>) {
        let mut textures = self.textures.borrow_mut();
        if textures.len() >= MAX_MEMORY_ENTRIES && !textures.contains_key(&origin) {
            textures.clear();
        }
        textures.insert(origin, texture);
    }
}

fn origin(address: &str) -> Option<String> {
    let url = url::Url::parse(address).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.origin().ascii_serialization())
}

fn directory() -> Option<PathBuf> {
    Some(crate::settings::data_dir()?.join("favicons"))
}

fn icon_path(origin: &str) -> Option<PathBuf> {
    let hash = Sha256::digest(origin.as_bytes());
    Some(directory()?.join(format!("{hash:x}.png")))
}

fn load(origin: &str) -> Option<RgbaImage> {
    let path = icon_path(origin)?;
    if path.metadata().ok()?.len() > 16 * 1024 {
        return None;
    }
    let mut reader = ImageReader::with_format(
        Cursor::new(std::fs::read(path).ok()?),
        image::ImageFormat::Png,
    );
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(SIZE);
    limits.max_image_height = Some(SIZE);
    limits.max_alloc = Some(64 * 1024);
    reader.limits(limits);
    Some(reader.decode().ok()?.into_rgba8())
}

fn trim(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()? != "png" {
                return None;
            }
            Some((entry.metadata().ok()?.modified().ok()?, path))
        })
        .collect();
    let excess = files.len().saturating_sub(MAX_DISK_ENTRIES);
    if excess > 0 {
        files.sort_unstable_by_key(|(modified, _)| *modified);
        for (_, path) in files.into_iter().take(excess) {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn thumbnail(width: u32, height: u32, rgba: Vec<u8>) -> Option<RgbaImage> {
    let image = RgbaImage::from_raw(width, height, rgba)?;
    if width.max(height) <= SIZE {
        return Some(image);
    }
    Some(
        image::DynamicImage::ImageRgba8(image)
            .thumbnail(SIZE, SIZE)
            .into_rgba8(),
    )
}

pub fn upload(ctx: &Context, image: &RgbaImage) -> TextureHandle {
    ctx.load_texture(
        "Website icon",
        ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        ),
        TextureOptions::LINEAR,
    )
}
