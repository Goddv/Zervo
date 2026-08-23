//! A picture of the window, taken by the window.
//!
//! Nothing outside the process can screenshot this application without the
//! operating system's screen-recording permission, which a build machine does
//! not have and a developer should not have to grant to check that a chrome
//! matches the design it was drawn from. So the chrome takes its own: the
//! renderer already reads the framebuffer back every frame to frost against,
//! and this is the same read at full size, written out as a PNG.
//!
//! Debug builds only, and only when asked:
//!
//! ```text
//! ZERVO_SHOT=/tmp/chrome.png ZERVO_SHOT_FRAME=90 zervo zervo://newtab
//! ```
//!
//! `ZERVO_SHOT_NAV` navigates somewhere `ZERVO_SHOT_NAV_BEFORE` frames before
//! the picture, which is the only way to photograph a page transition: one
//! exists solely in the middle of itself.
//!
//! It waits `ZERVO_SHOT_FRAME` frames — the default is enough for a wallpaper
//! to arrive and an aurora to settle — then writes the file and asks the event
//! loop to quit. Combined with a throwaway `HOME`, which is where the settings
//! file lives, that makes any arrangement reachable and comparable without
//! touching anybody's profile.

use std::path::PathBuf;

pub struct Shot {
    path: PathBuf,
    at: u32,
    frame: u32,
    /// Somewhere to go a few frames before the picture is taken.
    ///
    /// A page transition only exists in the middle of itself, and a harness
    /// that can only photograph a resting window can never see one. This
    /// navigates on cue so the shot lands mid-crossing.
    nav: Option<String>,
    /// How many frames before the shot to go there.
    nav_before: u32,
}

/// Hold the sidebar's reveal open, so a picture can be taken of it.
///
/// The reveal is opened by the pointer reaching the window's edge, and the
/// harness has no pointer — so the one piece of chrome that only exists while
/// something is being hovered was the one piece it could never photograph.
/// Debug builds only, like everything else here.
/// `ZERVO_SHOT_PEEK=<pass>` holds it open until that pass and then lets go, so
/// the *closing* half — which is where two address pills briefly exist at once
/// — can be photographed too. Bare `ZERVO_SHOT_PEEK=1` holds it open for good.
pub fn peek_forced(pass: u64) -> bool {
    /// Read once. This is asked on every frame, from the middle of the
    /// chrome's layout, and an environment lookup per frame is a cost the
    /// shipped browser should not carry for a harness it cannot use.
    static UNTIL: std::sync::LazyLock<Option<u64>> = std::sync::LazyLock::new(|| {
        if !cfg!(debug_assertions) {
            return None;
        }
        match std::env::var("ZERVO_SHOT_PEEK") {
            // Anything that is not a pass number means "hold it open".
            Ok(value) => Some(
                value
                    .parse::<u64>()
                    .ok()
                    .filter(|at| *at > 1)
                    .unwrap_or(u64::MAX),
            ),
            Err(_) => None,
        }
    });
    UNTIL.is_some_and(|until| pass < until)
}

/// Which step of the first run to open on.
///
/// `ZERVO_SHOT_SETUP=<n>`. The setup is six cards deep and every one of them is
/// reached by pressing a button, so without this the harness can photograph the
/// welcome card and nothing behind it. Debug builds only.
pub fn setup_step() -> Option<u8> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var("ZERVO_SHOT_SETUP")
        .ok()
        .and_then(|value| value.parse().ok())
}

impl Shot {
    /// Asked for, or not. Absent in a release build whatever the environment
    /// says: a browser that can be told to write a picture of itself somewhere
    /// is not a browser anybody should ship.
    pub fn from_env() -> Option<Shot> {
        if !cfg!(debug_assertions) {
            return None;
        }
        let path = PathBuf::from(std::env::var_os("ZERVO_SHOT")?);
        let at = std::env::var("ZERVO_SHOT_FRAME")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(90);
        Some(Shot {
            path,
            at,
            frame: 0,
            nav: std::env::var("ZERVO_SHOT_NAV")
                .ok()
                .filter(|url| !url.is_empty()),
            nav_before: std::env::var("ZERVO_SHOT_NAV_BEFORE")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(6),
        })
    }

    /// Whether this frame should change the layout.
    ///
    /// `ZERVO_SHOT_CYCLE=<n>` presses ⌘S every `n` frames. The sidebar's morph
    /// — the animation that opens and closes it — only exists between two
    /// layouts, so without this the harness can photograph both ends of it and
    /// never the middle, which is where anything that goes wrong goes wrong.
    pub fn cycle(&self) -> bool {
        let Ok(every) = std::env::var("ZERVO_SHOT_CYCLE") else {
            return false;
        };
        every
            .parse::<u32>()
            .ok()
            .filter(|every| *every > 0)
            .is_some_and(|every| self.frame > 0 && self.frame.is_multiple_of(every))
    }

    /// Where to go this frame, if this is the frame to go.
    ///
    /// Called at the top of a redraw, before anything is drawn, so the
    /// navigation is applied to the frame that is about to be painted.
    pub fn navigate(&mut self) -> Option<String> {
        (self.frame + 1 == self.at.saturating_sub(self.nav_before)).then(|| self.nav.take())?
    }

    /// Count a frame, and take the picture on the one that was asked for.
    ///
    /// Returns whether the application should now quit. Must be called after
    /// the chrome has been painted and before the buffers are swapped —
    /// afterwards the back buffer's contents are undefined, which on this
    /// driver means a picture of whatever was on screen two frames ago.
    pub fn tick(&mut self, gl: &glow::Context, size: (u32, u32)) -> bool {
        self.frame += 1;
        if self.frame < self.at {
            return false;
        }
        let (width, height) = (size.0.max(1) as i32, size.1.max(1) as i32);
        let mut pixels = vec![0_u8; (width * height * 4) as usize];
        // SAFETY: a current context with the window's framebuffer bound, which
        // is what the caller has just painted into. `read_pixels` writes as
        // many bytes as the pack state says rather than as many as the slice
        // holds, so alignment and row length are set explicitly here and put
        // back after — the same care `backdrop::Capture` takes, and for the
        // same reason.
        unsafe {
            use glow::HasContext as _;
            let alignment = gl.get_parameter_i32(glow::PACK_ALIGNMENT);
            let row_length = gl.get_parameter_i32(glow::PACK_ROW_LENGTH);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 4);
            gl.pixel_store_i32(glow::PACK_ROW_LENGTH, 0);
            gl.read_pixels(
                0,
                0,
                width,
                height,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, alignment);
            gl.pixel_store_i32(glow::PACK_ROW_LENGTH, row_length);
        }
        // GL's origin is the bottom left and every image format's is the top,
        // so the rows come back upside down.
        let stride = (width * 4) as usize;
        let flipped: Vec<u8> = pixels
            .chunks_exact(stride)
            .rev()
            .flat_map(<[u8]>::to_vec)
            .collect();
        match image::RgbaImage::from_raw(width as u32, height as u32, flipped) {
            Some(image) => match image.save(&self.path) {
                Ok(()) => log::warn!("wrote {}", self.path.display()),
                Err(error) => log::warn!("could not write {}: {error}", self.path.display()),
            },
            None => log::warn!("the framebuffer did not come back the size it said"),
        }
        true
    }
}
