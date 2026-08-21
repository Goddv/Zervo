//! Theme system: dark and light palettes with an Auto mode that follows the
//! OS appearance (which on macOS follows the day/night cycle when the system
//! is set to Auto). Icon and text colors always derive from the active
//! palette, so glyphs compose correctly on both light and dark chrome.

use egui::{Color32, Context, CornerRadius, Margin, Rect, Shadow, Stroke, TextureId, Vec2, pos2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    /// Follow the system appearance.
    Auto,
    Light,
    Dark,
}

/// User-selectable accent color presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccentColor {
    /// A colour the reader mixed themselves. First in the row, and the only
    /// one that opens a picker rather than simply being applied.
    ///
    /// Kept as it was chosen in both themes. The presets carry a pair each,
    /// tuned so they have contrast on dark chrome and on light; second-guessing
    /// somebody's own colour by lightening it in one theme would be worse than
    /// showing them what they picked.
    Custom(u8, u8, u8),
    Lavender,
    Sky,
    Mint,
    Amber,
    Rose,
    Coral,
    Teal,
    Violet,
    Lime,
    Graphite,
}

impl AccentColor {
    /// The fixed ones, in the order they are offered. `Custom` is not here:
    /// it has no colour until somebody picks one, so the settings page draws
    /// it from whatever they last chose.
    pub const PRESETS: [AccentColor; 10] = [
        AccentColor::Lavender,
        AccentColor::Sky,
        AccentColor::Mint,
        AccentColor::Amber,
        AccentColor::Rose,
        AccentColor::Coral,
        AccentColor::Teal,
        AccentColor::Violet,
        AccentColor::Lime,
        AccentColor::Graphite,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            AccentColor::Custom(..) => "Custom",
            AccentColor::Lavender => "Lavender",
            AccentColor::Sky => "Sky",
            AccentColor::Mint => "Mint",
            AccentColor::Amber => "Amber",
            AccentColor::Rose => "Rose",
            AccentColor::Coral => "Coral",
            AccentColor::Teal => "Teal",
            AccentColor::Violet => "Violet",
            AccentColor::Lime => "Lime",
            AccentColor::Graphite => "Graphite",
        }
    }

    /// The three bytes a custom accent is stored as, or the preset's own
    /// colour in the current theme — what the picker opens on.
    pub fn rgb(&self, dark: bool) -> [u8; 3] {
        let color = self.color(dark);
        [color.r(), color.g(), color.b()]
    }

    /// The accent color, tuned per theme so it has contrast on both.
    pub fn color(&self, dark: bool) -> Color32 {
        match (self, dark) {
            (AccentColor::Lavender, true) => Color32::from_rgb(167, 139, 250),
            (AccentColor::Lavender, false) => Color32::from_rgb(114, 84, 220),
            (AccentColor::Sky, true) => Color32::from_rgb(125, 196, 240),
            (AccentColor::Sky, false) => Color32::from_rgb(38, 122, 190),
            (AccentColor::Mint, true) => Color32::from_rgb(122, 214, 160),
            (AccentColor::Mint, false) => Color32::from_rgb(28, 138, 82),
            (AccentColor::Amber, true) => Color32::from_rgb(240, 187, 120),
            (AccentColor::Amber, false) => Color32::from_rgb(178, 108, 24),
            (AccentColor::Rose, true) => Color32::from_rgb(240, 148, 170),
            (AccentColor::Rose, false) => Color32::from_rgb(188, 62, 96),
            (AccentColor::Coral, true) => Color32::from_rgb(244, 152, 128),
            (AccentColor::Coral, false) => Color32::from_rgb(196, 74, 46),
            (AccentColor::Teal, true) => Color32::from_rgb(112, 204, 202),
            (AccentColor::Teal, false) => Color32::from_rgb(20, 124, 124),
            (AccentColor::Violet, true) => Color32::from_rgb(202, 142, 236),
            (AccentColor::Violet, false) => Color32::from_rgb(138, 60, 190),
            (AccentColor::Lime, true) => Color32::from_rgb(190, 214, 118),
            (AccentColor::Lime, false) => Color32::from_rgb(108, 138, 30),
            (AccentColor::Graphite, true) => Color32::from_rgb(172, 178, 188),
            (AccentColor::Graphite, false) => Color32::from_rgb(84, 92, 104),
            (AccentColor::Custom(red, green, blue), _) => Color32::from_rgb(*red, *green, *blue),
        }
    }
}

/// Linear per-channel mix of two opaque colors.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let inv = 1.0 - t;
    Color32::from_rgb(
        (a.r() as f32 * inv + b.r() as f32 * t) as u8,
        (a.g() as f32 * inv + b.g() as f32 * t) as u8,
        (a.b() as f32 * inv + b.b() as f32 * t) as u8,
    )
}

/// A picture behind the chrome, already blurred, for glass surfaces to frost
/// themselves against.
///
/// egui cannot blur what is behind a shape while it draws it, and nothing here
/// needs it to. The only thing ever behind the chrome is a wallpaper, which is
/// a still image — so it is blurred once, when it is decoded, and the material
/// samples that blurred copy through the same mapping the sharp one is drawn
/// with. What comes out is a real backdrop blur rather than an impression of
/// one, and it costs nothing per frame.
#[derive(Clone, Copy)]
pub struct Backdrop {
    /// A blurred copy of the picture. Small: it is blurred, so there is no
    /// detail left in it to be worth storing at size.
    pub texture: TextureId,
    /// Where the sharp picture is drawn, in screen points.
    pub rect: Rect,
    /// The part of the picture that `rect` shows — the same window the sharp
    /// one uses, so the blur underneath a card lines up with the photograph
    /// beside it.
    pub uv: Rect,
    /// How far the picture itself has arrived, 0..=1.
    ///
    /// A wallpaper fades in. While it is doing so the blur under a card has to
    /// fade with it, or a card sits on an opaque frost while the photograph
    /// beside it is still half transparent.
    pub alpha: f32,
}

/// How much of what is behind a surface comes through it.
///
/// Three steps rather than a slider. A surface's job is to hold text up, and
/// most of the range between "a card" and "not there" is a surface that has
/// stopped doing that — so the choice offered is between three that all work
/// rather than a hundred that mostly do not.
///
/// Every material honours this unless it says otherwise with
/// `Material::translucency`, so it is one setting across every theme rather
/// than something each one reinvents.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Translucency {
    /// What Zervo has always looked like.
    Solid,
    Frosted,
    Sheer,
}

impl Translucency {
    pub const ALL: [Translucency; 3] = [
        Translucency::Solid,
        Translucency::Frosted,
        Translucency::Sheer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Translucency::Solid => "Solid",
            Translucency::Frosted => "Frosted",
            Translucency::Sheer => "Sheer",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Translucency::Solid => {
                "Surfaces are their own colour. The most readable, and the least glass."
            },
            Translucency::Frosted => {
                "What is behind shows through — the page, the chrome, the wallpaper's blur."
            },
            Translucency::Sheer => "As far as glass goes before it stops holding text up.",
        }
    }

    /// How much of a surface's own material survives, 0..=1.
    ///
    /// Only the fill is scaled. The hairline and the shadow keep their
    /// strength at every step, because they are what say where a surface ends
    /// — a sheer card with no edge is not a sheer card, it is a smudge.
    pub fn fill(self) -> f32 {
        match self {
            Translucency::Solid => 1.0,
            Translucency::Frosted => 0.72,
            Translucency::Sheer => 0.48,
        }
    }
}

/// How far a material blurs what is behind it.
///
/// Only means anything for a material that frosts at all. A flat toolkit
/// material, or one built on Apple's Liquid Glass — which refracts rather than
/// blurs — sets `Material::frosts` false and never reads this.
///
/// Three steps, like [`Translucency`], and for the same reason: the useful
/// range is narrow. Below a certain radius a blur is just a smeared photograph
/// and text sits on top of the smear; above it, every wallpaper looks the
/// same and there was no point fetching one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Blur {
    Light,
    Medium,
    Deep,
}

impl Blur {
    pub const ALL: [Blur; 3] = [Blur::Light, Blur::Medium, Blur::Deep];

    pub fn label(self) -> &'static str {
        match self {
            Blur::Light => "Light",
            Blur::Medium => "Medium",
            Blur::Deep => "Deep",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Blur::Light => "The picture is still recognisable through a card.",
            Blur::Medium => "Shape and colour come through; detail does not.",
            Blur::Deep => "Colour only — the wallpaper as a wash behind the page.",
        }
    }

    /// Multiplies the material's own blur radius, so a material that blurs
    /// gently and one that blurs hard both keep their character across the
    /// three steps rather than being flattened onto the same numbers.
    pub fn scale(self) -> f32 {
        match self {
            Blur::Light => 0.45,
            Blur::Medium => 1.0,
            Blur::Deep => 2.1,
        }
    }
}

/// The corner-radius tier every rounded thing in Zervo picks from.
///
/// Sizes, not numbers. A card is a card whatever the material thinks a card's
/// corners should look like, so a call site names the tier and the material
/// decides — which is what makes a square-cornered Fluent theme or a heavily
/// rounded Material 3 one a change to six values rather than to eighty call
/// sites.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    /// Progress tracks, the bar's grabber — things a couple of points tall.
    Hairline,
    /// Small hit targets, up to about twenty-four points.
    Control,
    /// Rows, ghost-button hovers, tab rows. The commonest by far.
    Row,
    /// Ordinary cards: settings sections, widgets, menus, tiles.
    Card,
    /// Page-sized surfaces, and the floating content card.
    Panel,
    /// Search boxes and anything else that reads as a pill.
    Pill,
}

/// What each tier comes to, in points.
#[derive(Clone, Copy)]
pub struct Radii {
    pub hairline: u8,
    pub control: u8,
    pub row: u8,
    pub card: u8,
    pub panel: u8,
    pub pill: u8,
}

impl Radii {
    pub const fn of(&self, tier: Tier) -> u8 {
        match tier {
            Tier::Hairline => self.hairline,
            Tier::Control => self.control,
            Tier::Row => self.row,
            Tier::Card => self.card,
            Tier::Panel => self.panel,
            Tier::Pill => self.pill,
        }
    }
}

/// What Zervo's surfaces are made of.
///
/// A theme is a palette and a material: the palette says what colour things
/// are, the material says how they are built. Every surface the chrome draws
/// asks the material rather than deciding for itself, so a new material is a
/// new look for the whole application and nothing else has to change — the
/// same cards, the same rows, the same dialogs, built to a different recipe.
///
/// This is the seam a Fluent, GTK, Qt or Material 3 theme would be written
/// against. None of the numbers below are glass-specific: a material with
/// `frosts: false`, no sheen, a heavier edge and square corners is a flat
/// desktop toolkit, and the code that draws the chrome does not change.
#[derive(Clone, Copy)]
pub struct Material {
    pub name: &'static str,

    // ── What a surface is filled with
    /// How much of a surface's own colour it carries at rest, and how much
    /// more at full strength.
    pub fill: f32,
    pub fill_strength: f32,
    /// Whether this material honours the reader's translucency setting.
    ///
    /// True for anything glassy. A material for a toolkit that has no notion
    /// of translucency — a flat GTK or Fluent one — sets this false and its
    /// surfaces stay exactly as it drew them, whatever the setting says.
    pub translucency: bool,
    /// The same pair over a blurred backdrop, where the fill is a tint on the
    /// blur rather than a substitute for it.
    pub frosted_fill: f32,
    pub frosted_fill_strength: f32,
    /// The white sheen laid over the fill, out of 255, in the dark theme and
    /// the light one. Zero for a material that does not do glassiness.
    pub sheen_dark: f32,
    pub sheen_light: f32,

    // ── Its edge
    /// The hairline along a surface's edge, out of 255, dark and light.
    pub edge_dark: f32,
    pub edge_light: f32,

    // ── How far off the page it sits
    pub lift_dark: f32,
    pub lift_light: f32,
    /// How far a shadow reaches: a fixed part, plus a share of the radius.
    pub shadow_reach: f32,
    pub shadow_reach_per_radius: f32,
    /// The accent halo behind a focused surface, and how far it reaches.
    pub glow: f32,
    pub glow_reach: f32,

    // ── Whether it frosts what is behind it
    pub frosts: bool,
    /// How far a backdrop is blurred before anything is frosted against it,
    /// in pixels of the small copy that is kept for the purpose.
    pub blur: f32,

    // ── Shape and metrics
    pub radius: Radii,
    /// The height of a settings row, a menu row, a list row.
    pub row_height: f32,
    /// Padding inside a button, and the gap between stacked controls.
    pub control_padding: Vec2,
    pub item_spacing: Vec2,
    /// How long a hover, a selection or a fade takes to settle.
    pub animation: f32,
}

impl Material {
    /// Zervo's own: layered translucency, a hairline edge, a soft shadow, and
    /// a real blur of whatever is behind.
    pub const GLASS: Material = Material {
        name: "Glass",
        fill: 0.55,
        fill_strength: 0.4,
        translucency: true,
        frosted_fill: 0.58,
        frosted_fill_strength: 0.16,
        sheen_dark: 9.0,
        sheen_light: 24.0,
        edge_dark: 26.0,
        edge_light: 120.0,
        lift_dark: 0.55,
        lift_light: 0.8,
        shadow_reach: 4.0,
        shadow_reach_per_radius: 0.45,
        glow: 0.32,
        glow_reach: 8.0,
        frosts: true,
        blur: 10.0,
        radius: Radii {
            hairline: 2,
            control: 7,
            row: 8,
            card: 10,
            panel: 12,
            pill: 14,
        },
        row_height: 30.0,
        control_padding: Vec2::new(9.0, 5.0),
        item_spacing: Vec2::new(8.0, 5.0),
        // The default 0.1s reads as flicker; glass should settle, not pop.
        animation: 0.14,
    };
}

/// Cross one palette into another.
///
/// `mix` is for opaque tints and drops alpha on the floor; `shadow` carries
/// its alpha, so a theme crossfade needs a blend that keeps it.
pub fn lerp(a: &Palette, b: &Palette, t: f32) -> Palette {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    let blend = |x: Color32, y: Color32| {
        Color32::from_rgba_premultiplied(
            (x.r() as f32 * inv + y.r() as f32 * t) as u8,
            (x.g() as f32 * inv + y.g() as f32 * t) as u8,
            (x.b() as f32 * inv + y.b() as f32 * t) as u8,
            (x.a() as f32 * inv + y.a() as f32 * t) as u8,
        )
    };
    Palette {
        // Switches once, halfway. It picks which recipe a surface uses — a
        // white wash or a dark one — rather than being a color to interpolate.
        dark: if t < 0.5 { a.dark } else { b.dark },
        bg: blend(a.bg, b.bg),
        surface: blend(a.surface, b.surface),
        surface_hover: blend(a.surface_hover, b.surface_hover),
        active: blend(a.active, b.active),
        accent: blend(a.accent, b.accent),
        text: blend(a.text, b.text),
        text_muted: blend(a.text_muted, b.text_muted),
        border: blend(a.border, b.border),
        shadow: blend(a.shadow, b.shadow),
        // Not a colour and not part of the theme, so it does not cross over —
        // it is whatever the setting says, on both sides of the fade.
        translucency: b.translucency,
        backdrop: b.backdrop,
        // Not a colour either. A crossfade between two *materials* would mean
        // interpolating corner radii and metrics, which is a different and
        // much larger idea than fading two palettes into each other.
        material: b.material,
    }
}

impl Palette {
    /// Stamp the reader's translucency setting on. `resolve` has no business
    /// knowing about Settings, so main.rs does this once a frame.
    pub fn with_translucency(mut self, translucency: Translucency) -> Self {
        self.translucency = translucency;
        self
    }

    /// How round a surface of this size is, per the material.
    ///
    /// The way to spell a corner radius. A number written at a call site is a
    /// number a theme cannot change.
    pub fn radius(&self, tier: Tier) -> u8 {
        self.material.radius.of(tier)
    }

    /// Put a blurred backdrop behind every glass surface that sits on it.
    ///
    /// Scoped by the caller rather than global: the wallpaper is behind the
    /// content area and nothing else, so the sidebar must not be frosted
    /// against a picture that is not behind it.
    pub fn with_backdrop(mut self, backdrop: Option<Backdrop>) -> Self {
        self.backdrop = backdrop;
        self
    }

    /// The part of the backdrop that `rect` sits on: where to draw it, and
    /// which part of the picture that is.
    ///
    /// The overlap, not the whole rect. Requiring a surface to be *wholly*
    /// inside the picture is the obvious rule and the wrong one: a card
    /// scrolled half off the top of the page, or carried past the edge under
    /// the pointer, would stop frosting all at once — and since the fill
    /// recipe follows the frost, it would not merely lose its blur, it would
    /// turn into a different material between one frame and the next. Frosting
    /// the overlap degrades continuously instead, and the uv stays in range,
    /// which is what the containment test was protecting against: egui_glow
    /// samples with `CLAMP_TO_EDGE`, so an out-of-range uv smears the edge row
    /// of the picture across the overhang.
    pub fn backdrop_under(&self, rect: Rect) -> Option<(TextureId, Rect, Rect)> {
        let backdrop = self.backdrop?;
        let page = backdrop.rect;
        if page.width() <= 0.0 || page.height() <= 0.0 {
            return None;
        }
        let quad = rect.intersect(page);
        if quad.width() <= 0.0 || quad.height() <= 0.0 {
            return None;
        }
        let across = |value: f32, low: f32, high: f32, from: f32, to: f32| {
            from + (to - from) * ((value - low) / (high - low))
        };
        let map = |point: egui::Pos2| {
            pos2(
                across(
                    point.x,
                    page.min.x,
                    page.max.x,
                    backdrop.uv.min.x,
                    backdrop.uv.max.x,
                ),
                across(
                    point.y,
                    page.min.y,
                    page.max.y,
                    backdrop.uv.min.y,
                    backdrop.uv.max.y,
                ),
            )
        };
        Some((
            backdrop.texture,
            quad,
            Rect::from_min_max(map(quad.min), map(quad.max)),
        ))
    }
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 3] = [ThemeMode::Auto, ThemeMode::Light, ThemeMode::Dark];

    pub fn label(&self) -> &'static str {
        match self {
            ThemeMode::Auto => "Auto",
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Chrome base — window background, panels.
    pub bg: Color32,
    /// Raised surfaces: address pill, cards, inputs.
    pub surface: Color32,
    /// Hover fill for rows and ghost buttons.
    pub surface_hover: Color32,
    /// Active tab fill — accent-tinted surface.
    pub active: Color32,
    pub accent: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub border: Color32,
    /// Shadow color for the floating content card.
    pub shadow: Color32,
    /// How much comes through a surface — the reader's setting, not a colour.
    ///
    /// It lives here because the palette is already handed to every
    /// `glass::shapes` call in the app; the alternative was a parameter on
    /// nine drawing functions and their callers. `resolve` leaves it at 1.0
    /// and main.rs stamps the setting on, the same way `dark` is a fact about
    /// the theme rather than a colour.
    pub translucency: Translucency,
    /// What surfaces are made of: corner radii, fills, edges, shadows, the
    /// lot. See [`Material`].
    pub material: Material,
    /// What is behind the chrome, blurred, if anything is.
    ///
    /// The backdrop, not the frost: `material.frosts` says whether surfaces
    /// frost at all, and this is the thing they frost against.
    ///
    /// Here for the same reason the translucency setting is: frosted glass is
    /// the material every surface in Zervo is made of, so what it frosts has
    /// to reach every surface without nine call sites being edited to pass it.
    /// A caller draws something behind the chrome, hands the palette a blurred
    /// copy of it, and every card, pill and menu drawn on top is frosted
    /// against it — with no change at the call site at all.
    pub backdrop: Option<Backdrop>,
}

// Colour architecture inspired by Zen Browser's public design tokens: the chrome is neutral gray bases
// (dark #1b1b1b, light "paper" #ebebeb/#e2e2e2) with the accent blended in at
// low ratios, so the accent choice retints the whole chrome, not just
// highlights.

/// Corner radius of the floating web-content card, in points.
/// Largest element gets the largest radius in the size-tiered system.
pub const CONTENT_RADIUS: f32 = Material::GLASS.radius.panel as f32;
/// Gap between the chrome panels and the web-content card, in points.
pub const CONTENT_MARGIN: f32 = 8.0;

/// Workspace dot colors, cycled by workspace index. Mid-saturation pastels
/// that read on both light and dark chrome.
pub const WORKSPACE_COLORS: [Color32; 5] = [
    Color32::from_rgb(150, 120, 240),
    Color32::from_rgb(90, 165, 210),
    Color32::from_rgb(100, 180, 135),
    Color32::from_rgb(222, 160, 84),
    Color32::from_rgb(219, 120, 140),
];

pub fn workspace_color(index: usize) -> Color32 {
    WORKSPACE_COLORS[index % WORKSPACE_COLORS.len()]
}

/// The base the new tab page is painted on: deeper than the chrome in the dark
/// theme, airier in the light one. The page is the one surface with nothing
/// behind it, so it can afford to be further from the chrome than a panel can.
pub fn page_base(palette: &Palette) -> Color32 {
    if palette.dark {
        mix(palette.bg, Color32::BLACK, 0.35)
    } else {
        mix(palette.bg, Color32::WHITE, 0.45)
    }
}

/// The veil laid over a photograph, at `amount` of its full strength.
///
/// The same colour as the page itself, so a wallpaper reads as the page seen
/// through frosted glass rather than as a picture with a grey sheet on it.
/// Some veil is always there: a card has to be legible on a snowfield as well
/// as on a night sky, and no photograph is worth an unreadable page.
pub fn page_veil(palette: &Palette, amount: f32) -> Color32 {
    let base = page_base(palette);
    Color32::from_rgba_unmultiplied(
        base.r(),
        base.g(),
        base.b(),
        (amount.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

pub fn resolve(mode: ThemeMode, system_dark: bool, accent: AccentColor) -> Palette {
    let dark = match mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::Auto => system_dark,
    };
    let accent_color = accent.color(dark);
    let mut palette = if dark {
        Palette {
            dark: true,
            bg: mix(Color32::from_rgb(27, 27, 27), accent_color, 0.045),
            surface: mix(Color32::from_rgb(38, 38, 38), accent_color, 0.06),
            surface_hover: mix(Color32::from_rgb(51, 51, 51), accent_color, 0.07),
            active: Color32::PLACEHOLDER,
            accent: accent_color,
            text: Color32::from_rgb(228, 228, 232),
            text_muted: Color32::from_rgb(158, 158, 168),
            border: mix(Color32::from_rgb(60, 60, 62), accent_color, 0.08),
            shadow: Color32::from_rgba_premultiplied(0, 0, 0, 90),
            translucency: Translucency::Solid,
            material: Material::GLASS,
            backdrop: None,
        }
    } else {
        Palette {
            dark: false,
            bg: mix(Color32::from_rgb(235, 235, 235), accent_color, 0.03),
            surface: mix(Color32::from_rgb(226, 226, 226), accent_color, 0.04),
            surface_hover: mix(Color32::from_rgb(213, 213, 213), accent_color, 0.05),
            active: Color32::PLACEHOLDER,
            accent: accent_color,
            text: Color32::from_rgb(28, 28, 32),
            text_muted: Color32::from_rgb(96, 96, 104),
            border: Color32::from_rgb(204, 204, 204),
            shadow: Color32::from_rgba_premultiplied(0, 0, 0, 50),
            translucency: Translucency::Solid,
            material: Material::GLASS,
            backdrop: None,
        }
    };
    // The active-tab tint follows the accent.
    palette.active = mix(palette.bg, palette.accent, if dark { 0.30 } else { 0.24 });
    palette
}

pub fn apply(ctx: &Context, palette: &Palette) {
    let mut style = (*ctx.global_style()).clone();

    // egui's own widgets — buttons, combo boxes, tooltips, the scrollbars —
    // are styled from the material too, so a theme that changes what a control
    // looks like changes the stock ones with it rather than leaving them
    // looking like a different application bolted on.
    let material = &palette.material;
    style.spacing.item_spacing = material.item_spacing;
    style.spacing.button_padding = material.control_padding;
    style.spacing.window_margin = Margin::same(material.radius.row as i8);
    // egui's default 0.1s reads as flicker; a surface should settle, not pop.
    style.animation_time = material.animation;

    let visuals = &mut style.visuals;
    *visuals = if palette.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = palette.bg;
    visuals.window_fill = palette.bg;
    visuals.extreme_bg_color = palette.surface;
    visuals.faint_bg_color = palette.surface;
    visuals.hyperlink_color = palette.accent;

    visuals.selection.bg_fill = palette.active;
    visuals.selection.stroke = Stroke::new(1.0_f32, palette.accent);

    // Tooltips and context menus are drawn by egui itself, from these fields
    // rather than from the palette — so left alone they keep egui's defaults:
    // a 6pt radius, a flat gray border, and a shadow offset six points right
    // and ten down whose linear ramp bands. Beside Zervo's own cards, which
    // are 10-12pt with an accent-tinted hairline and a symmetric halo, they
    // read as belonging to a different application.
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.window_stroke = Stroke::new(1.0_f32, palette.border);
    visuals.window_shadow = Shadow {
        offset: [0, 2],
        blur: 14,
        spread: 0,
        color: palette.shadow,
    };
    visuals.popup_shadow = visuals.window_shadow;

    let rounding = CornerRadius::same(material.radius.row);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = rounding;
    }
    // Interactive stock widgets (checkboxes, radios, combo boxes) need a
    // visible outline or an unchecked box renders as nothing.
    visuals.widgets.noninteractive.bg_stroke = Stroke::NONE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, palette.border);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, palette.text_muted);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, palette.accent);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, palette.border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, palette.text_muted);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, palette.text_muted);
    visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, palette.text);
    visuals.widgets.hovered.weak_bg_fill = palette.surface_hover;
    visuals.widgets.hovered.bg_fill = palette.surface_hover;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, palette.text);
    visuals.widgets.active.weak_bg_fill = palette.active;
    visuals.widgets.active.bg_fill = palette.active;
    visuals.widgets.open.weak_bg_fill = palette.surface_hover;

    ctx.set_global_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, TextureId, pos2};

    /// A page at 100,100 → 900,700 showing the middle half of a picture.
    fn palette_with_backdrop() -> Palette {
        let mut palette = resolve(ThemeMode::Dark, true, AccentColor::Lavender);
        palette.backdrop = Some(Backdrop {
            texture: TextureId::default(),
            rect: Rect::from_min_max(pos2(100.0, 100.0), pos2(900.0, 700.0)),
            uv: Rect::from_min_max(pos2(0.25, 0.25), pos2(0.75, 0.75)),
            alpha: 1.0,
        });
        palette
    }

    #[test]
    fn a_surface_on_the_page_frosts_over_all_of_itself() {
        let palette = palette_with_backdrop();
        let card = Rect::from_min_max(pos2(300.0, 250.0), pos2(500.0, 400.0));
        let (_, quad, uv) = palette.backdrop_under(card).expect("card is on the page");
        assert_eq!(
            quad, card,
            "a card wholly on the page frosts over all of it"
        );
        // 300 is a quarter of the way across 100..900, and the picture shows
        // 0.25..0.75, so a quarter of the way in is 0.375.
        assert!((uv.min.x - 0.375).abs() < 1e-5, "uv.min.x was {}", uv.min.x);
        assert!((uv.max.x - 0.5).abs() < 1e-5, "uv.max.x was {}", uv.max.x);
    }

    /// The bug this was written for: a card scrolled half off the top of the
    /// page, or carried past its edge under the pointer, used to stop frosting
    /// altogether — and because the fill recipe followed the frost, it did not
    /// merely lose its blur, it changed material between one frame and the
    /// next.
    #[test]
    fn a_surface_hanging_off_the_page_still_frosts_over_the_part_that_is_on_it() {
        let palette = palette_with_backdrop();
        let page = palette.backdrop.unwrap().rect;
        for card in [
            Rect::from_min_max(pos2(300.0, 40.0), pos2(500.0, 300.0)), // off the top
            Rect::from_min_max(pos2(300.0, 600.0), pos2(500.0, 950.0)), // off the bottom
            Rect::from_min_max(pos2(20.0, 200.0), pos2(300.0, 400.0)), // off the left
            Rect::from_min_max(pos2(800.0, 200.0), pos2(1200.0, 400.0)), // off the right
        ] {
            let (_, quad, uv) = palette
                .backdrop_under(card)
                .expect("part of the card is still on the page");
            assert_eq!(quad, card.intersect(page), "frosts exactly the overlap");
            // Which is what keeps the uv in range: egui samples with
            // CLAMP_TO_EDGE, so an out-of-range uv smears the picture's edge
            // row across the overhang.
            let picture = palette.backdrop.unwrap().uv;
            assert!(
                uv.min.x >= picture.min.x - 1e-5
                    && uv.min.y >= picture.min.y - 1e-5
                    && uv.max.x <= picture.max.x + 1e-5
                    && uv.max.y <= picture.max.y + 1e-5,
                "uv {uv:?} escaped the picture {picture:?}"
            );
        }
    }

    #[test]
    fn a_surface_nowhere_near_the_page_does_not_frost() {
        let palette = palette_with_backdrop();
        let elsewhere = Rect::from_min_max(pos2(0.0, 0.0), pos2(90.0, 90.0));
        assert!(palette.backdrop_under(elsewhere).is_none());
    }

    #[test]
    fn nothing_frosts_without_a_backdrop() {
        let palette = resolve(ThemeMode::Dark, true, AccentColor::Lavender);
        let card = Rect::from_min_max(pos2(300.0, 250.0), pos2(500.0, 400.0));
        assert!(palette.backdrop_under(card).is_none());
    }

    /// Every tier has to resolve to something; a material that forgot one
    /// would give a surface square corners with no other sign.
    #[test]
    fn every_radius_tier_resolves() {
        let radii = Material::GLASS.radius;
        for tier in [
            Tier::Hairline,
            Tier::Control,
            Tier::Row,
            Tier::Card,
            Tier::Panel,
            Tier::Pill,
        ] {
            assert!(radii.of(tier) > 0, "{tier:?} resolved to nothing");
        }
    }
}
