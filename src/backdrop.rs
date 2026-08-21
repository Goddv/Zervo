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
//! It is cheap because it is tiny. The copy is a couple of hundred pixels
//! across — it is about to be blurred past anything finer — and the
//! downsampling is a `glBlitFramebuffer`, which the GPU does on its way past.
//! What is left is one small `glReadPixels`, throttled, because a readback
//! stalls the pipeline and the page behind a hover card does not change while
//! you are reading it.

use std::time::{Duration, Instant};

use glow::HasContext as _;

/// The longest side of the copy. Small on purpose: the blur removes anything
/// this resolution would have carried, and the texture is magnified back up
/// where the sampler's own filtering smooths it further.
const SIDE: i32 = 220;
/// How far it is blurred, in pixels of the copy.
const BLUR: f32 = 5.0;
/// A readback stalls the pipeline, so it is worth doing only about as often as
/// what it is copying actually changes.
const EVERY: Duration = Duration::from_millis(120);

pub struct PageBackdrop {
    framebuffer: Option<glow::Framebuffer>,
    texture: Option<glow::Texture>,
    size: (i32, i32),
    /// The most recent copy, waiting for the main thread to upload it.
    ready: Option<egui::ColorImage>,
    taken_at: Option<Instant>,
}

impl Default for PageBackdrop {
    fn default() -> Self {
        Self {
            framebuffer: None,
            texture: None,
            size: (0, 0),
            ready: None,
            taken_at: None,
        }
    }
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

    /// Forget everything. Called when there is no page to copy — an internal
    /// page, or a tab with no engine behind it — so the chrome does not go on
    /// frosting itself against a page that is no longer there.
    pub fn clear(&mut self) {
        self.ready = None;
        self.taken_at = None;
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
        if width <= 0 || height <= 0 {
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
