//! Morphing one icon into another.
//!
//! Zervo's icons are Phosphor codepoints rendered as text, so today every
//! state change is a glyph swap: one frame, no continuity, and at seventeen
//! points you frequently cannot tell it happened. Most of the fifty-odd icons
//! do not need better than that — they label a thing rather than report on it.
//! A handful carry state the reader has to track, and those are worth drawing
//! as geometry that can move: which layout you are in, whether something is
//! playing, whether a page is saved, whether a download is still going.
//!
//! ## Where the method comes from
//!
//! This is a port of the approach published by **morphicons**
//! (<https://github.com/guillermolg00/morphicons>, MIT — the notice is
//! vendored at `assets/licenses/MORPHICONS-LICENSE`), which is a TypeScript
//! library and so cannot be linked against from here.
//! What is portable is its argument, which is that a morph should be derived
//! rather than choreographed:
//!
//! 1. normalise every primitive — lines, arcs, circles — to cubic Béziers;
//! 2. resample each outline to a fixed number of points equidistant **by arc
//!    length**, which is what makes two unrelated shapes comparable at all;
//! 3. recover the best rotation and scale between them with a 2D Procrustes
//!    fit;
//! 4. interpolate in polar space, so the rotation falls out of the arithmetic
//!    instead of being declared for every pair by hand.
//!
//! The consequence worth having is that *any* outline morphs into any other.
//! Nothing below knows what a play button is.
//!
//! ## The two things that are easy to get wrong
//!
//! **Sampling by parameter instead of by arc length.** A cubic's parameter is
//! not its length; sampling `t` uniformly bunches points up wherever the curve
//! is tight, so a corner of one shape lines up with the middle of an edge of
//! the other and the morph turns inside out on the way across.
//!
//! **Interpolating the points linearly.** Two points on opposite sides of a
//! shape, moved in straight lines, pass through the middle — so a square
//! rotating to a diamond visibly collapses and re-inflates. Interpolating the
//! radius and the angle separately keeps every point at a sensible distance
//! from the centre for the whole crossing, which is the difference between a
//! shape turning and a shape imploding.

use egui::{Pos2, Vec2, pos2, vec2};

/// How many points an outline is resampled to.
///
/// The number morphicons uses. High enough that a sixteen-point star still
/// reads as one at the halfway mark, low enough that the correspondence search
/// below — which is quadratic in this — stays far cheaper than the tessellator
/// that will draw the result.
pub const SAMPLES: usize = 64;

/// How finely each segment is flattened before the arc-length walk.
///
/// A line needs one step and a cubic needs enough that measuring along the
/// chords is not measurably shorter than measuring along the curve. Sixteen is
/// well past the point where more changes any sampled position by a visible
/// amount at icon sizes.
const FLATTEN: usize = 16;

/// One segment of an outline, starting wherever the previous one ended.
#[derive(Clone, Copy, Debug)]
pub enum Seg {
    Line(Pos2),
    /// A cubic Bézier: two control points and an end point.
    ///
    /// Everything else normalises to this. An SVG quadratic is a cubic with
    /// its controls at two thirds; an arc is a handful of cubics; a circle is
    /// four of them. Keeping one case is what lets the sampler be six lines.
    Curve(Pos2, Pos2, Pos2),
}

/// A closed outline, before it has been sampled.
///
/// Borrowed rather than owned so an icon can be a `static` — these are shapes
/// the program knows at compile time, not data it loads.
#[derive(Clone, Copy, Debug)]
pub struct Path<'a> {
    pub start: Pos2,
    pub segs: &'a [Seg],
    /// Whether the last segment runs back to the first.
    ///
    /// A silhouette is closed and a stroke very often is not — an arrow, a
    /// tick. It matters for more than the extra segment: a closed outline is a
    /// ring, so which sample answers to which is free to rotate, and an open
    /// one has a start and an end that must stay the start and the end. See
    /// [`fit`].
    pub closed: bool,
}

/// One part of an icon: an outline, and whether it is filled or drawn as a
/// line.
#[derive(Clone, Copy, Debug)]
pub struct Part<'a> {
    pub path: Path<'a>,
    /// Filled parts must be convex — epaint tessellates a fill on that
    /// assumption, and a concave one comes out folded over itself. Anything
    /// that is not convex is a line instead, which is what most icon geometry
    /// wants anyway.
    pub filled: bool,
}

/// An icon, as the parts it is drawn from.
///
/// Two figures cross part by part, in order, so a pair has to agree on how
/// many parts it has. That is a real constraint and a useful one: a pause that
/// is two bars and a play that is one triangle would have nothing to
/// interpolate against, so the play is drawn as two halves of a triangle
/// instead and the fold is between four points and four points.
pub type Figure<'a> = &'a [Part<'a>];

/// An outline resampled to [`SAMPLES`] points, equidistant by arc length.
#[derive(Clone, Copy, Debug)]
pub struct Outline {
    points: [Pos2; SAMPLES],
    closed: bool,
}

fn cubic(from: Pos2, a: Pos2, b: Pos2, to: Pos2, t: f32) -> Pos2 {
    let u = 1.0 - t;
    let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    pos2(
        from.x * w0 + a.x * w1 + b.x * w2 + to.x * w3,
        from.y * w0 + a.y * w1 + b.y * w2 + to.y * w3,
    )
}

impl Outline {
    /// Flatten, measure, and walk the outline at equal arc-length steps.
    pub fn sample(path: &Path) -> Outline {
        // ── Flatten to a polyline, keeping the cumulative length as we go.
        let mut walk: Vec<Pos2> = vec![path.start];
        let mut at = path.start;
        for seg in path.segs {
            match *seg {
                Seg::Line(to) => {
                    walk.push(to);
                    at = to;
                },
                Seg::Curve(a, b, to) => {
                    for step in 1..=FLATTEN {
                        walk.push(cubic(at, a, b, to, step as f32 / FLATTEN as f32));
                    }
                    at = to;
                },
            }
        }
        // A closed outline comes back to where it started, or the last sample
        // interpolates across a gap that is not part of the shape. An open one
        // must not, or a stroke grows a return leg nobody drew.
        if path.closed && walk.last().is_some_and(|last| *last != path.start) {
            walk.push(path.start);
        }

        let mut lengths = Vec::with_capacity(walk.len());
        let mut total = 0.0_f32;
        lengths.push(0.0_f32);
        for pair in walk.windows(2) {
            total += (pair[1] - pair[0]).length();
            lengths.push(total);
        }

        let mut points = [path.start; SAMPLES];
        let closed = path.closed;
        if total <= f32::EPSILON {
            // A degenerate outline — every point on top of every other. It has
            // no shape to morph, but it must not produce NaNs for whatever is
            // morphing *to* it.
            return Outline { points, closed };
        }
        let mut cursor = 0_usize;
        for (index, point) in points.iter_mut().enumerate() {
            // A ring is divided into `SAMPLES` arcs and the last one closes
            // back to the first sample; a line is divided into `SAMPLES - 1`,
            // so that the last sample lands on its end rather than short of it.
            let want = if closed {
                total * index as f32 / SAMPLES as f32
            } else {
                total * index as f32 / (SAMPLES - 1) as f32
            };
            while cursor + 2 < lengths.len() && lengths[cursor + 1] < want {
                cursor += 1;
            }
            let span = lengths[cursor + 1] - lengths[cursor];
            let t = if span > f32::EPSILON {
                (want - lengths[cursor]) / span
            } else {
                0.0
            };
            *point = walk[cursor] + (walk[cursor + 1] - walk[cursor]) * t;
        }
        Outline { points, closed }
    }

    pub fn points(&self) -> &[Pos2; SAMPLES] {
        &self.points
    }

    /// The middle of the shape, by its samples.
    ///
    /// Equidistant sampling is what makes this the shape's centroid rather
    /// than a weighted average of wherever its control points happened to be.
    fn centre(&self) -> Pos2 {
        let sum = self
            .points
            .iter()
            .fold(Vec2::ZERO, |sum, point| sum + point.to_vec2());
        (sum / SAMPLES as f32).to_pos2()
    }

    /// How big it is: the root-mean-square distance of its samples from the
    /// centre. Robust in a way that a bounding box is not — one stray point
    /// cannot double it.
    fn spread(&self, centre: Pos2) -> f32 {
        let sum: f32 = self
            .points
            .iter()
            .map(|point| (*point - centre).length_sq())
            .sum();
        (sum / SAMPLES as f32).sqrt()
    }
}

/// The alignment between two outlines: which sample of `b` answers to sample
/// zero of `a`, and the rotation that best carries one onto the other.
struct Fit {
    offset: usize,
    angle: f32,
}

/// A 2D Procrustes fit, over every possible correspondence.
///
/// Resampling gives two outlines the same number of points but says nothing
/// about *which* point matches which: the same square sampled from a different
/// corner is the same square with its indices rolled round. So the offset is
/// searched rather than assumed, and the winner is the one whose optimal
/// rotation leaves the least residual — which, for a Procrustes fit, is the
/// one with the largest `hypot` of the two accumulators below.
fn fit(a: &[Vec2; SAMPLES], b: &[Vec2; SAMPLES], ring: bool) -> Fit {
    let mut best = Fit {
        offset: 0,
        angle: 0.0,
    };
    let mut best_magnitude = f32::NEG_INFINITY;
    // Only a ring may roll. An open stroke has a start and an end, and rolling
    // its correspondence would morph the head of one shape into the tail of
    // the other — which is a shape turning inside out, not a shape changing.
    let offsets = if ring { SAMPLES } else { 1 };
    for offset in 0..offsets {
        let mut cos = 0.0_f32;
        let mut sin = 0.0_f32;
        for index in 0..SAMPLES {
            let p = a[index];
            let q = b[(index + offset) % SAMPLES];
            cos += p.x * q.x + p.y * q.y;
            sin += p.x * q.y - p.y * q.x;
        }
        let magnitude = cos.hypot(sin);
        if magnitude > best_magnitude {
            best_magnitude = magnitude;
            best = Fit {
                offset,
                angle: sin.atan2(cos),
            };
        }
    }
    best
}

fn rotate(v: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

/// The shorter way round from one angle to another.
fn shortest(from: f32, to: f32) -> f32 {
    let mut delta = to - from;
    while delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    }
    while delta < -std::f32::consts::PI {
        delta += std::f32::consts::TAU;
    }
    delta
}

/// `a` on its way to `b`, `t` of the distance across.
///
/// At `t` of zero this is `a` exactly and at one it is `b` exactly, whatever
/// the fit made of them — the alignment decides the route, never the
/// endpoints.
pub fn between(a: &Outline, b: &Outline, t: f32) -> [Pos2; SAMPLES] {
    let t = t.clamp(0.0, 1.0);
    let (centre_a, centre_b) = (a.centre(), b.centre());
    let (spread_a, spread_b) = (a.spread(centre_a), b.spread(centre_b));
    if spread_a <= f32::EPSILON || spread_b <= f32::EPSILON {
        // One of them has no extent, so there is no rotation to recover and
        // nothing sensible to divide by. Cross straight over instead.
        let mut out = [Pos2::ZERO; SAMPLES];
        for (index, point) in out.iter_mut().enumerate() {
            *point = a.points[index] + (b.points[index] - a.points[index]) * t;
        }
        return out;
    }

    // Both shapes moved to the origin and scaled to the same size, which is
    // the only frame in which "how far has it rotated" is a question with an
    // answer.
    let mut unit_a = [Vec2::ZERO; SAMPLES];
    let mut unit_b = [Vec2::ZERO; SAMPLES];
    for index in 0..SAMPLES {
        unit_a[index] = (a.points[index] - centre_a) / spread_a;
        unit_b[index] = (b.points[index] - centre_b) / spread_b;
    }
    let Fit { offset, angle } = fit(&unit_a, &unit_b, a.closed && b.closed);

    let centre = centre_a + (centre_b - centre_a) * t;
    let spread = spread_a + (spread_b - spread_a) * t;
    let mut out = [Pos2::ZERO; SAMPLES];
    for (index, point) in out.iter_mut().enumerate() {
        let from = unit_a[index];
        // `b`'s matching sample, turned back into `a`'s orientation, so what
        // is left between the two is shape rather than shape *and* rotation.
        let to = rotate(unit_b[(index + offset) % SAMPLES], -angle);
        // Polar, not linear: a point crossing to the far side of the shape
        // goes round rather than through the middle.
        let (radius_from, radius_to) = (from.length(), to.length());
        let angle_from = from.y.atan2(from.x);
        let radius = radius_from + (radius_to - radius_from) * t;
        let theta = angle_from + shortest(angle_from, to.y.atan2(to.x)) * t;
        // And the rotation the fit recovered, applied over the same crossing,
        // which is the whole reason none of this had to be choreographed.
        let turned = rotate(vec2(theta.cos(), theta.sin()) * radius, angle * t);
        *point = centre + turned * spread;
    }
    out
}

/// Two figures, one crossing, and the points to draw at any moment of it.
///
/// Sampled on the spot rather than cached: sixty-four points across a handful
/// of parts is arithmetic a modern machine does in microseconds, and a cache
/// keyed on "which icon, which state" is a bug surface that buys nothing.
pub fn figure_between(from: Figure, to: Figure, t: f32) -> Vec<(Vec<Pos2>, bool, bool)> {
    from.iter()
        .zip(to.iter())
        .map(|(a, b)| {
            let points = between(&Outline::sample(&a.path), &Outline::sample(&b.path), t);
            // Filled, and closed, only while both ends agree they should be. A
            // part that fills at one end and strokes at the other has no
            // answer in between, so it takes the quieter of the two.
            (
                points.to_vec(),
                a.filled && b.filled,
                a.path.closed && b.path.closed,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(size: f32) -> Path<'static> {
        static SEGS: [Seg; 4] = [
            Seg::Line(pos2(1.0, -1.0)),
            Seg::Line(pos2(1.0, 1.0)),
            Seg::Line(pos2(-1.0, 1.0)),
            Seg::Line(pos2(-1.0, -1.0)),
        ];
        let _ = size;
        Path {
            start: pos2(-1.0, -1.0),
            segs: &SEGS,
            closed: true,
        }
    }

    /// A circle, as four cubics. The magic number is the one that makes a
    /// cubic hug a quarter turn.
    fn circle() -> Path<'static> {
        const K: f32 = 0.552_284_7;
        static SEGS: [Seg; 4] = [
            Seg::Curve(pos2(K, 1.0), pos2(1.0, K), pos2(1.0, 0.0)),
            Seg::Curve(pos2(1.0, -K), pos2(K, -1.0), pos2(0.0, -1.0)),
            Seg::Curve(pos2(-K, -1.0), pos2(-1.0, -K), pos2(-1.0, 0.0)),
            Seg::Curve(pos2(-1.0, K), pos2(-K, 1.0), pos2(0.0, 1.0)),
        ];
        Path {
            start: pos2(0.0, 1.0),
            segs: &SEGS,
            closed: true,
        }
    }

    /// The property the whole sampler exists for: points equidistant *along
    /// the outline*, not along its parameter. Sampling a cubic by `t` bunches
    /// them wherever the curve is tight, and a morph built on that lines a
    /// corner of one shape up with the middle of an edge of the other.
    ///
    /// Measured on a circle, because a circle is the one closed outline where
    /// equal arcs mean exactly equal chords — anywhere there is a corner, the
    /// chord across it is legitimately shorter than the arc and a test on
    /// chords would be testing the wrong thing.
    #[test]
    fn samples_are_equidistant_along_the_outline() {
        let outline = Outline::sample(&circle());
        let gaps: Vec<f32> = outline
            .points()
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).length())
            .collect();
        let mean = gaps.iter().sum::<f32>() / gaps.len() as f32;
        for gap in &gaps {
            assert!(
                (gap - mean).abs() < mean * 0.01,
                "a gap of {gap} against a mean of {mean} is not equidistant"
            );
        }
        // And they are on the circle, rather than merely evenly spaced along
        // something else.
        for point in outline.points() {
            let radius = point.to_vec2().length();
            assert!((radius - 1.0).abs() < 0.01, "a sample sits at {radius}");
        }
    }

    /// Both ends are exact. Whatever the fit decides about the route, it must
    /// not move the shapes it was fitting.
    #[test]
    fn the_ends_are_the_shapes_themselves() {
        let a = Outline::sample(&square(1.0));
        static SEGS: [Seg; 3] = [
            Seg::Line(pos2(20.0, 0.0)),
            Seg::Line(pos2(10.0, 18.0)),
            Seg::Line(pos2(0.0, 0.0)),
        ];
        let b = Outline::sample(&Path {
            start: pos2(0.0, 0.0),
            segs: &SEGS,
            closed: true,
        });
        for (index, point) in between(&a, &b, 0.0).iter().enumerate() {
            assert!((*point - a.points()[index]).length() < 1e-3, "t=0 moved it");
        }
        // At the far end the *shape* has to be exact, but the correspondence
        // is allowed to have rolled: the fit is free to decide that sample
        // zero of one outline answers to sample seventeen of the other, and
        // for a closed ring that is the same ring. So the assertion is that
        // some rotation of the indices matches, not that the identity one
        // does.
        let arrived = between(&a, &b, 1.0);
        let matched = (0..SAMPLES).any(|offset| {
            arrived.iter().enumerate().all(|(index, point)| {
                (*point - b.points()[(index + offset) % SAMPLES]).length() < 1e-3
            })
        });
        assert!(matched, "t=1 is not the shape it was crossing to");
    }

    /// The one that catches a linear interpolation pretending to be a morph.
    ///
    /// A square crossing to the same square turned by a quarter is the same
    /// square all the way — it should rotate, and every sample should stay the
    /// same distance from the centre. Interpolate the points in straight lines
    /// instead and the halfway mark is visibly smaller than either end, which
    /// reads as the icon imploding and re-inflating.
    #[test]
    fn a_rotation_turns_rather_than_collapsing() {
        let a = Outline::sample(&square(1.0));
        static TURNED: [Seg; 4] = [
            Seg::Line(pos2(1.0, 1.0)),
            Seg::Line(pos2(-1.0, 1.0)),
            Seg::Line(pos2(-1.0, -1.0)),
            Seg::Line(pos2(1.0, -1.0)),
        ];
        let b = Outline::sample(&Path {
            start: pos2(1.0, -1.0),
            segs: &TURNED,
            closed: true,
        });
        let reference = a.spread(a.centre());
        for step in 0..=10 {
            let t = step as f32 / 10.0;
            let points = between(&a, &b, t);
            let centre = points
                .iter()
                .fold(Vec2::ZERO, |sum, point| sum + point.to_vec2())
                / SAMPLES as f32;
            let spread = (points
                .iter()
                .map(|point| (*point - centre.to_pos2()).length_sq())
                .sum::<f32>()
                / SAMPLES as f32)
                .sqrt();
            assert!(
                (spread - reference).abs() < reference * 0.06,
                "at t={t} the shape is {spread} across against {reference} at both ends"
            );
        }
    }

    /// A shape crossing to itself never moves, whatever the fit says about it.
    #[test]
    fn a_shape_crossing_to_itself_stays_put() {
        let a = Outline::sample(&square(1.0));
        for step in 0..=4 {
            let t = step as f32 / 4.0;
            for (index, point) in between(&a, &a, t).iter().enumerate() {
                assert!(
                    (*point - a.points()[index]).length() < 1e-3,
                    "t={t} moved sample {index}"
                );
            }
        }
    }

    /// An open stroke keeps its ends.
    ///
    /// A ring may roll its correspondence, because rolling a ring gives the
    /// same ring. A line may not: roll it and the head of one shape morphs
    /// into the tail of the other, which is a stroke turning inside out on the
    /// way across rather than a stroke changing shape.
    #[test]
    fn an_open_stroke_keeps_its_ends() {
        static DOWN: [Seg; 2] = [Seg::Line(pos2(0.0, 10.0)), Seg::Line(pos2(6.0, 4.0))];
        static UP: [Seg; 2] = [Seg::Line(pos2(0.0, -10.0)), Seg::Line(pos2(6.0, -4.0))];
        let a = Outline::sample(&Path {
            start: pos2(-6.0, 4.0),
            segs: &DOWN,
            closed: false,
        });
        let b = Outline::sample(&Path {
            start: pos2(-6.0, -4.0),
            segs: &UP,
            closed: false,
        });
        // The first sample of one has to be travelling to the first sample of
        // the other, and the same at the far end.
        let half = between(&a, &b, 0.5);
        for index in [0, SAMPLES - 1] {
            let near_a = (half[index] - a.points()[index]).length();
            let near_b = (half[index] - b.points()[index]).length();
            let across = (a.points()[index] - b.points()[index]).length();
            assert!(
                near_a < across && near_b < across,
                "end {index} is not between the two ends it belongs to"
            );
        }
        // And the last sample lands on the path's end rather than short of it.
        assert!(
            (a.points()[SAMPLES - 1] - pos2(6.0, 4.0)).length() < 1e-3,
            "an open outline stopped before its end"
        );
    }

    /// A degenerate outline must not produce NaNs for whatever is morphing to
    /// or from it — an icon with no extent is a state a program can reach.
    #[test]
    fn nothing_becomes_not_a_number() {
        static NOTHING: [Seg; 1] = [Seg::Line(pos2(5.0, 5.0))];
        let empty = Outline::sample(&Path {
            start: pos2(5.0, 5.0),
            segs: &NOTHING,
            closed: true,
        });
        let square = Outline::sample(&square(1.0));
        for (a, b) in [(&empty, &square), (&square, &empty)] {
            for step in 0..=4 {
                for point in between(a, b, step as f32 / 4.0) {
                    assert!(point.x.is_finite() && point.y.is_finite());
                }
            }
        }
    }
}
