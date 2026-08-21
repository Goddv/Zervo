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

        // SAFETY: the caller guarantees a current context, which is what makes
        // every call below sound at all.
        //
        // The one that needs more than that is `read_pixels`. glow hands the
        // driver `pixels.as_mut_ptr()` and no length, so how many bytes get
        // written is decided by `PACK_ALIGNMENT`, `PACK_ROW_LENGTH` and the
        // `PACK_SKIP_*` values — none of which belong to this function. With
        // the defaults the arithmetic below is exact; with a row length left
        // set by another user of the context it would be a heap overflow. They
        // are therefore set explicitly before the read and put back after,
        // rather than assumed.
        //
        // Bindings this changes are restored too, but that is about not
        // corrupting egui's next draw call, not about soundness.
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
            // Drained first, so what is checked after the blit is the blit's
            // own error and not one left lying about by an earlier call.
            while gl.get_error() != glow::NO_ERROR {}
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

            // A downscaling blit out of a multisampled read framebuffer is
            // GL_INVALID_OPERATION, and so is one between incompatible formats.
            // Either way nothing is written, and the read below would then hand
            // back whatever the texture happened to contain — which is why it
            // is allocated cleared, and why this is worth noticing rather than
            // blurring and painting.
            let blit_error = gl.get_error();
            if blit_error != glow::NO_ERROR {
                log::warn!("backdrop: the page copy failed (GL error {blit_error:#x})");
                restore(glow::READ_FRAMEBUFFER, previous_read);
                restore(glow::DRAW_FRAMEBUFFER, previous_draw);
                if scissoring {
                    gl.enable(glow::SCISSOR_TEST);
                }
                return;
            }

            let mut pixels = vec![0_u8; (small.0 * small.1 * 4) as usize];
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(framebuffer));
            // See the SAFETY note above: these two decide how much the driver
            // writes through the pointer, so they are stated rather than hoped
            // for. A tightly packed RGBA row is a multiple of four bytes, so an
            // alignment of 4 matches the buffer exactly.
            let previous_alignment = gl.get_parameter_i32(glow::PACK_ALIGNMENT);
            let previous_row_length = gl.get_parameter_i32(glow::PACK_ROW_LENGTH);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 4);
            gl.pixel_store_i32(glow::PACK_ROW_LENGTH, 0);
            gl.read_pixels(
                0,
                0,
                small.0,
                small.1,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, previous_alignment);
            gl.pixel_store_i32(glow::PACK_ROW_LENGTH, previous_row_length);

            restore(glow::READ_FRAMEBUFFER, previous_read);
            restore(glow::DRAW_FRAMEBUFFER, previous_draw);
            if scissoring {
                gl.enable(glow::SCISSOR_TEST);
            }

            self.ready = blur(pixels, small);
            self.taken_at = Some(Instant::now());
        }
    }

    /// # Safety
    ///
    /// A GL context must be current on this thread.
    unsafe fn allocate(
        gl: &glow::Context,
        size: (i32, i32),
    ) -> Option<(glow::Framebuffer, glow::Texture)> {
        // SAFETY: the caller promises a current context, which is the whole of
        // what these calls need. The framebuffer binding is put back before
        // returning.
        unsafe {
            let texture = gl.create_texture().ok()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            // Given pixels rather than `Slice(None)`, which leaves the contents
            // undefined. If a later blit into this ever fails — a multisampled
            // source is enough — the read that follows returns whatever was in
            // that video memory, and the chrome frosts itself against it. Clear
            // is a poor backdrop; somebody else's freed texture is worse.
            let blank = vec![0_u8; (size.0 as usize) * (size.1 as usize) * 4];
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                size.0,
                size.1,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&blank)),
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

    /// # Safety
    ///
    /// A GL context must be current on this thread, and it must be the one
    /// these objects were made on.
    unsafe fn release(&mut self, gl: &glow::Context) {
        // SAFETY: the caller promises the right context. Each object is taken
        // out of its `Option` as it is deleted, so nothing can be deleted twice.
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
    /// Set once the GL objects could not be built, so it is not attempted
    /// again. A driver that will not compile this shader today will not
    /// compile it on the next frame either.
    failed: bool,
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
    /// All four. This used to be the bottom two only, on the belief that a
    /// hole along the top of the window would show whatever is behind the
    /// window rather than the backdrop, and that the chrome above the card was
    /// opaque toolbar furniture anyway. Neither holds: the effect view is
    /// installed on the frame view and autoresizes with it, so it covers the
    /// top as readily as the bottom, and the widget shelf above the card is
    /// frosted like the rest of the chrome.
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

        // All four corners, and for each a fan of wedges from the square corner
        // out to the arc, plus the feathered band across it.
        // Two steps per physical pixel of radius. One was enough while this
        // only ran along the bottom edge, where the arc meets flat chrome;
        // against the lit band at the top the facets are visible as stair
        // steps on the curve.
        let steps = ((radius_px * 2.0).ceil() as usize).clamp(16, 192);
        let mut vertices: Vec<f32> = Vec::with_capacity(steps * 36);
        let to_ndc = |px: f32, py: f32| {
            (
                (px - x as f32) / width as f32 * 2.0 - 1.0,
                (py - y as f32) / height as f32 * 2.0 - 1.0,
            )
        };
        // (corner, arc centre, the quadrant's start angle). GL's origin is at
        // the bottom left, so `y` is the card's bottom edge and `y + height`
        // its top.
        //
        // This used to cut the bottom two only, and the top two were painted
        // over instead — an opaque mask laid across the page's square corner.
        // A mask cannot match its surroundings: the chrome beside it is a thin
        // tint over a backdrop the window server composites outside this
        // framebuffer, so there is nothing to read and nothing to reproduce,
        // and the code that did it said as much. Cutting all four means the
        // chrome is drawn back over a hole in every corner — the same paint on
        // the same backdrop, with nothing left to match.
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
            (
                (x as f32, (y + height) as f32),
                (x as f32 + radius_px, (y + height) as f32 - radius_px),
                0.5 * std::f32::consts::PI,
            ),
            (
                ((x + width) as f32, (y + height) as f32),
                (
                    (x + width) as f32 - radius_px,
                    (y + height) as f32 - radius_px,
                ),
                0.0,
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
        if self.failed {
            return None;
        }
        // SAFETY: a current context.
        let built = unsafe { Self::build(gl) };
        match built {
            Some((program, array, buffer)) => {
                self.program = Some(program);
                self.array = Some(array);
                self.buffer = Some(buffer);
                Some((program, array, buffer))
            },
            None => {
                // Remembered, and not retried. `cut_corners` runs every frame,
                // so without this a driver that rejects the shader had a
                // program and two shaders abandoned to it sixty times a second
                // — none of them ever deleted, because nothing set
                // `self.program` and nothing tidied up on the way out.
                self.failed = true;
                None
            },
        }
    }

    /// Delete a half-built program and whatever shaders were attached to it.
    ///
    /// # Safety
    ///
    /// A GL context must be current, and `program` and `shaders` must be live
    /// objects belonging to it.
    unsafe fn discard(gl: &glow::Context, program: glow::Program, shaders: &[glow::Shader]) {
        // SAFETY: the caller promises live objects on a current context, and
        // this is the only path that deletes them — `build` hands them over
        // exactly once, on the way out.
        unsafe {
            for shader in shaders {
                gl.detach_shader(program, *shader);
                gl.delete_shader(*shader);
            }
            gl.delete_program(program);
        }
    }

    /// # Safety
    ///
    /// A GL context must be current on this thread.
    unsafe fn build(
        gl: &glow::Context,
    ) -> Option<(glow::Program, glow::VertexArray, glow::Buffer)> {
        // SAFETY: the caller promises a current context. Everything made here
        // is either returned or deleted before returning, so no name outlives
        // this call unowned.
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
                let Ok(shader) = gl.create_shader(kind) else {
                    Self::discard(gl, program, &shaders);
                    return None;
                };
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    log::warn!("corner eraser: {}", gl.get_shader_info_log(shader));
                    gl.delete_shader(shader);
                    Self::discard(gl, program, &shaders);
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
                gl.delete_program(program);
                return None;
            }
            let Ok(array) = gl.create_vertex_array() else {
                gl.delete_program(program);
                return None;
            };
            let Ok(buffer) = gl.create_buffer() else {
                gl.delete_vertex_array(array);
                gl.delete_program(program);
                return None;
            };
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
