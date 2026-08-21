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

/// An erase pass: geometry drawn to take the destination's alpha down rather
/// than to put colour on it.
///
/// `glClear` behind a scissor can do the same job and cannot antialias it — a
/// scissor is a whole number of pixels, so the corner came out as a staircase
/// while the two drawn by egui above it were smooth. Coverage has to come from
/// somewhere, and the cheapest place is a triangle's own interpolation: the
/// band either side of the arc is drawn with alpha running nought to one
/// across it, and the blend function turns that into how much of the page
/// survives.
#[derive(Default)]
pub struct Eraser {
    program: Option<glow::Program>,
    array: Option<glow::VertexArray>,
    buffer: Option<glow::Buffer>,
}

/// How wide the erase fades, in physical pixels. One is what antialiasing is.
const FEATHER_PX: f32 = 1.0;

impl Eraser {
    /// Cut the content card's rounded corners out of the framebuffer.
    ///
    /// The page is blitted as a square, so the corners have to be rounded by
    /// something. Painting over it cannot work: the cover has to be opaque —
    /// it is hiding opaque pixels — while the chrome beside it is a thin tint
    /// over the system's backdrop, composited by the window server outside
    /// this framebuffer and so impossible to read or reproduce. Taking the
    /// corners away instead leaves the chrome to be drawn back over nothing,
    /// which is the same paint on the same backdrop as the chrome beside it.
    ///
    /// Only the bottom two. What shows through a hole along the top of the
    /// window is not the backdrop but whatever is behind the window, and the
    /// chrome above the card is opaque toolbar furniture besides.
    ///
    /// `rect` is the card in physical pixels with a bottom-left origin, as a
    /// viewport is. `radius` is its corner radius in the same units.
    pub fn cut_corners(&mut self, gl: &glow::Context, rect: [i32; 4], radius: f32) {
        let [x, y, width, height] = rect;
        let radius_px = radius.min(width as f32 / 2.0).min(height as f32 / 2.0);
        if radius_px <= 0.5 || width <= 0 || height <= 0 {
            return;
        }
        let Some((program, array, buffer)) = self.ready(gl) else {
            return;
        };

        // Two corners, and for each a fan of wedges from the square corner out
        // to the arc, plus the feathered band across it.
        let steps = (radius_px.ceil() as usize).clamp(8, 64);
        let mut vertices: Vec<f32> = Vec::with_capacity(steps * 18);
        let to_ndc = |px: f32, py: f32| {
            (
                (px - x as f32) / width as f32 * 2.0 - 1.0,
                (py - y as f32) / height as f32 * 2.0 - 1.0,
            )
        };
        // (corner, arc centre, the quadrant's start angle)
        let corners = [
            (
                (x as f32, y as f32),
                (x as f32 + radius_px, y as f32 + radius_px),
                std::f32::consts::PI,
            ),
            (
                ((x + width) as f32, y as f32),
                ((x + width) as f32 - radius_px, y as f32 + radius_px),
                1.5 * std::f32::consts::PI,
            ),
        ];
        for (corner, centre, start) in corners {
            let at = |angle: f32, r: f32| (centre.0 + r * angle.cos(), centre.1 + r * angle.sin());
            let inner = radius_px - FEATHER_PX * 0.5;
            let outer = radius_px + FEATHER_PX * 0.5;
            for step in 0..steps {
                let a0 = start + (step as f32 / steps as f32) * 0.5 * std::f32::consts::PI;
                let a1 = start + ((step + 1) as f32 / steps as f32) * 0.5 * std::f32::consts::PI;
                let (i0, i1) = (at(a0, inner), at(a1, inner));
                let (o0, o1) = (at(a0, outer), at(a1, outer));
                // The band across the arc: nothing erased on the page's side,
                // everything on the chrome's, and a pixel of ramp between.
                for (p, alpha) in [
                    (i0, 0.0),
                    (i1, 0.0),
                    (o1, 1.0),
                    (i0, 0.0),
                    (o1, 1.0),
                    (o0, 1.0),
                ] {
                    let (nx, ny) = to_ndc(p.0, p.1);
                    vertices.extend_from_slice(&[nx, ny, alpha]);
                }
                // And the wedge from there out to the square corner, which is
                // page all the way and goes entirely.
                for p in [o0, o1, corner] {
                    let (nx, ny) = to_ndc(p.0, p.1);
                    vertices.extend_from_slice(&[nx, ny, 1.0]);
                }
            }
        }

        // SAFETY: a current context, and every piece of state this changes is
        // put back before returning.
        unsafe {
            let blending = gl.is_enabled(glow::BLEND);
            let scissoring = gl.is_enabled(glow::SCISSOR_TEST);
            let previous_program = gl.get_parameter_i32(glow::CURRENT_PROGRAM);
            gl.disable(glow::SCISSOR_TEST);
            gl.enable(glow::BLEND);
            // Destination-out: keep none of the source's colour, and keep the
            // destination in proportion to what the source did not cover.
            gl.blend_func_separate(
                glow::ZERO,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ZERO,
                glow::ONE_MINUS_SRC_ALPHA,
            );

            gl.use_program(Some(program));
            gl.bind_vertex_array(Some(array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck_cast(&vertices),
                glow::STREAM_DRAW,
            );
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 12, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 1, glow::FLOAT, false, 12, 8);
            gl.draw_arrays(glow::TRIANGLES, 0, (vertices.len() / 3) as i32);

            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.use_program(
                std::num::NonZeroU32::new(previous_program as u32).map(glow::NativeProgram),
            );
            // Back to what egui paints with, which is premultiplied source-over.
            gl.blend_func_separate(
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE_MINUS_DST_ALPHA,
                glow::ONE,
            );
            if !blending {
                gl.disable(glow::BLEND);
            }
            if scissoring {
                gl.enable(glow::SCISSOR_TEST);
            }
        }
    }

    /// Compile once, on first use, inside the paint callback where there is a
    /// context to compile against.
    fn ready(
        &mut self,
        gl: &glow::Context,
    ) -> Option<(glow::Program, glow::VertexArray, glow::Buffer)> {
        if let (Some(program), Some(array), Some(buffer)) = (self.program, self.array, self.buffer)
        {
            return Some((program, array, buffer));
        }
        // SAFETY: a current context. Everything made here is kept.
        unsafe {
            let program = gl.create_program().ok()?;
            const VERTEX: &str = "#version 150\n\
                in vec2 position;\n\
                in float coverage;\n\
                out float shade;\n\
                void main() {\n\
                    shade = coverage;\n\
                    gl_Position = vec4(position, 0.0, 1.0);\n\
                }\n";
            const FRAGMENT: &str = "#version 150\n\
                in float shade;\n\
                out vec4 colour;\n\
                void main() {\n\
                    colour = vec4(0.0, 0.0, 0.0, shade);\n\
                }\n";
            let mut shaders = Vec::new();
            for (kind, source) in [
                (glow::VERTEX_SHADER, VERTEX),
                (glow::FRAGMENT_SHADER, FRAGMENT),
            ] {
                let shader = gl.create_shader(kind).ok()?;
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    log::warn!("corner eraser: {}", gl.get_shader_info_log(shader));
                    return None;
                }
                gl.attach_shader(program, shader);
                shaders.push(shader);
            }
            gl.bind_attrib_location(program, 0, "position");
            gl.bind_attrib_location(program, 1, "coverage");
            gl.link_program(program);
            for shader in shaders {
                gl.detach_shader(program, shader);
                gl.delete_shader(shader);
            }
            if !gl.get_program_link_status(program) {
                log::warn!("corner eraser: {}", gl.get_program_info_log(program));
                return None;
            }
            let array = gl.create_vertex_array().ok()?;
            let buffer = gl.create_buffer().ok()?;
            self.program = Some(program);
            self.array = Some(array);
            self.buffer = Some(buffer);
            Some((program, array, buffer))
        }
    }
}

/// `f32` vertices as the bytes the GPU wants, without pulling in a crate for it.
fn bytemuck_cast(values: &[f32]) -> &[u8] {
    // SAFETY: `f32` has no padding and no invalid bit patterns, and the result
    // borrows the same slice for the same lifetime.
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

/// Add the cut to `painter`, at this point in its layer.
pub fn cut_corners_into(
    painter: &egui::Painter,
    rect: egui::Rect,
    radius: f32,
    eraser: &Arc<Mutex<Eraser>>,
) {
    let eraser = eraser.clone();
    painter.add(egui::PaintCallback {
        rect,
        callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
            let clip = info.viewport_in_pixels();
            let Ok(mut eraser) = eraser.lock() else {
                return;
            };
            eraser.cut_corners(
                painter.gl(),
                [
                    clip.left_px,
                    clip.from_bottom_px,
                    clip.width_px,
                    clip.height_px,
                ],
                radius * info.pixels_per_point,
            );
        })),
    });
}
