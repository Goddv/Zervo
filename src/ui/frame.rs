//! The chrome behind everything, and the frame around the content card.
//!
//! Gradients, the glow across the top, the card's halo and shadow, and the
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

/// Height of the chrome's top glow strip, in points.
pub(crate) const CHROME_GRADIENT_HEIGHT: f32 = 280.0;

/// Smoothstep falloff for the glow strip. Its slope reaches zero at the
/// bottom, so the strip melts into the flat chrome instead of ending on a
/// visible line (a linear ramp leaves a mach band where the slope breaks).
pub(crate) fn glow_falloff(t: f32) -> f32 {
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
        let color = lerp_premultiplied(top, bottom, glow_falloff(t));
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

/// The lightest chrome color, at the very top of the window: a whisper of
/// accent, echoing Zen Browser's workspace tinting. Light comes from above in both
/// themes, so the top is always the lighter end.
pub(crate) fn glow_strip_top(palette: &Palette, strength: f32) -> Color32 {
    let full = theme::mix(
        {
            let lift = if palette.dark { 14 } else { 6 };
            Color32::from_rgb(
                (palette.bg.r() as i16 + lift).clamp(0, 255) as u8,
                (palette.bg.g() as i16 + lift).clamp(0, 255) as u8,
                (palette.bg.b() as i16 + lift + 2).clamp(0, 255) as u8,
            )
        },
        palette.accent,
        0.07,
    );
    // Scale the whole effect back toward the flat chrome color.
    theme::mix(palette.bg, full, strength.clamp(0.0, 1.0))
}

/// The chrome color at a given window-space `y` — used both to paint the
/// gradient and to tint anything that must blend into it seamlessly.
/// The colour `paint_chrome_fill` actually leaves at height `y`, alpha and all.
///
/// Not `chrome_color_at` multiplied by the opacity afterwards. That mixes the
/// gradient at full alpha and multiplies the result; the fill multiplies its
/// two ends first and mixes those — and it clamps the opacity to a fifth,
/// which anything reading the raw setting does not. Between them that came to
/// three units out of 255, which is precisely what the corner masks were still
/// showing against the chrome beside them.
pub(crate) fn chrome_fill_at(
    root: &Ui,
    y: f32,
    palette: &Palette,
    top_glow: f32,
    opacity: f32,
) -> Color32 {
    let opacity = opacity.clamp(0.2, 1.0);
    let bg = palette.bg.gamma_multiply(opacity);
    if top_glow <= 0.0 {
        return bg;
    }
    let top = root.ctx().content_rect().top();
    if y >= top + CHROME_GRADIENT_HEIGHT {
        return bg;
    }
    let t = glow_falloff((y - top) / CHROME_GRADIENT_HEIGHT);
    // Premultiplied, because both ends are translucent whenever `opacity` is.
    // `theme::mix` returns an opaque colour, so it handed back the glow's RGB
    // already scaled down by the tint and then declared it fully opaque — a
    // near-black patch wherever the chrome is see-through and the glow band
    // reaches. The card's bottom corners sit past the band and took the early
    // return above, which is why only the top two were ever black.
    lerp_premultiplied(
        glow_strip_top(palette, top_glow).gamma_multiply(opacity),
        bg,
        t,
    )
}

pub(crate) fn chrome_color_at(root: &Ui, y: f32, palette: &Palette, top_glow: f32) -> Color32 {
    if top_glow <= 0.0 {
        return palette.bg;
    }
    let top = root.ctx().content_rect().top();
    let t = glow_falloff((y - top) / CHROME_GRADIENT_HEIGHT);
    theme::mix(glow_strip_top(palette, top_glow), palette.bg, t)
}

/// Flat chrome fill plus the window-wide top gradient, painted under
/// everything else.
pub(crate) fn paint_chrome_base(root: &Ui, palette: &Palette, top_glow: f32, opacity: f32) {
    let painter = root.ctx().layer_painter(egui::LayerId::background());
    let window = root.ctx().content_rect();
    paint_chrome_fill(&painter, window, window.top(), palette, top_glow, opacity);
}

/// Blend two colours that are already premultiplied, keeping them
/// premultiplied.
///
/// `theme::mix` cannot do this: it returns an *opaque* colour. Handed two
/// translucent premultiplied colours it produces one whose RGB has been scaled
/// down by an alpha it then throws away — which at a low tint is very nearly
/// black. ARCHITECTURE.md has warned about this for as long as it has existed;
/// it is what made the content card's top corners black.
pub(crate) fn lerp_premultiplied(a: Color32, b: Color32, t: f32) -> Color32 {
    let inv = 1.0 - t;
    let channel = |a: u8, b: u8| (a as f32 * inv + b as f32 * t) as u8;
    Color32::from_rgba_premultiplied(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
        channel(a.a(), b.a()),
    )
}

/// Fill `rect` with the chrome's colour, including the glow band across the
/// top of the window. `window_top` anchors the band, so a rect that starts
/// below the window's top edge still lines up with the rest of the chrome.
pub(crate) fn paint_chrome_fill(
    painter: &egui::Painter,
    rect: Rect,
    window_top: f32,
    palette: &Palette,
    top_glow: f32,
    opacity: f32,
) {
    let opacity = opacity.clamp(0.2, 1.0);
    let bg = palette.bg.gamma_multiply(opacity);
    if top_glow <= 0.0 {
        painter.rect_filled(rect, CornerRadius::ZERO, bg);
        return;
    }

    // The glow band and the flat fill below it are painted as DISJOINT rects.
    // Overlapping them would composite two translucent layers on top of each
    // other inside the band, making it measurably more opaque than the chrome
    // below and leaving a hard horizontal seam where the band ends.
    let band_bottom = (window_top + CHROME_GRADIENT_HEIGHT).min(rect.max.y);
    let band = Rect::from_min_max(rect.min, pos2(rect.max.x, band_bottom));
    let rest = Rect::from_min_max(pos2(rect.min.x, band_bottom), rect.max);
    if rest.is_positive() {
        painter.rect_filled(rest, CornerRadius::ZERO, bg);
    }
    if band.is_positive() {
        eased_gradient(
            painter,
            band,
            glow_strip_top(palette, top_glow).gamma_multiply(opacity),
            bg,
        );
    }
}

/// Compute the inset card rect. (Nothing is painted here — see the note.)
/// `top` is separate because the navigation bar already ends in the space the
/// card would otherwise add for itself. Applying both put 14pt under the
/// address pill against 6pt above it, which is the card sitting lower than it
/// needs to rather than a margin anyone chose.
pub(crate) fn paint_content_backdrop(root: &Ui, outer: Rect, _palette: &Palette, top: f32) -> Rect {
    // No background fill here: the window-wide chrome base already covers
    // this area, and filling it again would cut the gradient off at the
    // sidebar edge.
    // The card's shadow is NOT painted here: the webview blit overwrites the
    // whole square content rect, which would erase the shadow inside the
    // rounded corners and leave square unshadowed patches. It is painted as
    // a ring in `finish_content_frame`, after the corner masks.
    let inset = Rect::from_min_max(
        pos2(outer.min.x + theme::CONTENT_MARGIN, outer.min.y + top),
        outer.max - vec2(theme::CONTENT_MARGIN, theme::CONTENT_MARGIN),
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

/// A soft glow around the content card, when asked for.
pub(crate) fn paint_card_halo(
    root: &Ui,
    painter: &egui::Painter,
    rect: Rect,
    radius: f32,
    palette: &Palette,
    top_glow: f32,
    halo: (crate::settings::HaloTint, f32),
) {
    let (tint, amount) = halo;
    // The square box has to be covered, not merely approached. Its furthest
    // point from the arc is radius * (sqrt(2) - 1) at each corner, and a
    // falloff that begins at the silhouette has decayed to a third of its
    // strength by the time it gets there. So the ring holds full strength out
    // to the wedge and only then fades.
    let wedge = radius * (std::f32::consts::SQRT_2 - 1.0);
    painter.add(glass::shadow_tinted(
        rect,
        radius,
        (wedge + radius * 0.42 + 6.0) * amount,
        // The held stretch covers the page's square corners, which sit a fixed
        // distance out — turning the halo up spreads it further, it does not
        // move what it is covering.
        wedge.min(wedge * amount),
        glass::Inner::Outside,
        |at| match tint {
            crate::settings::HaloTint::Accent => palette.accent,
            crate::settings::HaloTint::Chrome => chrome_color_at(root, at.y, palette, top_glow),
        },
    ));
}

/// A soft shadow hugging the content card.
///
/// `Outside`, unlike every other surface: what is inside this silhouette is
/// the web page, blitted pixel for pixel, and a feather row drawn over it
/// leaves a dark fringe around the whole page.
/// What a spread slider reads, calling out the shape both were drawn at before
/// either was adjustable rather than leaving it to be guessed from the handle.
pub(crate) fn spread_note(amount: f32) -> String {
    if (amount - 1.0).abs() < 0.02 {
        "Default.".to_owned()
    } else {
        format!("{:.0}% of default.", amount * 100.0)
    }
}

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
    /// An internal page, drawn by egui with rounded corners of its own.
    /// Masking here would lay a second tint over the first.
    AlreadyRounded,
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
    /// Strength of the lit band across the top of the window.
    pub top_glow: f32,
    pub border: bool,
    /// Spread, when the card casts a shadow.
    pub shadow: Option<f32>,
    /// Tint and amount, when the card carries a halo.
    pub halo: Option<(crate::settings::HaloTint, f32)>,
}

pub fn finish_content_frame(
    root: &Ui,
    content_rect: Rect,
    palette: &Palette,
    corners_style: Corners,
    frame: CardFrame,
) {
    let CardFrame {
        top_glow,
        border,
        shadow,
        halo,
    } = frame;
    let painter = root.ctx().layer_painter(egui::LayerId::background());
    // The oversized fans must only bleed inward over the blit — clip them so
    // they can't notch the halo painted around the card.
    let fan_painter = painter.with_clip_rect(content_rect);
    let radius = theme::CONTENT_RADIUS;
    let pad = 1.5;

    // One arc segment per physical pixel, so the facets are sub-pixel and the
    // curve cannot look polygonal on a Retina display.
    // Matched to the eraser's tessellation in backdrop.rs — where the two
    // polygons disagree, the mask's hard mesh edge overhangs the cut's
    // feathered one and the difference reads as facets along the curve.
    let segments = ((radius * root.pixels_per_point() * 2.0).ceil() as usize).clamp(16, 192);
    // (corner, arc center, start angle, outward direction)
    let corners = [
        (
            content_rect.left_top(),
            pos2(content_rect.left() + radius, content_rect.top() + radius),
            std::f32::consts::PI,
            vec2(-1.0, -1.0),
        ),
        (
            content_rect.right_top(),
            pos2(content_rect.right() - radius, content_rect.top() + radius),
            1.5 * std::f32::consts::PI,
            vec2(1.0, -1.0),
        ),
        (
            content_rect.right_bottom(),
            pos2(
                content_rect.right() - radius,
                content_rect.bottom() - radius,
            ),
            0.0,
            vec2(1.0, 1.0),
        ),
        (
            content_rect.left_bottom(),
            pos2(content_rect.left() + radius, content_rect.bottom() - radius),
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
    let tint = palette.chrome_tint();
    let chrome_at =
        |y: f32, opacity: f32, glow: f32| chrome_fill_at(root, y, palette, glow, opacity);

    // All four corner masks in ONE mesh: independent triangles each get their
    // own antialiased edges, and the AA seams between adjacent fan triangles
    // let the page underneath shine through as hairlines. A single mesh with
    // shared vertices has no interior seams.
    //
    // Meshes are not antialiased at all, though, so the arc where the mask
    // meets the page is a hard, stair-stepped edge. Each arc is therefore
    // retraced afterwards with a stroked line — which epaint *does*
    // antialias — in the same colour, feathering the boundary.
    if corners_style != Corners::AlreadyRounded {
        let mut mesh = Mesh::default();
        let mut arc_edges: Vec<(Vec<egui::Pos2>, Color32)> = Vec::new();
        for (corner, center, start_angle, outward) in corners {
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
            // The glow applies either way. Leaving it out was a workaround for
            // a bug in `chrome_fill_at`, not a decision: it blended with
            // `theme::mix`, which returns an opaque colour, so a translucent
            // glow came back nearly black. That is fixed, so the mask can carry
            // the same lit band the chrome beside it carries.
            let opacity = match corners_style {
                Corners::Cut => tint,
                _ => 1.0,
            };
            let glow = top_glow;
            mesh.colored_vertex(corner_out, chrome_at(corner_out.y, opacity, glow));
            for point in &arc {
                mesh.colored_vertex(*point, chrome_at(point.y, opacity, glow));
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

            arc_edges.push((true_arc, chrome_at(corner.y, opacity, glow)));
        }
        fan_painter.add(Shape::mesh(mesh));
        // Bridge the step where the mask meets the chrome.
        //
        // A mask has to be opaque: what it covers is the page's square corner,
        // blitted pixel for pixel. The chrome beside it is a thin tint over the
        // system's backdrop, and that backdrop is composited by the window
        // server outside this framebuffer — so we cannot read it, reproduce it,
        // or match it. Opaque chrome colour lands about five per cent off, and
        // the hard edge where the two meet reads as a jagged corner.
        //
        // It shows at the bottom and not the top because the toolbar above the
        // card is opaque, so up there the two happen to agree.
        //
        // So the mask fades out over a few points past the rect, where there is
        // no page to hide and nothing to lose. Only at the corners, tapering to
        // nothing where the arc meets the edge — a ring all the way round would
        // be the halo, which is a decision rather than a repair.
        // One physical pixel wide, so it feathers the mesh edge and no more.
        // At 1.4 points it was nearly three pixels, laid half over the page —
        // a chrome-coloured bite out of the page along the arcs but not along
        // the straight edges, which read as a chip just past each corner.
        let hairline = 1.0 / root.pixels_per_point();
        for (arc, colour) in arc_edges {
            fan_painter.add(Shape::line(arc, Stroke::new(hairline, colour)));
        }
    }

    // Drawn AFTER the corner masks: filling it beforehand works on internal
    // pages but not on web pages, where the blit wipes the square content rect
    // and leaves unshadowed patches in the corners.
    if let Some((tint, amount)) = halo {
        paint_card_halo(
            root,
            &painter,
            content_rect,
            radius,
            palette,
            top_glow,
            (tint, amount),
        );
    }
    if let Some(amount) = shadow {
        paint_card_shadow(&painter, content_rect, radius, palette, amount);
    }

    // Flat: a single accent-tinted edge all the way around the card — no
    // white rim light, no highlights. It also antialiases the corner masks,
    // whose mesh triangles have hard edges.
    if border {
        painter.rect_stroke(
            content_rect,
            CornerRadius::same(radius as u8),
            Stroke::new(1.2_f32, theme::mix(palette.border, palette.accent, 0.55)),
            StrokeKind::Middle,
        );
    }
}
