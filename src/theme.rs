//! Theme system: dark and light palettes with an Auto mode that follows the
//! OS appearance (which on macOS follows the day/night cycle when the system
//! is set to Auto). Icon and text colors always derive from the active
//! palette, so glyphs compose correctly on both light and dark chrome.

use egui::{Color32, Context, CornerRadius, Margin, Stroke, Vec2};
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
}

// Colour architecture inspired by Zen Browser's public design tokens: the chrome is neutral gray bases
// (dark #1b1b1b, light "paper" #ebebeb/#e2e2e2) with the accent blended in at
// low ratios, so the accent choice retints the whole chrome, not just
// highlights.

/// Corner radius of the floating web-content card, in points.
/// Largest element gets the largest radius in the size-tiered system.
pub const CONTENT_RADIUS: f32 = 12.0;
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
        }
    };
    // The active-tab tint follows the accent.
    palette.active = mix(palette.bg, palette.accent, if dark { 0.30 } else { 0.24 });
    palette
}

pub fn apply(ctx: &Context, palette: &Palette) {
    let mut style = (*ctx.global_style()).clone();

    style.spacing.item_spacing = Vec2::new(8.0, 5.0);
    style.spacing.button_padding = Vec2::new(9.0, 5.0);
    style.spacing.window_margin = Margin::same(8);
    // The default 0.1s reads as flicker; glass should settle, not pop.
    style.animation_time = 0.14;

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

    let rounding = CornerRadius::same(8);
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
