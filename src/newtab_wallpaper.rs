//! Local New Tab images and a small, entirely procedural animated wallpaper set.

use std::io::{Cursor, Read as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::epaint::RectShape;
use egui::{Color32, Context, CornerRadius, Id, Painter, Rect, TextureHandle, Ui, pos2, vec2};
use image::{DynamicImage, ImageDecoder as _, ImageReader, RgbaImage};

use crate::settings::NewTabBackground;
use crate::theme::{self, Palette};
use crate::{WakerEvent, l10n::text as tr};

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_TEXTURE_SIZE: u32 = 3072;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Error {
    Read,
    Format,
    Save,
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    Unavailable,
}

impl Error {
    pub fn label(self) -> &'static str {
        tr(match self {
            Self::Read => "newtab-wallpaper-read-error",
            Self::Format => "newtab-wallpaper-format-error",
            Self::Save => "newtab-wallpaper-save-error",
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            Self::Unavailable => "newtab-playground-unavailable",
        })
    }
}

pub(crate) enum Input {
    Restore(PathBuf),
    File(PathBuf),
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    Generated(Vec<u8>),
}

pub(crate) fn saved_path() -> Option<PathBuf> {
    Some(crate::settings::data_dir()?.join("new-tab-wallpaper.jpg"))
}

#[derive(Default)]
pub(crate) struct State {
    pub texture: Option<TextureHandle>,
    pub busy: bool,
    pub error: Option<Error>,
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    pub can_generate: bool,
}

impl State {
    pub fn upload(&mut self, ctx: &Context, image: RgbaImage) {
        self.texture = Some(ctx.load_texture(
            "New Tab wallpaper",
            egui::ColorImage::from_rgba_unmultiplied(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        ));
    }

    pub fn load(&mut self, input: Input, proxy: winit::event_loop::EventLoopProxy<WakerEvent>) {
        self.error = None;
        self.busy = true;
        if std::thread::Builder::new()
            .name("new-tab-wallpaper".into())
            .spawn(move || {
                let select = !matches!(input, Input::Restore(_));
                let result = decode(input);
                let _ = proxy.send_event(WakerEvent::WallpaperLoaded { result, select });
            })
            .is_err()
        {
            self.busy = false;
            self.error = Some(Error::Read);
        }
    }
}

fn read(path: &Path) -> Result<Vec<u8>, Error> {
    let file = std::fs::File::open(path).map_err(|_| Error::Read)?;
    if !file.metadata().map_err(|_| Error::Read)?.is_file() {
        return Err(Error::Read);
    }
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Read)?;
    Ok(bytes)
}

fn decode(input: Input) -> Result<RgbaImage, Error> {
    let save = !matches!(input, Input::Restore(_));
    let bytes = match input {
        Input::Restore(path) | Input::File(path) => read(&path)?,
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        Input::Generated(bytes) => bytes,
    };
    if bytes.len() > MAX_BYTES {
        return Err(Error::Format);
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| Error::Format)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|_| Error::Format)?;
    let orientation = decoder.orientation().map_err(|_| Error::Format)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| Error::Format)?;
    image.apply_orientation(orientation);
    if image.width().max(image.height()) > MAX_TEXTURE_SIZE {
        image = image.thumbnail(MAX_TEXTURE_SIZE, MAX_TEXTURE_SIZE);
    }
    if save {
        let mut encoded = Vec::new();
        let rgb = image.to_rgb8();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 88)
            .encode(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|_| Error::Save)?;
        let path = saved_path().ok_or(Error::Save)?;
        crate::store::write_private(&path, &encoded).map_err(|_| Error::Save)?;
    }
    Ok(image.into_rgba8())
}

/// Tick only while a wallpaper is visible and the window is active. Keep the
/// current phase while paused so the image does not jump when motion resumes.
pub(crate) fn phase(ui: &Ui, animate: bool) -> f32 {
    let (time, focused) = ui.input(|input| (input.time, input.focused));
    let id = Id::unique("new_tab_wallpaper_phase");
    if animate && focused {
        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
    ui.ctx().data_mut(|data| {
        let clock = data.get_temp_mut_or_insert_with(id, || (time, 0.0_f32));
        if animate && focused {
            clock.1 += (time - clock.0).clamp(0.0, 0.05) as f32 * 0.17;
        }
        clock.0 = time;
        clock.1
    })
}

pub(crate) fn paint(
    painter: &Painter,
    rect: Rect,
    corners: CornerRadius,
    palette: &Palette,
    background: NewTabBackground,
    photo: Option<&TextureHandle>,
    animation: Option<f32>,
) {
    if !rect.is_positive() {
        return;
    }
    match background {
        NewTabBackground::CustomImage => {
            if let Some(texture) = photo {
                let [width, height] = texture.resolution();
                let scale = (rect.width() / width as f32).max(rect.height() / height as f32);
                let crop_x = rect.width() / (width as f32 * scale);
                let crop_y = rect.height() / (height as f32 * scale);
                let uv = Rect::from_center_size(pos2(0.5, 0.5), vec2(crop_x, crop_y));
                painter.add(
                    RectShape::filled(rect, corners, Color32::WHITE).with_texture(texture.id(), uv),
                );
                let veil = if palette.dark { 0.55 } else { 0.48 };
                painter.rect_filled(rect, corners, theme::page_veil(palette, veil));
            }
        },
        background if background.animated() => {
            // Let the same antialiased rounded path that draws the frame
            // clip the wallpaper. Moving the corner of a coarse gradient
            // mesh leaves a diagonal notch when the viewport grows.
            let texture = gradient_texture(painter.ctx(), background, palette.dark, animation);
            painter.add(
                RectShape::filled(rect, corners, Color32::WHITE).with_texture(
                    texture.id(),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                ),
            );
        },
        _ => {},
    }
}

fn gradient_texture(
    ctx: &Context,
    background: NewTabBackground,
    dark: bool,
    animation: Option<f32>,
) -> TextureHandle {
    // The static swatches keep their own texture and never re-upload as the
    // full-size wallpaper animates. Both use a small, linearly filtered map.
    let id = Id::new((
        "wallpaper-gradient",
        background as u8,
        dark,
        animation.is_some(),
    ));
    let phase = animation.unwrap_or(0.5);
    let cached = ctx.data(|data| data.get_temp::<(f32, TextureHandle)>(id));
    if let Some((last_phase, texture)) = &cached
        && *last_phase == phase
    {
        return texture.clone();
    }
    const SIDE: usize = 32;
    let pixels = (0..SIDE * SIDE)
        .map(|index| {
            let unit = |value: usize| (value as f32 + 0.5) / SIDE as f32;
            shade(
                background,
                dark,
                unit(index % SIDE),
                unit(index / SIDE),
                phase,
            )
        })
        .collect();
    let image = egui::ColorImage::new([SIDE, SIDE], pixels);
    let texture = if let Some((_, mut texture)) = cached {
        texture.set(image, egui::TextureOptions::LINEAR);
        texture
    } else {
        ctx.load_texture("New Tab gradient", image, egui::TextureOptions::LINEAR)
    };
    ctx.data_mut(|data| data.insert_temp(id, (phase, texture.clone())));
    texture
}

fn shade(background: NewTabBackground, dark: bool, x: f32, y: f32, t: f32) -> Color32 {
    let rgb = Color32::from_rgb;
    let (base, light, peak, curve) = match background {
        NewTabBackground::Aurora => (
            rgb(13, 25, 54),
            rgb(50, 159, 142),
            rgb(133, 99, 209),
            y - 0.45 - (x * 5.0 + t).sin() * 0.20,
        ),
        NewTabBackground::Tidelight => (
            rgb(11, 35, 68),
            rgb(34, 136, 206),
            rgb(100, 205, 225),
            y + (x * 6.3 - t).sin() * 0.14 - 0.64,
        ),
        _ => (
            rgb(50, 20, 58),
            rgb(191, 76, 110),
            rgb(253, 160, 115),
            ((x - 0.71).powi(2) * 0.8 + (y + 0.48).powi(2)).sqrt() - 0.98 - t.sin() * 0.08,
        ),
    };
    let haze = (-curve.powi(2) * 17.0).exp();
    let ribbon = (-(curve - 0.09).powi(2) * 72.0).exp();
    let colour = theme::mix(base, light, haze * (0.45 + 0.40 * x));
    let colour = theme::mix(colour, peak, ribbon * (0.36 + 0.28 * (x * 4.0 - t).cos()));
    if dark {
        theme::mix(colour, base, 0.16)
    } else {
        theme::mix(colour, rgb(241, 240, 255), 0.54)
    }
}
