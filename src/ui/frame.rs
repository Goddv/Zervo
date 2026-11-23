//! The chrome behind everything, and the frame around the content card.
//!
//! The chrome background, the card's shadow and outline, and the
//! antialiased masks that round the corners of a page the engine blitted as a
//! square. None of it reads `ChromeContext`: every function here takes a
//! `Palette` and a handful of numbers, which is what made this the first part
//! of `ui.rs` worth lifting out of it.

use egui::epaint::Mesh;
use egui::{Color32, CornerRadius, Rect, Shape, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::glass::{self};
use crate::theme::{self, Palette};

pub(crate) fn snap_rect(rect: Rect, pixels_per_point: f32) -> Rect {
    let snap = |value: f32| (value * pixels_per_point).round() / pixels_per_point;
    Rect::from_min_max(
        pos2(snap(rect.min.x), snap(rect.min.y)),
        pos2(snap(rect.max.x), snap(rect.max.y)),
    )
}

/// A subtle vertical gradient, used to give the chrome surfaces depth.
pub fn vertical_gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
}

/// Smoothstep falloff for a soft gradient.
fn gradient_falloff(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A vertical gradient with an eased (non-linear) colour ramp, emitted as a
/// stack of strips so the curve is actually followed.
pub(crate) fn eased_gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    // NOTE: interpolate in PREMULTIPLIED space, alpha included. `theme::mix`
    // returns an opaque colour, so using it here would make the strip fully
    // opaque with darkened (already premultiplied) RGB — a black band.
    const STRIPS: usize = 32;
    let mut mesh = Mesh::default();
    for strip in 0..=STRIPS {
        let t = strip as f32 / STRIPS as f32;
        let color = top.lerp_to_gamma(bottom, gradient_falloff(t));
        let y = rect.min.y + rect.height() * t;
        mesh.colored_vertex(pos2(rect.min.x, y), color);
        mesh.colored_vertex(pos2(rect.max.x, y), color);
    }
    for strip in 0..STRIPS as u32 {
        let base = strip * 2;
        mesh.add_triangle(base, base + 1, base + 3);
        mesh.add_triangle(base, base + 3, base + 2);
    }
    painter.add(Shape::mesh(mesh));
}

/// Tint left over the window's native glass. Keep only a small workspace
/// wash so the system's material is visible along the inset.
pub fn window_chrome_tint(palette: &Palette, window_glass: bool) -> f32 {
    if window_glass {
        0.12
    } else {
        palette.chrome_tint().clamp(0.2, 1.0)
    }
}

/// Paint the chrome background beneath the page and sidebar.
pub(crate) fn paint_chrome_base(
    root: &Ui,
    palette: &Palette,
    opacity: f32,
    sidebar_right: Option<f32>,
) {
    let window = root.ctx().content_rect();
    let mut painter = root.ctx().layer_painter(egui::LayerId::background());
    if let Some(right) = sidebar_right {
        // Leave the framebuffer clear over the AppKit sidebar material.
        let clip = Rect::from_min_max(pos2(right, window.top()), window.max);
        painter = painter.with_clip_rect(clip);
    }
    painter.rect_filled(
        window,
        CornerRadius::ZERO,
        palette.bg.gamma_multiply(opacity),
    );
}

/// Compute the inset card rect. Full Page mode leaves the top edge clear.
pub(crate) fn paint_content_backdrop(root: &Ui, outer: Rect, palette: &Palette, top: f32) -> Rect {
    // The chrome background is painted once for the entire window.
    // The card's shadow is NOT painted here: the webview blit overwrites the
    // whole square content rect, which would erase the shadow inside the
    // rounded corners and leave square unshadowed patches. It is painted as
    // a ring in `finish_content_frame`, after the corner masks.
    let margin = theme::content_margin(palette);
    let inset = Rect::from_min_max(
        pos2(outer.min.x + margin, outer.min.y + top),
        outer.max - vec2(margin, margin),
    );
    snap_rect(inset, root.pixels_per_point())
}

/// Draw the rounded-corner masks and border over the (square) webview blit.
/// Must run after the blit callback is registered on the background layer.
/// The masks are oversized by a pixel so no sliver of the square blit can
/// peek out at fractional DPI. `mask_corners` is false for internal pages
/// whose fill is already rounded — masking there double-paints the corners.
/// How far a floating panel may hang off the page and still frost against it.
///
/// A hover card drops out of a toolbar button, and with the widget shelf open
/// there can be a good stretch of chrome between the two — so the card sat
/// entirely above the page, found nothing to frost against, and came out flat
/// while everything below it was glass. Generous on purpose: these panels
/// belong to the page they are opened over, and the sampler clamps, so what a
/// distant one gets is the page's own edge carried up to it.
pub const PANEL_REACH: f32 = 400.0;

/// Describe the shadow spread relative to its default size.
pub(crate) fn spread_note(amount: f32) -> String {
    if (amount - 1.0).abs() < 0.02 {
        "Default.".to_owned()
    } else {
        format!("{:.0}% of default.", amount * 100.0)
    }
}

/// A soft shadow outside the content card's silhouette.
pub(crate) fn paint_card_shadow(
    painter: &egui::Painter,
    rect: Rect,
    radius: f32,
    palette: &Palette,
    amount: f32,
) {
    painter.add(glass::shadow(
        rect,
        radius,
        palette.shadow.gamma_multiply((0.9 * amount).min(1.0)),
        9.0 * amount,
        glass::Inner::Outside,
    ));
}

/// How the content card's square corners get rounded off.
///
/// Which of these applies is not a style choice — it is what the window can
/// actually do. Cutting a corner away and painting the chrome back over the
/// hole gives a perfect match, because it is the same paint on the same
/// backdrop as the chrome beside it; but an erased pixel is only a hole where
/// the window composites with alpha and something sits behind it. On an opaque
/// window the destination-out pass takes the colour with it and leaves black.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Corners {
    /// Internally rounded content, or a cut that directly reveals native chrome.
    NoMask,
    /// Cut out of the framebuffer. Paint the chrome back at its own tint.
    Cut,
    /// Nothing could be cut, so the page's square corner is still there and
    /// has to be hidden. Opaque, because that is what hiding means.
    Masked,
}

/// What Appearance says the card should look like.
///
/// Bundled rather than passed as four positional arguments. `border` used to
/// sit between two `f32`s and next to two `Option`s, and transposing any of
/// them changed the render with nothing to complain about.
#[derive(Clone, Copy)]
pub struct CardFrame {
    /// The same backing tint as the surround, for the cut webview corners.
    pub backdrop_tint: f32,
    pub border: bool,
    /// Spread, when the card casts a shadow.
    pub shadow: Option<f32>,
}

pub fn finish_content_frame(
    root: &Ui,
    content_rect: Rect,
    palette: &Palette,
    corners_style: Corners,
    frame: CardFrame,
) {
    let CardFrame {
        backdrop_tint,
        border,
        shadow,
    } = frame;
    let painter = root.ctx().layer_painter(egui::LayerId::background());
    // Clip the antialiased corner masks to the edges of the webview blit.
    let fan_painter = painter.with_clip_rect(content_rect);
    let radius = theme::content_radius(palette);
    let pad = 1.5;

    // One arc segment per physical pixel, so the facets are sub-pixel and the
    // curve cannot look polygonal on a Retina display.
    // Matched to the eraser's tessellation in backdrop.rs — where the two
    // polygons disagree, the mask's hard mesh edge overhangs the cut's
    // feathered one and the difference reads as facets along the curve.
    let segments = ((radius * root.pixels_per_point() * 2.0).ceil() as usize).clamp(16, 192);
    // One radius per corner, in the same order the framebuffer eraser walks
    // them. They are not all equal once the seam closes: the card is flush
    // against the sidebar then, its left corners are square, and a mask over a
    // corner that was never cut is a chrome-coloured notch on the page.
    let [nw, ne, se, sw] = theme::content_corner_radii(palette);
    // (radius, corner, arc center, start angle, outward direction)
    let corners = [
        (
            nw,
            content_rect.left_top(),
            pos2(content_rect.left() + nw, content_rect.top() + nw),
            std::f32::consts::PI,
            vec2(-1.0, -1.0),
        ),
        (
            ne,
            content_rect.right_top(),
            pos2(content_rect.right() - ne, content_rect.top() + ne),
            1.5 * std::f32::consts::PI,
            vec2(1.0, -1.0),
        ),
        (
            se,
            content_rect.right_bottom(),
            pos2(content_rect.right() - se, content_rect.bottom() - se),
            0.0,
            vec2(1.0, 1.0),
        ),
        (
            sw,
            content_rect.left_bottom(),
            pos2(content_rect.left() + sw, content_rect.bottom() - sw),
            0.5 * std::f32::consts::PI,
            vec2(-1.0, 1.0),
        ),
    ];
    // The corners have been cut out of the framebuffer by now, page and chrome
    // both, so what these masks land on is transparency — and the chrome's own
    // tint over transparency is exactly what the chrome beside the card is.
    // They were opaque before, standing in for a colour they could not know,
    // and every corner showed the difference.
    // Below and beside the card the neighbour is the bare chrome — this one
    // tint over the system's backdrop, which a mask can now match exactly,
    // because the corner beneath it has been cut away and it is the same paint
    // on the same backdrop.
    //
    // Above the card it is the widget shelf, which is frosted like the rest of
    // the chrome rather than opaque — so there was never an opaque neighbour
    // for an opaque mask to match, and the top masks read as dark notches
    // against it. This comment used to say the opposite, and that a hole in the
    // top of the window would show what is behind the window rather than the
    // backdrop; the effect view is installed on the frame view and autoresizes
    // with it, so it covers the top as well as the bottom. Checked by cutting
    // the top corners and painting nothing back: the notch shows the frosted
    // backdrop, same as below.

    // All four corner masks in ONE mesh: independent triangles each get their
    // own antialiased edges, and the AA seams between adjacent fan triangles
    // let the page underneath shine through as hairlines. A single mesh with
    // shared vertices has no interior seams.
    //
    // Meshes are not antialiased at all, though, so the arc where the mask
    // meets the page is a hard, stair-stepped edge. Each arc is therefore
    // retraced afterwards with a stroked line — which epaint *does*
    // antialias — in the same colour, feathering the boundary.
    if corners_style != Corners::NoMask {
        let mut mesh = Mesh::default();
        let mut arc_edges: Vec<(Vec<egui::Pos2>, Color32)> = Vec::new();
        for (radius, corner, center, start_angle, outward) in corners {
            if radius <= 0.5 {
                continue;
            }
            let corner_out = corner + outward * pad;
            let mut arc: Vec<egui::Pos2> = (0..=segments)
                .map(|segment| {
                    let angle = start_angle
                        + (segment as f32 / segments as f32) * 0.5 * std::f32::consts::PI;
                    pos2(
                        center.x + radius * angle.cos(),
                        center.y + radius * angle.sin(),
                    )
                })
                .collect();
            // Push the arc endpoints outward past the rect edges, so the fan
            // overlaps the border area instead of stopping exactly at it.
            let first = arc.first_mut().expect("arc has points");
            if (first.x - corner.x).abs() < (first.y - corner.y).abs() {
                first.x += outward.x * pad;
            } else {
                first.y += outward.y * pad;
            }
            let last = arc.last_mut().expect("arc has points");
            if (last.x - corner.x).abs() < (last.y - corner.y).abs() {
                last.x += outward.x * pad;
            } else {
                last.y += outward.y * pad;
            }

            // Tint each vertex with the chrome color at its own height, so
            // the masks disappear into the top gradient instead of stamping
            // flat background patches over it.
            let base = mesh.vertices.len() as u32;
            // Over a cut corner the chrome's own tint is exactly right: it is
            // the same paint on the same backdrop as the chrome beside it.
            // Over an uncut one it would be a thin wash over the page's white
            // square, so there it has to be opaque — that is the whole job.
            //
            let opacity = if corners_style == Corners::Cut {
                backdrop_tint
            } else {
                1.0
            };
            let fill = palette.bg.gamma_multiply(opacity);
            mesh.colored_vertex(corner_out, fill);
            for point in &arc {
                mesh.colored_vertex(*point, fill);
            }
            for segment in 0..segments as u32 {
                mesh.add_triangle(base, base + 1 + segment, base + 2 + segment);
            }
            // The true arc, endpoints not pushed out, for the AA pass.
            let true_arc: Vec<egui::Pos2> = (0..=segments)
                .map(|segment| {
                    let angle = start_angle
                        + (segment as f32 / segments as f32) * 0.5 * std::f32::consts::PI;
                    pos2(
                        center.x + radius * angle.cos(),
                        center.y + radius * angle.sin(),
                    )
                })
                .collect();

            arc_edges.push((true_arc, fill));
        }
        fan_painter.add(Shape::mesh(mesh));
        // Feather just the arc edge. A wider stroke clips into the page.
        let hairline = 1.0 / root.pixels_per_point();
        for (arc, colour) in arc_edges {
            fan_painter.add(Shape::line(arc, Stroke::new(hairline, colour)));
        }
    }

    // Drawn AFTER the corner masks: filling it beforehand works on internal
    // pages but not on web pages, where the blit wipes the square content rect
    // and leaves unshadowed patches in the corners.
    // The shadow ring follows the card's widest corner.
    let ring_radius = [nw, ne, se, sw].iter().copied().fold(0.0_f32, f32::max);
    if let Some(amount) = shadow {
        paint_card_shadow(&painter, content_rect, ring_radius, palette, amount);
    }

    // The outline also antialiases the hard edges of the corner meshes.
    if border {
        let stroke = if palette.translucency == theme::Translucency::LiquidGlass {
            Stroke::new(1.0_f32, palette.border)
        } else {
            Stroke::new(1.2_f32, theme::mix(palette.border, palette.accent, 0.55))
        };
        painter.rect_stroke(
            content_rect,
            theme::content_corners(palette),
            stroke,
            StrokeKind::Middle,
        );
    }
}
