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
    Lavender,
    Sky,
    Mint,
    Amber,
    Rose,
}

impl AccentColor {
    pub const ALL: [AccentColor; 5] = [
        AccentColor::Lavender,
        AccentColor::Sky,
        AccentColor::Mint,
        AccentColor::Amber,
        AccentColor::Rose,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            AccentColor::Lavender => "Lavender",
            AccentColor::Sky => "Sky",
            AccentColor::Mint => "Mint",
            AccentColor::Amber => "Amber",
            AccentColor::Rose => "Rose",
        }
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

/// A picture behind the chrome, already blurred, for glass surfaces to frost.
///
/// egui cannot blur what is behind a shape while it draws it, and nothing here
/// needs it to. The only thing ever behind the chrome is a wallpaper, which is
/// a still image — so it is blurred once, when it is decoded, and the material
/// samples that blurred copy through the same mapping the sharp one is drawn
/// with. What comes out is a real backdrop blur rather than an impression of
/// one, and it costs nothing per frame.
#[derive(Clone, Copy)]
pub struct Frost {
    /// A blurred copy of the picture. Small: it is blurred, so there is no
    /// detail left in it to be worth storing at size.
    pub texture: TextureId,
    /// Where the sharp picture is drawn, in screen points.
    pub rect: Rect,
    /// The part of the picture that `rect` shows — the same window the sharp
    /// one uses, so the blur underneath a card lines up with the photograph
    /// beside it.
    pub uv: Rect,
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
    pub shadow_reach_radius: f32,
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
        frosted_fill: 0.58,
        frosted_fill_strength: 0.16,
        sheen_dark: 9.0,
        sheen_light: 24.0,
        edge_dark: 26.0,
        edge_light: 120.0,
        lift_dark: 0.55,
        lift_light: 0.8,
        shadow_reach: 4.0,
        shadow_reach_radius: 0.45,
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
        card_opacity: b.card_opacity,
        frost: b.frost,
        // Not a colour either. A crossfade between two *materials* would mean
        // interpolating corner radii and metrics, which is a different and
        // much larger idea than fading two palettes into each other.
        material: b.material,
    }
}

impl Palette {
    /// Stamp the user's card-opacity setting on. `resolve` has no business
    /// knowing about Settings, so main.rs does this once a frame.
    ///
    /// NaN is possible — the value is deserialised from settings.json — and it
    /// would reach `Color32::gamma_multiply`, which debug-asserts on a
    /// non-finite factor and would then panic once per surface per frame.
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
    pub fn with_frost(mut self, frost: Option<Frost>) -> Self {
        self.frost = frost;
        self
    }

    /// The part of the frosted backdrop that `rect` covers, if it covers any.
    ///
    /// A surface frosts only when it sits wholly inside the picture. One
    /// hanging off an edge would sample past the texture, and clamped
    /// sampling smears the last row of pixels down the overhang — which looks
    /// far worse than not frosting it at all.
    pub fn frost_behind(&self, rect: Rect) -> Option<(TextureId, Rect)> {
        let frost = self.frost?;
        let page = frost.rect;
        if page.width() <= 0.0 || page.height() <= 0.0 {
            return None;
        }
        if rect.min.x < page.min.x
            || rect.min.y < page.min.y
            || rect.max.x > page.max.x
            || rect.max.y > page.max.y
        {
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
                    frost.uv.min.x,
                    frost.uv.max.x,
                ),
                across(
                    point.y,
                    page.min.y,
                    page.max.y,
                    frost.uv.min.y,
                    frost.uv.max.y,
                ),
            )
        };
        Some((
            frost.texture,
            Rect::from_min_max(map(rect.min), map(rect.max)),
        ))
    }

    pub fn with_card_opacity(mut self, opacity: f32) -> Self {
        self.card_opacity = if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self
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
    /// How solid the chrome's card surfaces are, 0.0..=1.0 — the user's
    /// setting, not a colour.
    ///
    /// It lives here because the palette is already handed to every
    /// `glass::shapes` call in the app; the alternative was a parameter on
    /// nine drawing functions and their callers. `resolve` leaves it at 1.0
    /// and main.rs stamps the setting on, the same way `dark` is a fact about
    /// the theme rather than a colour.
    pub card_opacity: f32,
    /// What surfaces are made of: corner radii, fills, edges, shadows, the
    /// lot. See [`Material`].
    pub material: Material,
    /// What is behind the chrome, blurred, if anything is.
    ///
    /// Here for the same reason `card_opacity` is: frosted glass is the
    /// material every surface in Zervo is made of, so the thing it frosts has
    /// to reach every surface without nine call sites being edited to pass it.
    /// A caller draws something behind the chrome, hands the palette a blurred
    /// copy of it, and every card, pill and menu drawn on top is frosted
    /// against it — with no change at the call site at all.
    pub frost: Option<Frost>,
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
            card_opacity: 1.0,
            material: Material::GLASS,
            frost: None,
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
            card_opacity: 1.0,
            material: Material::GLASS,
            frost: None,
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
