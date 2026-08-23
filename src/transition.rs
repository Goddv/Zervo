//! What happens between two pages.
//!
//! Turn 8c's argument is that a transition should be *derivable* from the
//! geometry rather than chosen from a menu: ask what the page is under this
//! seam — a card, a tint, a frame, a layer — and the motion follows. "Pick the
//! seam and the motion comes with it; there is no second decision to get
//! wrong", which is also why the Appearance page grows no new control for it.
//!
//! So there are four, one per [`Seam`]:
//!
//! - **Card — stack.** The page is a discrete object on a tray, so it behaves
//!   like one in a deck: the front card recedes, back and down and dimmer.
//! - **Frosted — dissolve.** There is no card to move, only a tint on a
//!   backdrop, so the backdrop defocuses. Nothing translates: the window
//!   re-focuses on somewhere else.
//! - **Edge to edge — slide.** The page fills a fixed rectangle with a hard
//!   left edge, which is a filmstrip whether you meant it or not.
//! - **One surface — pass under.** The chrome floats over the page and is the
//!   one fixed thing on screen, so it must not move. The old page keeps going
//!   up and travels out under the glass.
//!
//! # What is here and what is not
//!
//! All four move the page that is *leaving*. Only two of them want the page
//! that is arriving to move as well — the slide's second frame and the stack's
//! card coming forward — and that half is not here.
//!
//! It is not an oversight and it is not small. A web page is a blit of the
//! engine's own framebuffer into the rectangle the engine was handed; moving it
//! means either handing the engine a moving rectangle, which is a resize per
//! frame, or rendering the page to a texture of our own, which is a change to
//! how every page in the browser is drawn. The dissolve and the pass-under are
//! whole as they stand, because the design says of both that the arriving page
//! does not translate.

use std::time::{Duration, Instant};

use egui::{Rect, TextureHandle, pos2, vec2};

use crate::theme::{Palette, Seam};

/// How long a crossing takes.
///
/// 8c names 230ms for the dissolve's blur spike and nothing for the others, so
/// they take the material's own settle time — which is the whole point of
/// having one, and means a reader who has turned motion down gets a shorter
/// crossing without a second slider.
const BASE: f32 = 0.26;

/// Below this the reader has asked for no motion, and a crossing is motion.
const STILL: f32 = 0.02;

/// How far past the edge the two travelling motions go.
///
/// Exactly one width out is where the frame *should* land — the old frame flush
/// against the new one, one track, two frames. A hair further, because "flush"
/// and "one pixel of the old frame still lit along the seam" are the same
/// arithmetic once rounding has had a go at it, and the second is a bright line
/// down the edge of the page at the end of every navigation.
const CLEAR: f32 = 1.02;

/// How far the dissolve's copy is blurred, in pixels of the copy.
///
/// 8c asks for "a real 9px blur" that "spikes for 230ms and settles while the
/// two pages cross through it". Half of that is here: the copy is read back
/// through the same softening the frost uses, so what fades out is a defocused
/// page rather than a sharp one. The live page underneath cannot be blurred —
/// it is an engine blit — so the spike is one-sided.
pub const DEFOCUS: f32 = 9.0;

/// The largest copy taken of a departing page, on its long side.
///
/// A full-resolution readback of a retina content rect is fifteen megabytes and
/// a pipeline stall, on every navigation. This is a picture that will be
/// scaled, faded and gone inside three tenths of a second; half a thousand
/// pixels of it is more than the eye gets to check.
pub const COPY: i32 = 1280;

/// Which way a crossing goes, which only the slide has an opinion about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Way {
    /// Forward: a link, an address, a new tab.
    On,
    /// Backward: the back button, the back gesture, ⌘←.
    Back,
}

/// A picture of the page that is leaving, and the clock it leaves on.
pub struct Crossing {
    page: TextureHandle,
    /// The rect the copy was taken of, in points. The page may have moved or
    /// resized since — the sidebar may even have collapsed — so the motion is
    /// drawn against where it *was*, not where the new one is.
    was: Rect,
    /// Where it may be *seen*, which is not always where it was.
    ///
    /// Under the seam that floats the chrome, the page runs on underneath the
    /// sidebar — so a copy of it is a copy of the sidebar too, and a readback
    /// of a composited framebuffer cannot tell the two apart. Sliding that
    /// upward slides the chrome with it, and the chrome is the one thing 8c
    /// says must not move. So the travel is shown where the page is not
    /// covered, and the glass stays where it is.
    seen: Rect,
    seam: Seam,
    way: Way,
    started: Instant,
    /// The whole crossing's length in seconds, from the material.
    over: f32,
}

impl Crossing {
    pub fn new(
        page: TextureHandle,
        was: Rect,
        seen: Rect,
        seam: Seam,
        way: Way,
        motion: f32,
    ) -> Crossing {
        Crossing {
            page,
            was,
            seen,
            seam,
            way,
            started: Instant::now(),
            // The material's settle time against Zervo's own, so turning motion
            // up or down moves this with everything else.
            over: (BASE * (motion / 0.14).clamp(0.35, 2.5)).clamp(0.08, 0.7),
        }
    }

    /// Whether this arrangement crosses at all.
    pub fn wanted(palette: &Palette) -> bool {
        palette.appearance.motion > STILL
    }

    /// How far through, 0..1.
    fn t(&self) -> f32 {
        (self.started.elapsed().as_secs_f32() / self.over).clamp(0.0, 1.0)
    }

    pub fn done(&self) -> bool {
        self.t() >= 1.0
    }

    /// Draw the departing page over the arriving one.
    ///
    /// Over, not under, for all four: what is arriving is already on the
    /// screen — it is the live page — and the only thing that can be composited
    /// is the copy. A motion that wanted the old page *behind* the new one, as
    /// the stack does, gets it by receding and fading rather than by ordering.
    pub fn draw(&self, painter: &egui::Painter, palette: &Palette) {
        let t = crate::glass::ease_out(self.t());
        if t >= 1.0 {
            return;
        }
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let (rect, alpha) = self.frame(t);
        // A textured *rounded* rect, in one shape.
        //
        // `Painter::image` rasterises a square, and the page it is a copy of is
        // not one under any seam that leaves the content card a corner. Worse,
        // those corners are not transparent in the copy: the corner-erase pass
        // has already run by the time the framebuffer is read, and the readback
        // composites everything partly transparent over the page's own base and
        // forces it opaque. So a square copy paints four solid page-coloured
        // blocks over four rounded corners for the length of the crossing.
        //
        // This was a `rect_stroke` with `Stroke::NONE` after the image, which
        // draws nothing at all — and could not have worked if it had, because a
        // ring cannot subtract the corners of a square already rasterised.
        painter.add(
            egui::epaint::RectShape::filled(
                rect,
                self.radius(palette),
                egui::Color32::WHITE.gamma_multiply(alpha),
            )
            .with_texture(self.page.id(), uv),
        );
    }

    /// The corner the copy is cut to.
    ///
    /// The page's own, and it does not shrink with the card: a corner that
    /// scaled with the stack's recede would be a rounder card rather than a
    /// smaller one.
    fn radius(&self, palette: &Palette) -> egui::CornerRadius {
        crate::theme::content_corners(palette)
    }

    /// Where the departing page is at `t`, and how much of it is left.
    ///
    /// Split out from the drawing so the four motions can be read as four
    /// motions, and tested as arithmetic rather than as pixels.
    fn frame(&self, t: f32) -> (Rect, f32) {
        let was = self.was;
        match self.seam {
            // Back, down and dimmer. The recede is small — a card that shrinks
            // by a tenth has fallen off the tray rather than gone behind the
            // one in front of it.
            Seam::Card => {
                let scale = 1.0 - 0.055 * t;
                let rect =
                    Rect::from_center_size(was.center() + vec2(0.0, 14.0 * t), was.size() * scale);
                (rect, 1.0 - t)
            },
            // Nothing translates: "the window simply re-focuses on somewhere
            // else". The copy was read back blurred, so what fades out over the
            // page arriving is a defocused version of the one leaving.
            Seam::Frosted => (was, 1.0 - t),
            // One track. The old frame leaves the way the reader is going:
            // forward pushes it off to the left, back to the right.
            Seam::EdgeToEdge => {
                let across = was.width() * CLEAR;
                let across = match self.way {
                    Way::On => -across,
                    Way::Back => across,
                };
                (was.translate(vec2(across * t, 0.0)), 1.0)
            },
            // Up and out, under a chrome that does not move. Back sends it the
            // other way, so the two are not the same gesture.
            Seam::OneSurface => {
                let up = was.height() * CLEAR;
                let up = match self.way {
                    Way::On => -up,
                    Way::Back => up,
                };
                (was.translate(vec2(0.0, up * t)), 1.0 - t * 0.35)
            },
        }
    }

    /// Where the departing page may be drawn.
    pub fn clip(&self) -> Rect {
        self.seen
    }

    /// How long until this wants painting again.
    pub fn again(&self) -> Duration {
        Duration::from_millis(8)
    }
}

/// How far a departing page under this seam is blurred on the way out.
pub fn defocus(seam: Seam) -> f32 {
    match seam {
        Seam::Frosted => DEFOCUS,
        _ => 0.0,
    }
}

/// A crossing that has been asked for and not yet had its picture taken.
///
/// The picture can only be taken at one moment in the frame — after the chrome
/// has been painted and before the buffers are swapped, which is the only point
/// where the framebuffer holds the page that is leaving and still holds it.
/// Actions are applied earlier than that, so the request has to wait.
#[derive(Clone, Copy)]
pub struct Wanted {
    pub seam: Seam,
    pub way: Way,
    /// Where the page was, in points — the rect the copy is taken of.
    pub was: Rect,
    /// Where the copy may be shown. See [`Crossing::seen`].
    pub seen: Rect,
    pub motion: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn crossing(seam: Seam, way: Way) -> Crossing {
        let ctx = egui::Context::default();
        let page = ctx.load_texture(
            "test",
            egui::ColorImage::filled([2, 2], egui::Color32::WHITE),
            egui::TextureOptions::LINEAR,
        );
        Crossing {
            page,
            was: Rect::from_min_max(pos2(200.0, 40.0), pos2(1000.0, 700.0)),
            seen: Rect::from_min_max(pos2(200.0, 40.0), pos2(1000.0, 700.0)),
            seam,
            way,
            started: Instant::now(),
            over: 0.26,
        }
    }

    /// Every motion starts exactly where the page was and ends invisible or
    /// entirely outside it. A transition that ends anywhere else is a jump cut
    /// with extra steps.
    #[test]
    fn every_motion_starts_still_and_finishes() {
        for seam in [
            Seam::Card,
            Seam::Frosted,
            Seam::EdgeToEdge,
            Seam::OneSurface,
        ] {
            for way in [Way::On, Way::Back] {
                let crossing = crossing(seam, way);
                let (rect, alpha) = crossing.frame(0.0);
                assert_eq!(rect, crossing.was, "{seam:?} does not start where it was");
                assert!((alpha - 1.0).abs() < 0.001, "{seam:?} does not start whole");

                let (rect, alpha) = crossing.frame(1.0);
                let gone = alpha <= 0.001 || !rect.intersects(crossing.was);
                assert!(
                    gone,
                    "{seam:?} {way:?} is still there at the end: {rect:?} at {alpha}"
                );
            }
        }
    }

    /// The two that translate go opposite ways depending on the direction, and
    /// the two that do not, do not.
    #[test]
    fn only_the_travelling_ones_have_a_direction() {
        for seam in [Seam::Card, Seam::Frosted] {
            let on = crossing(seam, Way::On).frame(0.5).0;
            let back = crossing(seam, Way::Back).frame(0.5).0;
            assert_eq!(on, back, "{seam:?} should not care which way you went");
        }
        for seam in [Seam::EdgeToEdge, Seam::OneSurface] {
            let on = crossing(seam, Way::On).frame(0.5).0;
            let back = crossing(seam, Way::Back).frame(0.5).0;
            assert_ne!(on, back, "{seam:?} should go the other way going back");
        }
    }

    /// The slide moves sideways and the pass-under moves up. Getting these the
    /// wrong way round would be two seams sharing one motion, which is the one
    /// thing 8c is arguing against.
    #[test]
    fn each_seam_moves_along_its_own_axis() {
        let was = crossing(Seam::EdgeToEdge, Way::On).was;
        let slide = crossing(Seam::EdgeToEdge, Way::On).frame(0.5).0;
        assert!(slide.min.x < was.min.x, "the slide goes left going forward");
        assert!((slide.min.y - was.min.y).abs() < 0.001, "and not up");

        let under = crossing(Seam::OneSurface, Way::On).frame(0.5).0;
        assert!(
            under.min.y < was.min.y,
            "the pass-under goes up going forward"
        );
        assert!((under.min.x - was.min.x).abs() < 0.001, "and not sideways");
    }

    /// The settle time is the material's, so turning motion down shortens the
    /// crossing rather than leaving one length behind.
    #[test]
    fn the_crossing_takes_the_materials_own_time() {
        let ctx = egui::Context::default();
        let page = ctx.load_texture(
            "test",
            egui::ColorImage::filled([2, 2], egui::Color32::WHITE),
            egui::TextureOptions::LINEAR,
        );
        let at = |motion: f32| {
            Crossing::new(
                page.clone(),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(10.0, 10.0)),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(10.0, 10.0)),
                Seam::Card,
                Way::On,
                motion,
            )
            .over
        };
        assert!(at(0.06) < at(0.14), "Flat's motion is quicker than Zervo's");
        assert!(at(0.14) < at(0.22), "and Liquid Glass's is slower");
        assert!((at(0.14) - BASE).abs() < 0.001, "Zervo's is the base");
    }
}
