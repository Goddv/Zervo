//! Liquid Glass material for the chrome: layered translucency, specular top
//! sheen, hairline rim light, and soft feathered depth shadow — parameterized
//! by the active palette so it composes on dark and light themes. (egui
//! cannot backdrop-blur, but over the chrome's solid gradient the layered
//! recipe reads the same.)

use egui::epaint::Mesh;
use egui::{
    Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2,
};

use crate::theme::Palette;

/// Expo-style ease-out for interaction animations (cubic out).
pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

pub struct Glass {
    pub radius: u8,
    /// Material prominence, 0..1 — drives fill, sheen, and shadow strength.
    pub strength: f32,
    /// Accent glow behind the element, 0..1 (active/focused elements).
    pub glow: f32,
    /// Optional core color override (e.g. accent-tinted active surfaces).
    pub tint: Option<Color32>,
    /// Drop shadow — disable for elements packed against neighbors (tab
    /// rows, grid tiles) where the shadow would bleed onto them.
    pub shadow: bool,
    /// Paint an opaque backing under the material, in this color.
    ///
    /// For anything floating over a live page: the material is translucent by
    /// design, which is right over the chrome and wrong over page text. The
    /// backing has to go *under* the drop shadow, which is why it belongs
    /// here rather than in a `rect_filled` at the call site — painting it
    /// first and the material over it puts the shadow inside the card.
    pub opaque: Option<Color32>,
    /// Hairline border color, overriding the default white translucency.
    pub border: Option<Color32>,
}

impl Glass {
    pub fn new(radius: u8) -> Self {
        Self {
            radius,
            strength: 1.0,
            glow: 0.0,
            tint: None,
            shadow: true,
            opaque: None,
            border: None,
        }
    }

    /// Back the material with an opaque fill, for cards floating over a page.
    pub fn opaque(mut self, color: Color32) -> Self {
        self.opaque = Some(color);
        self
    }

    /// Draw the hairline in a specific color rather than white translucency.
    pub fn border(mut self, color: Color32) -> Self {
        self.border = Some(color);
        self
    }

    pub fn no_shadow(mut self) -> Self {
        self.shadow = false;
        self
    }

    pub fn strength(mut self, strength: f32) -> Self {
        self.strength = strength;
        self
    }

    pub fn glow(mut self, glow: f32) -> Self {
        self.glow = glow;
        self
    }

    pub fn tint(mut self, tint: Color32) -> Self {
        self.tint = Some(tint);
        self
    }
}

pub fn paint(painter: &Painter, rect: Rect, palette: &Palette, glass: Glass) {
    painter.extend(shapes(rect, palette, glass));
}

/// Build the material as a shape list, so callers can also backfill a
/// placeholder (`painter.set`) once the covered rect is known.
/// The rounded-rect outline as (point, outward normal).
///
/// Sampling the four corner arcs and joining them also yields the straight
/// edges: an arc's endpoints sit exactly at the edge tangent points, and their
/// normals are the edge normals.
pub fn outline(rect: Rect, radius: f32, arc_segments: usize) -> Vec<(Pos2, Vec2)> {
    let radius = radius
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5)
        .max(0.0);
    let mut outline = Vec::with_capacity(4 * (arc_segments + 1));
    let corners = [
        (pos2(rect.max.x - radius, rect.max.y - radius), 0.0_f32),
        (pos2(rect.min.x + radius, rect.max.y - radius), 0.5),
        (pos2(rect.min.x + radius, rect.min.y + radius), 1.0),
        (pos2(rect.max.x - radius, rect.min.y + radius), 1.5),
    ];
    for (centre, quarter) in corners {
        for segment in 0..=arc_segments {
            let angle =
                (quarter + segment as f32 / arc_segments as f32 * 0.5) * std::f32::consts::PI;
            let normal = vec2(angle.cos(), angle.sin());
            outline.push((centre + normal * radius, normal));
        }
    }
    outline
}

/// Extrude `outline` outwards as a ring mesh, coloring each radial step with
/// `color_at(t)` where `t` runs 0 (at the outline) to 1 (at `spread`).
pub fn ring(
    outline: &[(Pos2, Vec2)],
    spread: f32,
    steps: usize,
    color_at: impl Fn(f32) -> Color32,
) -> Shape {
    let mut mesh = Mesh::default();
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let color = color_at(t);
        for (point, normal) in outline {
            mesh.colored_vertex(*point + *normal * (spread * t), color);
        }
    }
    let count = outline.len() as u32;
    for step in 0..steps as u32 {
        for index in 0..count {
            let next = (index + 1) % count;
            let (inner, outer) = (step * count, (step + 1) * count);
            mesh.add_triangle(inner + index, inner + next, outer + next);
            mesh.add_triangle(inner + index, outer + next, outer + index);
        }
    }
    Shape::mesh(mesh)
}

/// A soft shadow hugging a rounded rect, as one mesh.
///
/// Concentric strokes are the obvious approach and the wrong one: each stroke
/// has a hard edge, so a handful of them read as visible bands rather than a
/// shadow. epaint's own `blur_width` is no better here — it feathers the
/// shape's edge with a linear ramp centered on the outline, so half the blur
/// lands *inside* the card and the falloff outside is short and abrupt.
/// Interpolating vertex colors across a ring mesh gives a continuous falloff
/// that starts where the card ends.
pub fn shadow(rect: Rect, radius: f32, base: Color32, spread: f32) -> Shape {
    // One arc segment per couple of physical pixels of radius: enough that the
    // curve reads as a curve, few enough that a shadow behind every button is
    // not thousands of triangles.
    let segments = (radius * 2.0).ceil().clamp(10.0, 40.0) as usize;
    // Quadratic falloff, close to how a real penumbra reads.
    ring(&outline(rect, radius, segments), spread, 8, |t| {
        base.gamma_multiply((1.0 - t).powi(2))
    })
}

/// Composite `top` over `bottom`, both premultiplied — the same arithmetic the
/// GPU does when it draws one over the other.
///
/// Stacking translucent rounded rects at the same radius is what this avoids:
/// their antialiased corner pixels composite once per layer, so a corner ends
/// up darker and harder than the straight edges beside it. Flattening the
/// layers into one fill leaves a single antialiased edge.
fn over(top: Color32, bottom: Color32) -> Color32 {
    let inv = 1.0 - top.a() as f32 / 255.0;
    let mix = |t: u8, b: u8| (t as f32 + b as f32 * inv).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_premultiplied(
        mix(top.r(), bottom.r()),
        mix(top.g(), bottom.g()),
        mix(top.b(), bottom.b()),
        mix(top.a(), bottom.a()),
    )
}

pub fn shapes(rect: Rect, palette: &Palette, glass: Glass) -> Vec<Shape> {
    let mut out = Vec::new();
    if glass.strength <= 0.0 && glass.glow <= 0.0 {
        return out;
    }
    let radius_px = f32::from(glass.radius);
    let corner = CornerRadius::same(glass.radius);
    let dark = palette.dark;
    let strength = glass.strength.clamp(0.0, 1.0);

    // Accent glow halo behind active/focused elements — a falloff rather than
    // a feathered edge, so it reads as light rather than as a colored band.
    if glass.glow > 0.0 {
        out.push(shadow(
            rect,
            radius_px,
            palette.accent.gamma_multiply(0.32 * glass.glow),
            8.0,
        ));
    }

    // Drop shadow for lift, offset a touch downward.
    if glass.shadow {
        let lift = if dark { 0.55 } else { 0.8 } * strength;
        out.push(shadow(
            rect.translate(vec2(0.0, 1.5)),
            radius_px,
            palette.shadow.gamma_multiply(lift),
            4.0 + radius_px * 0.45,
        ));
    }

    // Translucent core over the chrome gradient, plus the glass wash — white
    // translucency per the glassmorphism recipe, with the light theme leaning
    // on the darker core for contrast and using white for highlights only.
    // Flattened into one fill, and onto the opaque backing when there is one.
    let core = glass
        .tint
        .unwrap_or(palette.surface)
        .gamma_multiply(0.55 + 0.4 * strength);
    let wash = Color32::from_white_alpha((if dark { 9.0 } else { 24.0 } * strength) as u8);
    let mut fill = over(wash, core);
    if let Some(backing) = glass.opaque {
        fill = over(fill, backing);
    }
    out.push(Shape::rect_filled(rect, corner, fill));

    // Flat design: no specular sheen, no rim light, no bottom shade. Surfaces
    // are carried by fill + hairline border alone; depth comes from the
    // shadow and the accent glow, not from faked highlights.

    // Hairline border. One stroke, never two: an inner and an outer stroke at
    // the same radius put two antialiased curves a pixel apart, which is the
    // faint second corner you can see on a card that has both.
    let hairline = glass.border.unwrap_or_else(|| {
        let alpha = if dark { 26.0 } else { 120.0 } * strength.max(0.6);
        Color32::from_white_alpha(alpha as u8)
    });
    out.push(Shape::rect_stroke(
        rect,
        corner,
        Stroke::new(1.0_f32, hairline),
        StrokeKind::Inside,
    ));

    out
}
