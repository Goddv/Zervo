//! GPU colour targets for composing a complete frame before publishing it.
use glow::HasContext as _;

pub(crate) struct ColorTarget {
    pub(crate) framebuffer: glow::Framebuffer,
    color: glow::Renderbuffer,
    pub(crate) size: [u32; 2],
}

impl ColorTarget {
    pub(crate) fn new(gl: &glow::Context, size: [u32; 2]) -> Option<Self> {
        if size.contains(&0) {
            return None;
        }
        // SAFETY: called on the render thread with its context current. Restore the
        // caller's bindings and release both names if the allocation is incomplete.
        unsafe {
            let framebuffer = gl.create_framebuffer().ok()?;
            let Ok(color) = gl.create_renderbuffer() else {
                gl.delete_framebuffer(framebuffer);
                return None;
            };
            let draw = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
            let rb = gl.get_parameter_renderbuffer(glow::RENDERBUFFER_BINDING);
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
            gl.renderbuffer_storage(
                glow::RENDERBUFFER,
                glow::RGBA8,
                size[0] as i32,
                size[1] as i32,
            );
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(framebuffer));
            gl.framebuffer_renderbuffer(
                glow::DRAW_FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::RENDERBUFFER,
                Some(color),
            );
            let complete =
                gl.check_framebuffer_status(glow::DRAW_FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, draw);
            gl.bind_renderbuffer(glow::RENDERBUFFER, rb);
            let target = Self {
                framebuffer,
                color,
                size,
            };
            if complete {
                Some(target)
            } else {
                target.destroy(gl);
                None
            }
        }
    }

    pub(crate) fn destroy(self, gl: &glow::Context) {
        // SAFETY: the owning render context is current, and consuming the target
        // prevents releasing these GL names twice.
        unsafe {
            gl.delete_framebuffer(self.framebuffer);
            gl.delete_renderbuffer(self.color);
        }
    }
}

/// Copy into the current draw target. In particular, a page callback must
/// not rebind surfman's window framebuffer while composing the private frame.
pub(crate) fn blit(gl: &glow::Context, source: glow::Framebuffer, from: [i32; 4], dest: [i32; 4]) {
    // SAFETY: both targets are live in this context. A colour blit reads GPU
    // storage only. Restore the incoming read binding and scissor state.
    unsafe {
        let read = gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING);
        let scissor = gl.is_enabled(glow::SCISSOR_TEST);
        gl.disable(glow::SCISSOR_TEST);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(source));
        gl.blit_framebuffer(
            from[0],
            from[1],
            from[2],
            from[3],
            dest[0],
            dest[1],
            dest[2],
            dest[3],
            glow::COLOR_BUFFER_BIT,
            glow::NEAREST,
        );
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, read);
        if scissor {
            gl.enable(glow::SCISSOR_TEST);
        }
    }
}

/// GL starts at the bottom left. Input and layout start at the top left.
pub(crate) fn page_crop(height: i32, width_visible: i32, height_visible: i32) -> [i32; 4] {
    [0, height - height_visible, width_visible, height]
}

#[cfg(test)]
mod tests {
    use super::page_crop;

    #[test]
    fn moving_sidebar_reveals_the_top_left_at_device_pixel_scale() {
        assert_eq!(page_crop(1600, 1840, 1584), [0, 16, 1840, 1600]);
        assert_eq!(page_crop(1600, 2400, 1600), [0, 0, 2400, 1600]);
    }
}
