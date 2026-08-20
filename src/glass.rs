//! Liquid Glass material for the chrome: layered translucency, specular top
//! sheen, hairline rim light, and soft feathered depth shadow — parameterized
//! by the active palette so it composes on dark and light themes. (egui
//! cannot backdrop-blur, but over the chrome's solid gradient the layered
//! recipe reads the same.)

use egui::epaint::{Mesh, RectShape};
use egui::{
    Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2,
};

use crate::theme::{Material, Palette, Tier};

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
    /// Subject to the user's card-opacity setting.
    ///
    /// Off by default, and deliberately: "card" means the things you arrange
    /// and dismiss — the favourites and downloads cards, the shelf's widgets,
    /// the new tab page's — not every surface the material happens to draw.
    /// The chrome's own furniture, the settings sections and the tab rows
    /// are not cards and thinning them is not what anyone means by the
    /// setting. Opting in per surface keeps a new one from silently
    /// disappearing on someone.
    pub fades: bool,
}

impl Glass {
    /// A surface whose corners the material decides. Prefer this.
    pub fn tier(tier: Tier) -> Self {
        Self::of(Radius::Tier(tier))
    }

    /// A surface with a corner radius the caller worked out itself.
    pub fn new(radius: u8) -> Self {
        Self::of(Radius::Exact(radius))
    }

    fn of(radius: Radius) -> Self {
        Self {
            radius,
            strength: 1.0,
            glow: 0.0,
            tint: None,
            shadow: true,
            opaque: None,
            border: None,
            fades: false,
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

    /// Follow the user's card-opacity setting — see the field.
    ///
    /// Orthogonal to `opaque`: that one decides what the surface is painted
    /// over, this one decides whether the user is allowed to thin it.
    pub fn fades(mut self) -> Self {
        self.fades = true;
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
fn lift(material: &Material, dark: bool, strength: f32, fade: f32) -> f32 {
    (if dark {
        material.lift_dark
    } else {
        material.lift_light
    }) * strength
        * fade
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
pub fn shadow(rect: Rect, radius: f32, base: Color32, spread: f32, inner: Inner) -> Shape {
    let rows: Vec<_> = falloff(spread, inner)
        .into_iter()
        .map(|(offset, alpha)| (offset, base.gamma_multiply(alpha)))
        .collect();
    ring(&outline(rect, radius, segments(radius)), &rows)
}

/// A ring whose color is sampled per vertex rather than per row, for a surface
/// sitting on something that is itself a gradient.
///
/// `hold` keeps the ring at full strength for that many points past the
/// outline before the falloff starts.
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

pub fn shapes(rect: Rect, palette: &Palette, glass: Glass) -> Vec<Shape> {
    let mut out = Vec::new();
    let faded_away = glass.strength <= 0.0 || (glass.fades && palette.card_opacity <= 0.0);
    if faded_away && glass.glow <= 0.0 {
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
    let fade = if glass.fades {
        palette.card_opacity
    } else {
        1.0
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
    if glass.shadow && lift(material, dark, strength, fade) > 0.0 {
        let lift = lift(material, dark, strength, fade);
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
    let backdrop = material
        .frosts
        .then(|| palette.backdrop_under(rect))
        .flatten();

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
            material.frosted_fill + material.frosted_fill_strength * strength
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
    // it worth showing. A frosted one does, so the backing is what it would
    // paint over.
    if backdrop.is_none()
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
    let fill = fill.gamma_multiply(fade);
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
        let arrival = palette.backdrop.map_or(1.0, |backdrop| backdrop.alpha);
        out.push(
            RectShape::filled(quad, corner, Color32::WHITE.gamma_multiply(fade * arrival))
                .with_texture(texture, uv)
                .into(),
        );
    }
    out.push(
        RectShape::new(
            rect,
            corner,
            fill,
            Stroke::new(1.0_f32, hairline.gamma_multiply(fade)),
            StrokeKind::Inside,
        )
        .into(),
    );

    // Flat design: no specular sheen, no rim light, no bottom shade. Surfaces
    // are carried by fill + hairline border alone; depth comes from the
    // shadow and the accent glow, not from faked highlights.

    out
}
