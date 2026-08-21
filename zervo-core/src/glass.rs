//! Liquid Glass material for the chrome: layered translucency, specular top
//! sheen, hairline rim light, and soft feathered depth shadow — parameterized
//! by the active palette so it composes on dark and light themes. (egui
//! cannot backdrop-blur, but over the chrome's solid gradient the layered
//! recipe reads the same.)

use egui::epaint::{Mesh, RectShape};
use egui::{
    Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2,
};

use crate::theme::{Material, Palette, Surface, Tier};

/// Expo-style ease-out for interaction animations (cubic out).
pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// How round a surface's corners are: a tier the material decides, or a number
/// the caller worked out for itself.
///
/// Named for what it is rather than where it is — a corner is a place, this is
/// a length, and `CornerRadius` is already egui's word for the four of them.
///
/// Naming the tier is the one to reach for. A number is right only where it is
/// derived from something else — a pill whose corners are half its height —
/// and a material cannot know that.
#[derive(Clone, Copy)]
pub enum Radius {
    Tier(Tier),
    Exact(u8),
}

impl Radius {
    pub fn resolve(self, material: &Material) -> u8 {
        match self {
            Radius::Tier(tier) => material.radius.of(tier),
            Radius::Exact(radius) => radius,
        }
    }
}

pub struct Glass {
    /// Which class of surface this is. The material decides what that class is
    /// made of; the call site only says which one it is.
    pub surface: Surface,
    pub radius: Radius,
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
    /// A surface whose corners the material decides. Prefer this.
    pub fn tier(tier: Tier) -> Self {
        Self::at(Radius::Tier(tier))
    }

    /// A surface with a corner radius the caller worked out itself.
    pub fn new(radius: u8) -> Self {
        Self::at(Radius::Exact(radius))
    }

    /// A surface of this class, with the corners its class implies. The way to
    /// spell a menu, a card or a text field.
    pub fn of(surface: Surface) -> Self {
        let tier = match surface {
            Surface::Card => Tier::Card,
            Surface::Menu => Tier::Card,
            Surface::Input => Tier::Pill,
        };
        Self {
            surface,
            ..Self::at(Radius::Tier(tier))
        }
    }

    /// Override the corners the class implied, with a tier.
    pub fn radius(mut self, tier: Tier) -> Self {
        self.radius = Radius::Tier(tier);
        self
    }

    /// Override them with a number the caller worked out — a pill whose
    /// corners are half its height, and little else.
    pub fn radius_exact(mut self, radius: u8) -> Self {
        self.radius = Radius::Exact(radius);
        self
    }

    fn at(radius: Radius) -> Self {
        Self {
            surface: Surface::Card,
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

/// How far outside its own rect the material paints, for a given corner
/// radius: the drop shadow's spread, or the accent glow's, whichever reaches
/// further. Anything that clips a glass surface has to leave this much room —
/// clip tight to the rect and the shadow is scissored off with a hard
/// rectangle, which leaves a square-cornered wedge of it outside every rounded
/// corner.
pub fn room(radius: u8) -> f32 {
    (4.0 + f32::from(radius) * 0.45).max(8.0) + 1.0
}

/// One physical pixel on a 2x display: the width the ring fades in over, so
/// that its inner boundary is not a hard edge. See `shadow`.
const FEATHER: f32 = 0.5;

/// Extrude `outline` as a ring mesh. Each row is a radial offset from the
/// outline — negative is inward — and the color at that offset; the mesh
/// interpolates between consecutive rows.
pub fn ring(outline: &[(Pos2, Vec2)], rows: &[(f32, Color32)]) -> Shape {
    let mut mesh = Mesh::default();
    for (offset, color) in rows {
        for (point, normal) in outline {
            mesh.colored_vertex(*point + *normal * *offset, *color);
        }
    }
    stitch(&mut mesh, outline.len() as u32, rows.len());
    Shape::mesh(mesh)
}

/// Join consecutive rows of `count` vertices each into quads.
fn stitch(mesh: &mut Mesh, count: u32, rows: usize) {
    for row in 0..rows.saturating_sub(1) as u32 {
        for index in 0..count {
            let next = (index + 1) % count;
            let (inner, outer) = (row * count, (row + 1) * count);
            mesh.add_triangle(inner + index, inner + next, outer + next);
            mesh.add_triangle(inner + index, outer + next, outer + index);
        }
    }
}

/// The drop shadow's strength, which the card-opacity setting thins along
/// with everything else the material paints.
/// How heavy a surface's shadow is. Not scaled by translucency: the shadow and
/// the hairline are what say where a surface ends, and a sheer card with
/// neither is not a sheer card, it is a smudge.
fn lift(material: &Material, dark: bool, strength: f32) -> f32 {
    (if dark {
        material.lift_dark
    } else {
        material.lift_light
    }) * strength
}

/// Where a ring's feathered inner edge sits.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Inner {
    /// Half a point inside the silhouette, where the caller's own fill covers
    /// it. The right choice whenever the caller paints the surface itself.
    Under,
    /// Flush with the silhouette, so the mesh never reaches inside it. For a
    /// surface whose interior belongs to something else — the content card,
    /// where the web page owns every pixel inside the rounded rect and a row
    /// drawn over it fringes the page all the way round.
    Outside,
}

/// The radial offsets and alphas of a quadratic falloff, feathered at the
/// inner end so the mesh has no hard boundary. See `shadow`.
fn falloff(spread: f32, inner: Inner) -> Vec<(f32, f32)> {
    const STEPS: usize = 8;
    let start = match inner {
        Inner::Under => -FEATHER,
        Inner::Outside => 0.0,
    };
    let peak = start + FEATHER;
    let mut rows = Vec::with_capacity(STEPS + 2);
    rows.push((start, 0.0));
    for step in 0..=STEPS {
        let t = step as f32 / STEPS as f32;
        // Quadratic falloff, close to how a real penumbra reads.
        rows.push((peak + (spread - peak).max(0.0) * t, (1.0 - t).powi(2)));
    }
    rows
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
///
/// Both of the mesh's silhouettes are transparent, and that is the point. A
/// mesh gets no antialiasing from the tessellator, so a boundary drawn at full
/// strength is a hard edge, and a hard edge along a curve is a staircase —
/// which is what a second, jagged outline around a card's corner turned out to
/// be. The outer boundary has faded to nothing anyway; `inner` says where to
/// put the row the inner one fades from.
pub fn shadow_tinted(
    rect: Rect,
    radius: f32,
    spread: f32,
    hold: f32,
    inner: Inner,
    color_at: impl Fn(Pos2) -> Color32,
) -> Shape {
    let outline = outline(rect, radius, segments(radius));
    let mut rows = falloff((spread - hold).max(0.0), inner);
    // Push everything from the peak outward past the hold, and repeat the peak
    // at the outline so the held stretch is flat rather than sloping.
    let peak = match inner {
        Inner::Under => 0.0,
        Inner::Outside => FEATHER,
    };
    for row in &mut rows {
        if row.0 >= peak {
            row.0 += hold;
        }
    }
    rows.insert(
        rows.iter()
            .position(|row| row.0 >= peak + hold)
            .unwrap_or(1),
        (peak, 1.0),
    );

    let mut mesh = Mesh::default();
    for (offset, alpha) in &rows {
        for (point, normal) in &outline {
            let at = *point + *normal * *offset;
            mesh.colored_vertex(at, color_at(at).gamma_multiply(*alpha));
        }
    }
    stitch(&mut mesh, outline.len() as u32, rows.len());
    Shape::mesh(mesh)
}

pub fn shadow(rect: Rect, radius: f32, base: Color32, spread: f32, inner: Inner) -> Shape {
    let rows: Vec<_> = falloff(spread, inner)
        .into_iter()
        .map(|(offset, alpha)| (offset, base.gamma_multiply(alpha)))
        .collect();
    ring(&outline(rect, radius, segments(radius)), &rows)
}

/// One arc segment per couple of physical pixels of radius: enough that the
/// curve reads as a curve, few enough that a shadow behind every button is not
/// thousands of triangles.
fn segments(radius: f32) -> usize {
    (radius * 2.0).ceil().clamp(10.0, 40.0) as usize
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

/// The core's own alpha, before the class tint scales the finished surface.
fn core_fill(material: &crate::theme::Material, strength: f32) -> f32 {
    material.frosted_fill + material.frosted_fill_strength * strength
}

pub fn shapes(rect: Rect, palette: &Palette, glass: Glass) -> Vec<Shape> {
    let mut out = Vec::new();
    if glass.strength <= 0.0 && glass.glow <= 0.0 {
        return out;
    }
    let material = &palette.material;
    // The tier a call site asked for, resolved by the material. An explicit
    // radius still overrides it, for the handful of places where the number is
    // derived from something else — a pill whose corners are half its height.
    let radius = glass.radius.resolve(material);
    let radius_px = f32::from(radius);
    let corner = CornerRadius::same(radius);
    let dark = palette.dark;
    let strength = glass.strength.clamp(0.0, 1.0);
    // How much of this surface's own material survives, per the reader's
    // setting — unless the material has opted out of having one.
    // What is behind this surface, blurred, if anything is.
    let backdrop = (material.frosts && palette.translucency == crate::theme::Translucency::Frosted)
        .then(|| palette.backdrop_under(rect))
        .flatten();
    let sheer = if material.translucency {
        palette.tint_for(glass.surface)
    } else {
        1.0
    };
    // What the reader sees is the core's own alpha times this, so this is where
    // holding the theme has to happen. Thickening the core instead cannot work:
    // it saturates at one, and the class tint then hands two thirds of the page
    // straight back — which is how a dark-mode card over a bright wallpaper
    // stayed pale, and took its text with it.
    let sheer = if backdrop.is_some() {
        let own = core_fill(material, strength);
        (palette.tint_over(rect, own * sheer) / own.max(0.01)).min(1.0)
    } else {
        sheer
    };

    // Accent glow halo behind active/focused elements — a falloff rather than
    // a feathered edge, so it reads as light rather than as a colored band.
    if glass.glow > 0.0 {
        out.push(shadow(
            rect,
            radius_px,
            palette.accent.gamma_multiply(material.glow * glass.glow),
            material.glow_reach,
            Inner::Under,
        ));
    }

    // Drop shadow for lift. Concentric with the card, not offset downward: an
    // offset ring starts outside the silhouette on the far side, which leaves
    // a bright gap between the card's edge and its shadow and turns the
    // shadow's leading edge into a second outline around the corner.
    if glass.shadow && lift(material, dark, strength) > 0.0 {
        let lift = lift(material, dark, strength);
        out.push(shadow(
            rect,
            radius_px,
            palette.shadow.gamma_multiply(lift),
            material.shadow_reach + radius_px * material.shadow_reach_per_radius,
            Inner::Under,
        ));
    }

    // What is behind this surface, blurred, if anything is. The palette
    // carries it, so a caller that has put a picture behind the chrome gets
    // every card, pill and menu on top of it frosted without saying anything
    // at the call site — and a change to the recipe below reaches all of them
    // at once, which is the point of having a material rather than a habit.
    //
    // Solid takes its name seriously: an opaque surface shows nothing of what
    // is behind it, so it does not ask. The callers that supply the picture
    // skip the work of making one under Solid, but the rule belongs here — it
    // is a property of the material, not something two call sites have to
    // remember in step.

    // Translucent core over whatever is behind, plus the glass wash — white
    // translucency per the glassmorphism recipe, with the light theme leaning
    // on the darker core for contrast and using white for highlights only.
    //
    // Over a blurred backdrop the core is a tint *on* the blur rather than a
    // substitute for it, so it is thinner: a surface opaque enough to be a
    // card in its own right hides the frost it is sitting on, and then this is
    // an ordinary card with an expensive texture behind it.
    let core = glass
        .tint
        .unwrap_or(palette.surface)
        .gamma_multiply(if backdrop.is_some() {
            core_fill(material, strength)
        } else {
            material.fill + material.fill_strength * strength
        });
    let sheen = if dark {
        material.sheen_dark
    } else {
        material.sheen_light
    };
    let wash = Color32::from_white_alpha((sheen * strength) as u8);
    let mut fill = over(wash, core);
    // An opaque backing is what a surface asks for when it has nothing behind
    // it worth showing. Two things can make that untrue. A frosted one has its
    // own blurred backdrop, which is the thing worth showing. And once the
    // reader has asked for glass at all, an opaque backing is the one way a
    // surface can refuse to give it to them — a card that paints itself solid
    // before the material ever sees it is not translucent at any setting. So
    // the backing is honoured only when nothing is meant to come through.
    if backdrop.is_none()
        && palette.translucency == crate::theme::Translucency::Solid
        && let Some(backing) = glass.opaque
    {
        fill = over(fill, backing);
    }
    // Faded *after* the backing is composited in, not before. Fading the core
    // and the wash first and then compositing leaves the backing at full
    // strength — the card would lose its tint and its hairline and become a
    // flat opaque slab, right up to the point where it vanished. Scaling the
    // finished premultiplied colour thins the whole surface continuously,
    // backing included.
    let fill = fill.gamma_multiply(sheer);
    // Fill and hairline as one shape, not two. Two rounded rects at the same
    // radius antialias the same curve twice over, so the corner composites
    // heavier than the straight edges beside it — one RectShape tessellates
    // both together and antialiases the silhouette once.
    let hairline = glass.border.unwrap_or_else(|| {
        let edge = if dark {
            material.edge_dark
        } else {
            material.edge_light
        };
        Color32::from_white_alpha((edge * strength.max(0.6)) as u8)
    });
    // The blur goes under the fill, inside the same silhouette, and fades with
    // it: at zero card opacity a surface has to disappear completely, blur
    // included, or the setting stops meaning anything. It also fades with the
    // backdrop's own arrival, so a card is never sitting on a solid blur of a
    // picture that has not finished appearing.
    //
    // `quad` is the part of the surface the backdrop actually reaches, which
    // is the whole of it except where the surface hangs off the edge of the
    // picture. It keeps the surface's corner radius: on the three sides that
    // are not cut it is the surface's own edge, and the cut side is inside the
    // surface where a rounded corner costs nothing to be slightly wrong.
    if let Some((texture, quad, uv)) = backdrop {
        // At full strength, never thinned by the setting. This layer is the
        // card's *vibrancy*, and the window's own vibrancy is not thinned
        // either — macOS blurs the desktop behind the chrome at full strength
        // whatever the chrome's opacity is, and the opacity paints a tint over
        // the top of it. Scaling this too made a card mix its blurred backdrop
        // with the sharp one underneath, which is a smear rather than glass,
        // and is why the cards read as a different material from the window
        // holding them.
        let arrival = palette.backdrop.map_or(1.0, |backdrop| backdrop.alpha);
        out.push(
            RectShape::filled(quad, corner, Color32::WHITE.gamma_multiply(arrival))
                .with_texture(texture, uv)
                .into(),
        );
    }
    out.push(
        RectShape::new(
            rect,
            corner,
            fill,
            Stroke::new(1.0_f32, hairline),
            StrokeKind::Inside,
        )
        .into(),
    );

    // Flat design: no specular sheen, no rim light, no bottom shade. Surfaces
    // are carried by fill + hairline border alone; depth comes from the
    // shadow and the accent glow, not from faked highlights.

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{
        AccentColor, Backdrop, LUMA_CELLS, Surface, ThemeMode, Translucency, resolve,
    };
    use egui::{Rect, TextureId, pos2};

    /// A page filling 100,100 → 900,700, as the content rect does.
    fn frosting_palette() -> Palette {
        let mut palette = resolve(ThemeMode::Dark, true, AccentColor::Lavender);
        palette.translucency = Translucency::Frosted;
        palette.backdrop = Some(Backdrop {
            texture: TextureId::default(),
            rect: Rect::from_min_max(pos2(100.0, 100.0), pos2(900.0, 700.0)),
            uv: Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            luma: [128; LUMA_CELLS * LUMA_CELLS],
            reach: 0.0,
            alpha: 1.0,
        });
        palette
    }

    /// Every shape carrying the blurred copy, and the rectangle it covers.
    fn frosted_area(shapes: &[Shape]) -> Vec<Rect> {
        shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Rect(rect) if rect.brush.is_some() => Some(rect.rect),
                _ => None,
            })
            .collect()
    }

    /// The seam in the screenshot: a downloads card anchored under a toolbar
    /// button overhangs the top of the page by a few points, and used to draw
    /// as glass below that line and flat above it.
    #[test]
    fn a_card_overhanging_the_page_is_frosted_edge_to_edge() {
        let palette = frosting_palette();
        // Sitting six points above the page's top edge, like a hover card
        // hanging off the toolbar.
        let card = Rect::from_min_max(pos2(300.0, 94.0), pos2(660.0, 300.0));
        let frosted = frosted_area(&shapes(card, &palette, Glass::of(Surface::Menu)));
        assert_eq!(
            frosted,
            vec![card],
            "one frosted rectangle, covering the whole card"
        );
    }

    /// And the case that must keep working: nothing to frost against means no
    /// frost, rather than a smear of the page's edge under a menu that is
    /// nowhere near it.
    #[test]
    fn a_card_away_from_the_page_is_not_frosted() {
        let palette = frosting_palette();
        let card = Rect::from_min_max(pos2(0.0, 0.0), pos2(90.0, 90.0));
        assert!(frosted_area(&shapes(card, &palette, Glass::of(Surface::Menu))).is_empty());
    }

    /// The strongest fill any shape covering the card carries.
    fn heaviest_fill(shapes: &[Shape], card: Rect) -> u8 {
        shapes
            .iter()
            .filter_map(|shape| match shape {
                // Exactly the card: the shadow is drawn on a larger rectangle
                // and would otherwise answer for it.
                Shape::Rect(rect) if rect.brush.is_none() && rect.rect == card => {
                    Some(rect.fill.a())
                },
                _ => None,
            })
            .max()
            .unwrap_or(0)
    }

    /// The bug this is here to stop coming back: the tint is applied twice —
    /// once as the core's own alpha, and again as the class tint scaling the
    /// finished fill. Thickening only the first left a dark-mode card over a
    /// bright wallpaper at a quarter of its own colour, so the beach came
    /// through it and its pale text went with it.
    #[test]
    fn a_card_gets_heavier_over_a_page_that_would_overrule_it() {
        let card = Rect::from_min_max(pos2(300.0, 250.0), pos2(660.0, 450.0));
        let weight = |luma: u8| {
            let mut palette = frosting_palette();
            palette.backdrop.as_mut().unwrap().luma = [luma; LUMA_CELLS * LUMA_CELLS];
            heaviest_fill(
                &shapes(card, &palette, Glass::tier(crate::theme::Tier::Card)),
                card,
            )
        };
        let (agreeable, opposite) = (weight(10), weight(255));
        assert!(
            opposite > agreeable,
            "a dark card over a white page has to be heavier than over a black one: \
             {opposite} vs {agreeable}"
        );
        // And heavier by enough to matter — the first attempt at this raised
        // the core from 0.58 to 0.73, which the class tint then cut back to a
        // quarter and nobody could see.
        assert!(
            f32::from(opposite) > f32::from(agreeable) * 1.5,
            "barely heavier is the same bug: {opposite} vs {agreeable}"
        );
    }

    /// Solid is opaque, whatever is behind the window.
    #[test]
    fn a_solid_card_is_not_frosted() {
        let mut palette = frosting_palette();
        palette.translucency = Translucency::Solid;
        let card = Rect::from_min_max(pos2(300.0, 200.0), pos2(660.0, 400.0));
        let solid = shapes(card, &palette, Glass::of(Surface::Menu).opaque(palette.bg));
        assert!(frosted_area(&solid).is_empty());
    }
}
