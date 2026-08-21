//! A blurred copy of whatever the engine last drew, for the chrome to frost
//! itself against.
//!
//! The new tab page's cards look like glass because there is a blurred picture
//! behind them to sample. Everything else that floats over a web page — the
//! favourites card, the downloads card, a menu — had nothing: the page is
//! opaque pixels the engine has already drawn, and no amount of translucency
//! turns opaque pixels into a blur. So those surfaces were a grey wash with a
//! sharp page showing through, which is the one thing frosted glass never
//! looks like.
//!
//! This takes the page as drawn and makes a blur of it, so the same machinery
//! that frosts a card against a wallpaper frosts a menu against a web page.
//!
//! It is cheap because it is small. The downsampling is a `glBlitFramebuffer`,
//! which the GPU does on its way past; what is left is one small `glReadPixels`,
//! throttled, because a readback stalls the pipeline and the page behind a
//! hover card does not change while you are reading it.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use glow::HasContext as _;

/// A handle to the one copy the window keeps, shared with the paint callback
/// that takes it.
pub type Capture = Arc<Mutex<PageBackdrop>>;

/// Take the copy at this point in `painter`'s layer.
///
/// Where this is called is the whole design. Shapes within a layer are drawn in
/// the order they were added, so the copy holds exactly what the page has drawn
/// so far and none of what comes after — which is how a surface avoids frosting
/// against itself. Every page puts it in the same place: straight after
/// whatever filled the content rect, and before anything that sits on it.
pub fn capture_into(painter: &egui::Painter, rect: egui::Rect, capture: &Capture) {
    let capture = capture.clone();
    painter.add(egui::PaintCallback {
        rect,
        callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
            let clip = info.viewport_in_pixels();
            let Ok(mut backdrop) = capture.lock() else {
                return;
            };
            backdrop.capture(
                painter.gl(),
                [
                    clip.left_px,
                    clip.from_bottom_px,
                    clip.width_px,
                    clip.height_px,
                ],
            );
        })),
    });
}

/// The longest side of the copy.
///
/// This started at 220, on the reasoning that the blur removes anything finer
/// anyway. It does — that is the problem. Shrinking a window to a fifth of its
/// width is itself a heavy blur, and blurring *that* left a flat wash with no
/// structure in it at all, which does not read as frosted glass. It reads as a
/// grey rectangle, which is the exact complaint the file was written to fix.
///
/// What makes a surface look like glass is seeing the shapes behind it soften,
/// not seeing them go. So this matches the wallpaper's frost, which has always
/// looked right, at the same fraction of it.
const SIDE: i32 = crate::wallpaper::FROST_SIDE as i32 * 3 / 4;
/// How far it is blurred, in pixels of the copy.
///
/// Taken from the material at the same blur-to-size ratio the wallpaper uses,
/// so a card over a page and a card over a photograph are the same glass. Left
/// to itself the number means nothing: blur is only ever relative to the size
/// of the thing being blurred.
const BLUR: f32 =
    crate::theme::Material::GLASS.blur * (SIDE as f32) / (crate::wallpaper::FROST_SIDE as f32);
/// A readback stalls the pipeline, so it is worth doing only about as often as
/// what it is copying actually changes.
const EVERY: Duration = Duration::from_millis(120);

#[derive(Default)]
pub struct PageBackdrop {
    framebuffer: Option<glow::Framebuffer>,
    texture: Option<glow::Texture>,
    size: (i32, i32),
    /// The most recent copy, waiting for the main thread to upload it.
    ready: Option<egui::ColorImage>,
    taken_at: Option<Instant>,
}

impl PageBackdrop {
    /// Whether enough time has passed to be worth another copy.
    pub fn due(&self) -> bool {
        self.taken_at.is_none_or(|taken| taken.elapsed() >= EVERY)
    }

    /// Take the newest copy, for the caller to make a texture of.
    pub fn take(&mut self) -> Option<egui::ColorImage> {
        self.ready.take()
    }

    /// Copy `source` — a rectangle of the framebuffer currently bound for
    /// drawing, in physical pixels with a bottom-left origin — into a small
    /// blurred image.
    ///
    /// Must be called with a current GL context, from inside a paint callback
    /// ordered after whatever drew the thing being copied and before anything
    /// that would draw over it.
    pub fn capture(&mut self, gl: &glow::Context, source: [i32; 4]) {
        let [x, y, width, height] = source;
        // One copy per frame, whoever asks first. The pages that ask are meant
        // to be mutually exclusive, but a readback is too expensive to leave
        // that to trust.
        if width <= 0 || height <= 0 || self.ready.is_some() {
            return;
        }
        // Keep the page's shape, so the blur is not stretched.
        let scale = f32::from(SIDE as i16) / width.max(height) as f32;
        let small = (
            ((width as f32 * scale) as i32).clamp(1, SIDE),
            ((height as f32 * scale) as i32).clamp(1, SIDE),
        );

        // SAFETY: a current context, and every binding this changes is put
        // back before returning.
        unsafe {
            if self.size != small {
                self.release(gl);
                let Some((framebuffer, texture)) = Self::allocate(gl, small) else {
                    return;
                };
                self.framebuffer = Some(framebuffer);
                self.texture = Some(texture);
                self.size = small;
            }
            let (Some(framebuffer), Some(_)) = (self.framebuffer, self.texture) else {
                return;
            };

            let previous_draw = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
            let previous_read = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
            let restore = |binding: u32, id: i32| {
                gl.bind_framebuffer(
                    binding,
                    std::num::NonZeroU32::new(id as u32).map(glow::NativeFramebuffer),
                );
            };

            // Down to size on the way across — the GPU filters as it blits, so
            // most of the blur is already done by the time it lands.
            gl.bind_framebuffer(
                glow::READ_FRAMEBUFFER,
                std::num::NonZeroU32::new(previous_draw as u32).map(glow::NativeFramebuffer),
            );
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(framebuffer));
            // Off for the blit, which must not be clipped to whatever egui was
            // last drawing — and back on afterwards, because egui enables it
            // once for the whole frame and every draw call after this one goes
            // unclipped without it. Leaving it off painted the new tab page
            // outside its own bounds for the length of a frame, eight times a
            // second, which is what the blinking was.
            let scissoring = gl.is_enabled(glow::SCISSOR_TEST);
            gl.disable(glow::SCISSOR_TEST);
            gl.blit_framebuffer(
                x,
                y,
                x + width,
                y + height,
                0,
                0,
                small.0,
                small.1,
                glow::COLOR_BUFFER_BIT,
                glow::LINEAR,
            );

            let mut pixels = vec![0_u8; (small.0 * small.1 * 4) as usize];
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(framebuffer));
            gl.read_pixels(
                0,
                0,
                small.0,
                small.1,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );

            restore(glow::READ_FRAMEBUFFER, previous_read);
            restore(glow::DRAW_FRAMEBUFFER, previous_draw);
            if scissoring {
                gl.enable(glow::SCISSOR_TEST);
            }

            self.ready = blur(pixels, small);
            self.taken_at = Some(Instant::now());
        }
    }

    /// SAFETY: current context.
    unsafe fn allocate(
        gl: &glow::Context,
        size: (i32, i32),
    ) -> Option<(glow::Framebuffer, glow::Texture)> {
        unsafe {
            let texture = gl.create_texture().ok()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                size.0,
                size.1,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.bind_texture(glow::TEXTURE_2D, None);

            let framebuffer = gl.create_framebuffer().ok()?;
            let previous = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(framebuffer));
            gl.framebuffer_texture_2d(
                glow::DRAW_FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            let complete =
                gl.check_framebuffer_status(glow::DRAW_FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            gl.bind_framebuffer(
                glow::DRAW_FRAMEBUFFER,
                std::num::NonZeroU32::new(previous as u32).map(glow::NativeFramebuffer),
            );
            if !complete {
                gl.delete_framebuffer(framebuffer);
                gl.delete_texture(texture);
                return None;
            }
            Some((framebuffer, texture))
        }
    }

    /// SAFETY: current context.
    unsafe fn release(&mut self, gl: &glow::Context) {
        unsafe {
            if let Some(framebuffer) = self.framebuffer.take() {
                gl.delete_framebuffer(framebuffer);
            }
            if let Some(texture) = self.texture.take() {
                gl.delete_texture(texture);
            }
        }
        self.size = (0, 0);
    }
}

/// Blur the copy and turn it the right way up.
///
/// GL hands back rows from the bottom, and every other picture in the chrome
/// runs the other way — a backdrop sampled upside down is worse than no
/// backdrop, because it looks almost right.
fn blur(pixels: Vec<u8>, size: (i32, i32)) -> Option<egui::ColorImage> {
    let (width, height) = (size.0 as u32, size.1 as u32);
    let mut image = image::RgbaImage::from_raw(width, height, pixels)?;
    image::imageops::flip_vertical_in_place(&mut image);
    let blurred = image::imageops::fast_blur(&image, BLUR);
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        &blurred,
    ))
}

/// Cut the content card's rounded corners out of the framebuffer.
///
/// The page is blitted as a square, so the card's corners have to be rounded by
/// something drawn over it. That something has to be opaque — it is hiding
/// opaque pixels — while the chrome beside it is a thin tint over the system's
/// backdrop, and that backdrop is composited by the window server, outside this
/// framebuffer. It cannot be read, reproduced or matched, so the mask always
/// landed a few per cent off its surroundings and the corners always showed it.
/// Three attempts to pick a better colour each moved the seam rather than
/// removing it.
///
/// So nothing is painted over the page at all. The corners are cleared to
/// nothing — the page and the chrome under it both — and the chrome is drawn
/// back over them at its own tint. Over transparency that composites to exactly
/// what the chrome beside it is, because it is the same paint on the same
/// backdrop.
///
/// The top two corners are left alone: what shows through a hole cut up there
/// is not the backdrop but whatever is behind the window, and the chrome above
/// the card is opaque toolbar furniture that no colour chosen per height could
/// match anyway. Those keep the opaque mask they always had.
///
/// A row at a time, because a scissor is a rectangle and a corner is not.
/// Twenty-odd rows a corner, cleared to nothing, is not work anybody will
/// measure.
///
/// `rect` is the content card in physical pixels with a bottom-left origin, as
/// a viewport is. `radius` is its corner radius in the same units.
pub fn cut_corners(gl: &glow::Context, rect: [i32; 4], radius: i32) {
    let [x, y, width, height] = rect;
    if radius <= 0 || width <= radius * 2 || height <= radius * 2 {
        return;
    }
    // SAFETY: a current context, and every piece of state this touches is put
    // back before returning.
    unsafe {
        let scissoring = gl.is_enabled(glow::SCISSOR_TEST);
        let mut box_before = [0_i32; 4];
        gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut box_before);
        let mut clear_before = [0_f32; 4];
        gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut clear_before);
        gl.enable(glow::SCISSOR_TEST);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);

        for row in 0..radius {
            // How far in the arc has come by this row, measured from the flat
            // edge. Rounded outward, so what is cleared is everything the arc
            // does not cover and the mask's own antialiasing can soften the
            // boundary from there.
            let from_edge = radius - row;
            let reach = radius
                - ((radius * radius - from_edge * from_edge) as f64)
                    .sqrt()
                    .floor() as i32;
            if reach <= 0 {
                continue;
            }
            // The bottom two corners only. Cleared pixels show whatever the
            // window server has behind the window, and along the top of the
            // window that is not the system's backdrop view — a hole cut there
            // comes out black rather than blurred, and only the mask over it
            // keeps that off the screen. Below, it is the backdrop, which is
            // the whole point.
            for (corner_x, corner_y) in [(x, y + row), (x + width - reach, y + row)] {
                gl.scissor(corner_x, corner_y, reach, 1);
                gl.clear(glow::COLOR_BUFFER_BIT);
            }
        }

        gl.clear_color(
            clear_before[0],
            clear_before[1],
            clear_before[2],
            clear_before[3],
        );
        gl.scissor(box_before[0], box_before[1], box_before[2], box_before[3]);
        if !scissoring {
            gl.disable(glow::SCISSOR_TEST);
        }
    }
}

/// Add the cut to `painter`, at this point in its layer.
pub fn cut_corners_into(painter: &egui::Painter, rect: egui::Rect, radius: f32) {
    painter.add(egui::PaintCallback {
        rect,
        callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
            let clip = info.viewport_in_pixels();
            let scale = info.pixels_per_point;
            cut_corners(
                painter.gl(),
                [
                    clip.left_px,
                    clip.from_bottom_px,
                    clip.width_px,
                    clip.height_px,
                ],
                (radius * scale).round() as i32,
            );
        })),
    });
}
