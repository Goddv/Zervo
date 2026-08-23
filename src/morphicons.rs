//! The handful of icons that are worth drawing as geometry.
//!
//! Every other icon in Zervo is a Phosphor codepoint rendered as text, and
//! that is right for them: they label a thing, and a label does not need to be
//! able to move. These few report on state the reader is tracking — which
//! layout they are in, whether something is playing — and for those a swap is
//! one frame with no continuity, which at seventeen points is frequently not
//! noticed at all.
//!
//! The morph itself is [`zervo_core::morph`], which knows nothing about any of
//! these shapes. What is here is the geometry and the two-state pairing.
//!
//! ## The design box
//!
//! Everything below is drawn in a 24×24 box with y downward, matching the
//! convention every icon set uses, and mapped onto whatever rectangle the call
//! site has. The numbers are therefore comparable with the Phosphor glyph they
//! stand in for, which matters when one sits in a row of them.

use egui::{Color32, Pos2, Rect, Stroke, pos2, vec2};
use zervo_core::morph::{Figure, Part, Path, Seg, figure_between};

/// The box the shapes below are drawn in.
const BOX: f32 = 24.0;
/// Phosphor's regular weight, in that box. A morphing icon that is visibly
/// heavier or lighter than the glyphs beside it draws attention to the wrong
/// thing.
const WEIGHT: f32 = 1.8;

/// A closed run of straight lines — a silhouette.
macro_rules! polygon {
    ($start:expr, $($point:expr),+ $(,)?) => {
        Path {
            start: $start,
            segs: &[$(Seg::Line($point)),+],
            closed: true,
        }
    };
}

/// An open one — a stroke, which keeps its two ends.
macro_rules! polyline {
    ($start:expr, $($point:expr),+ $(,)?) => {
        Path {
            start: $start,
            segs: &[$(Seg::Line($point)),+],
            closed: false,
        }
    };
}

// ── Sidebar ⇄ bar ─────────────────────────────────────────────────────────
//
// The same frame either way, and a divider that rotates inside it. The button
// shows the layout you are moving to, and shows it moving — which is the
// whole difference between a button that reports a state and one that just
// happens to be lit.

static FRAME: Path<'static> = polygon!(
    pos2(4.0, 5.0),
    pos2(20.0, 5.0),
    pos2(20.0, 19.0),
    pos2(4.0, 19.0),
);

static DIVIDER_UPRIGHT: Path<'static> = polygon!(
    pos2(9.0, 5.0),
    pos2(10.2, 5.0),
    pos2(10.2, 19.0),
    pos2(9.0, 19.0)
);

static DIVIDER_FLAT: Path<'static> = polygon!(
    pos2(4.0, 9.0),
    pos2(20.0, 9.0),
    pos2(20.0, 10.2),
    pos2(4.0, 10.2)
);

pub static SIDEBAR: [Part<'static>; 2] = [
    Part {
        path: FRAME,
        filled: false,
    },
    Part {
        path: DIVIDER_UPRIGHT,
        filled: true,
    },
];

pub static BAR: [Part<'static>; 2] = [
    Part {
        path: FRAME,
        filled: false,
    },
    Part {
        path: DIVIDER_FLAT,
        filled: true,
    },
];

// ── Play ⇄ pause ──────────────────────────────────────────────────────────
//
// Two bars fold into a triangle. The play is drawn as two halves of one rather
// than as a single triangle, so there are two parts on both sides and the fold
// is between a bar and a half rather than between two bars and nothing.

static PAUSE_LEFT: Path<'static> = polygon!(
    pos2(7.0, 5.0),
    pos2(10.5, 5.0),
    pos2(10.5, 19.0),
    pos2(7.0, 19.0),
);

static PAUSE_RIGHT: Path<'static> = polygon!(
    pos2(13.5, 5.0),
    pos2(17.0, 5.0),
    pos2(17.0, 19.0),
    pos2(13.5, 19.0),
);

static PLAY_LEFT: Path<'static> = polygon!(
    pos2(7.0, 4.0),
    pos2(13.0, 8.0),
    pos2(13.0, 16.0),
    pos2(7.0, 20.0),
);

static PLAY_RIGHT: Path<'static> = polygon!(pos2(13.0, 8.0), pos2(19.0, 12.0), pos2(13.0, 16.0));

pub static PAUSE: [Part<'static>; 2] = [
    Part {
        path: PAUSE_LEFT,
        filled: true,
    },
    Part {
        path: PAUSE_RIGHT,
        filled: true,
    },
];

pub static PLAY: [Part<'static>; 2] = [
    Part {
        path: PLAY_LEFT,
        filled: true,
    },
    Part {
        path: PLAY_RIGHT,
        filled: true,
    },
];

// ── Downloading ⇄ done ────────────────────────────────────────────────────
//
// The shaft retracts into its own base while the chevron straightens into a
// tick. Three points throughout and no swap, which is what makes it read as
// one thing finishing rather than two icons taking turns.
//
// Both parts are open strokes: an arrow and a tick are lines, and drawing them
// as silhouettes would need twice the geometry to say the same thing.

static ARROW_SHAFT: Path<'static> = polyline!(pos2(12.0, 4.5), pos2(12.0, 13.5));

static ARROW_HEAD: Path<'static> = polyline!(pos2(7.0, 9.0), pos2(12.0, 14.0), pos2(17.0, 9.0));

/// Retracted into its own base rather than to nothing: a part with no extent
/// has no orientation, and the fit would have nothing to recover from it.
static TICK_STUB: Path<'static> = polyline!(pos2(12.0, 12.5), pos2(12.0, 14.0));

static TICK: Path<'static> = polyline!(pos2(6.5, 12.0), pos2(10.5, 15.5), pos2(17.5, 7.5));

pub static DOWNLOADING: [Part<'static>; 2] = [
    Part {
        path: ARROW_SHAFT,
        filled: false,
    },
    Part {
        path: ARROW_HEAD,
        filled: false,
    },
];

pub static DONE: [Part<'static>; 2] = [
    Part {
        path: TICK_STUB,
        filled: false,
    },
    Part {
        path: TICK,
        filled: false,
    },
];

/// Draw `from` on its way to `to`, `t` of the way across, inside `rect`.
///
/// `t` of zero is exactly the first figure and one is exactly the second, so a
/// caller with nothing animating can pass either end and get a still icon
/// without a special case.
pub fn draw(painter: &egui::Painter, rect: Rect, from: Figure, to: Figure, t: f32, tint: Color32) {
    // Square and centred, so a shape drawn for a 24×24 box is not stretched by
    // a call site that happened to hand over a rectangle.
    let side = rect.width().min(rect.height());
    let scale = side / BOX;
    let origin = rect.center() - vec2(side, side) / 2.0;
    let place = |point: Pos2| origin + point.to_vec2() * scale;

    for (points, filled, closed) in figure_between(from, to, t) {
        let points: Vec<Pos2> = points.into_iter().map(place).collect();
        let stroke = Stroke::new(WEIGHT * scale, tint);
        match (filled, closed) {
            (true, _) => painter.add(egui::Shape::convex_polygon(points, tint, Stroke::NONE)),
            (false, true) => painter.add(egui::Shape::closed_line(points, stroke)),
            (false, false) => painter.add(egui::Shape::line(points, stroke)),
        };
    }
}
