//! Liquid Glass material for the chrome: layered translucency, specular top
//! sheen, hairline rim light, and soft feathered depth shadow — parameterized
//! by the active palette so it composes on dark and light themes. (egui
//! cannot backdrop-blur, but over the chrome's solid gradient the layered
//! recipe reads the same.)

use egui::epaint::RectShape;
use egui::{Color32, CornerRadius, Painter, Rect, Shape, Stroke, StrokeKind, vec2};

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
}

impl Glass {
    pub fn new(radius: u8) -> Self {
        Self {
            radius,
            strength: 1.0,
            glow: 0.0,
            tint: None,
            shadow: true,
        }
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
pub fn shapes(rect: Rect, palette: &Palette, glass: Glass) -> Vec<Shape> {
    let mut out = Vec::new();
    if glass.strength <= 0.0 && glass.glow <= 0.0 {
        return out;
    }
    let radius = CornerRadius::same(glass.radius);
    let dark = palette.dark;
    let strength = glass.strength.clamp(0.0, 1.0);

    // Accent glow halo behind active/focused elements — blur-feathered so it
    // falls off like light rather than reading as a colored band.
    if glass.glow > 0.0 {
        out.push(
            RectShape::filled(
                rect.expand(5.0),
                CornerRadius::same(glass.radius.saturating_add(5)),
                palette.accent.gamma_multiply(0.22 * glass.glow),
            )
            .with_blur_width(10.0)
            .into(),
        );
    }

    // Soft drop shadow for lift, feathered downward.
    if glass.shadow {
        out.push(
            RectShape::filled(
                rect.translate(vec2(0.0, 2.0)).expand(2.0),
                CornerRadius::same(glass.radius.saturating_add(2)),
                palette.shadow.gamma_multiply(0.6 * strength),
            )
            .with_blur_width(6.0)
            .into(),
        );
    }

    // Translucent core over the chrome gradient.
    let core = glass.tint.unwrap_or(palette.surface);
    out.push(Shape::rect_filled(
        rect,
        radius,
        core.gamma_multiply(0.55 + 0.4 * strength),
    ));

    // Glass wash — white translucency per the glassmorphism recipe. Light
    // theme leans on the darker core for contrast; white is highlights only.
    let wash_alpha = if dark { 9.0 } else { 24.0 } * strength;
    out.push(Shape::rect_filled(
        rect,
        radius,
        Color32::from_white_alpha(wash_alpha as u8),
    ));

    // Flat design: no specular sheen, no rim light, no bottom shade. Surfaces
    // are carried by fill + hairline border alone; depth comes from the
    // shadow and the accent glow, not from faked highlights.

    // Hairline border (translucent white per spec).
    let border_alpha = if dark { 26.0 } else { 120.0 } * strength.max(0.6);
    out.push(Shape::rect_stroke(
        rect,
        radius,
        Stroke::new(1.0_f32, Color32::from_white_alpha(border_alpha as u8)),
        StrokeKind::Inside,
    ));
    // Light theme needs a whisper of dark outer definition too.
    if !dark {
        out.push(Shape::rect_stroke(
            rect,
            radius,
            Stroke::new(1.0_f32, Color32::from_black_alpha(24)),
            StrokeKind::Outside,
        ));
    }

    out
}
