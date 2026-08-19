//! Consistent, palette-driven form controls. egui's stock checkbox, radio and
//! slider each carry their own visual language; these replace them with one
//! family of controls that matches the rest of the chrome — same radii,
//! same accent, same animation timings.

use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Rect, Sense, Stroke, StrokeKind, Ui, pos2,
    vec2,
};

use crate::glass;
use crate::theme::{self, Palette};

/// Row height shared by every control, so stacked settings line up.
const ROW_HEIGHT: f32 = 30.0;

/// An iOS-style switch with a label. Returns true when toggled.
pub fn toggle(ui: &mut Ui, value: &mut bool, label: &str, palette: &Palette) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    let changed = response.clicked();
    if changed {
        *value = !*value;
    }

    let on = glass::ease_out(
        ui.ctx()
            .animate_bool_with_time(response.id.with("on"), *value, 0.16),
    );
    let hover = glass::ease_out(ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered(),
        0.12,
    ));

    let painter = ui.painter();
    painter.text(
        pos2(rect.min.x, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        theme::mix(palette.text_muted, palette.text, hover.max(on)),
    );

    // Track.
    let track = Rect::from_center_size(pos2(rect.max.x - 20.0, rect.center().y), vec2(38.0, 22.0));
    let track_color = theme::mix(
        palette.surface_hover.gamma_multiply(0.9),
        palette.accent,
        on,
    );
    painter.rect_filled(track, CornerRadius::same(11), track_color);
    if on < 1.0 {
        painter.rect_stroke(
            track,
            CornerRadius::same(11),
            Stroke::new(1.0_f32, palette.border.gamma_multiply(1.0 - on)),
            StrokeKind::Inside,
        );
    }
    // Knob slides across as the value animates.
    let knob_x = track.min.x + 11.0 + on * (track.width() - 22.0);
    painter.circle_filled(
        pos2(knob_x, track.center().y),
        8.0 + hover * 0.5,
        Color32::WHITE.gamma_multiply(if palette.dark { 0.94 } else { 1.0 }),
    );

    response.on_hover_cursor(CursorIcon::PointingHand);
    changed
}

/// A labelled slider with an accent-filled track. Returns true while dragging
/// changed the value.
pub fn slider(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    palette: &Palette,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), ROW_HEIGHT),
        Sense::click_and_drag(),
    );

    let (min, max) = (*range.start(), *range.end());
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), 5.0));

    let mut changed = false;
    if let Some(pointer) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked())
    {
        let t = ((pointer.x - track.min.x) / track.width()).clamp(0.0, 1.0);
        let next = min + t * (max - min);
        if (next - *value).abs() > f32::EPSILON {
            *value = next;
            changed = true;
        }
    }

    let t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
    let hover = glass::ease_out(ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() || response.dragged(),
        0.12,
    ));

    let painter = ui.painter();
    painter.rect_filled(track, CornerRadius::same(3), palette.surface_hover);
    let filled = Rect::from_min_max(
        track.min,
        pos2(track.min.x + track.width() * t, track.max.y),
    );
    if filled.width() > 0.0 {
        painter.rect_filled(filled, CornerRadius::same(3), palette.accent);
    }
    let knob = pos2(track.min.x + track.width() * t, track.center().y);
    painter.circle_filled(knob, 9.0 + hover, palette.shadow.gamma_multiply(0.5));
    painter.circle_filled(knob, 8.0 + hover * 0.5, Color32::WHITE);

    response.on_hover_cursor(CursorIcon::ResizeHorizontal);
    changed
}

/// A segmented control — the consistent replacement for rows of radio
/// buttons. Returns the newly selected index when it changes.
pub fn segmented(
    ui: &mut Ui,
    selected: usize,
    options: &[&str],
    palette: &Palette,
) -> Option<usize> {
    if options.is_empty() {
        return None;
    }
    let height = 32.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(9),
        palette.surface_hover.gamma_multiply(0.7),
    );

    let slot = rect.width() / options.len() as f32;
    let mut changed = None;
    for (index, option) in options.iter().enumerate() {
        let slot_rect = Rect::from_min_size(
            pos2(rect.min.x + slot * index as f32, rect.min.y),
            vec2(slot, height),
        );
        let response = ui.interact(
            slot_rect,
            ui.id().with(("segment", index, *option)),
            Sense::click(),
        );
        let on = glass::ease_out(ui.ctx().animate_bool_with_time(
            ui.id().with(("segment_on", index, *option)),
            index == selected,
            0.16,
        ));
        let hover = glass::ease_out(ui.ctx().animate_bool_with_time(
            ui.id().with(("segment_hover", index, *option)),
            response.hovered() && index != selected,
            0.12,
        ));
        if on > 0.0 {
            ui.painter().rect_filled(
                slot_rect.shrink(3.0),
                CornerRadius::same(7),
                palette.accent.gamma_multiply(0.85 * on),
            );
        } else if hover > 0.0 {
            ui.painter().rect_filled(
                slot_rect.shrink(3.0),
                CornerRadius::same(7),
                palette.surface.gamma_multiply(hover),
            );
        }
        ui.painter().text(
            slot_rect.center(),
            Align2::CENTER_CENTER,
            *option,
            FontId::proportional(12.5),
            if on > 0.5 {
                Color32::WHITE
            } else {
                theme::mix(palette.text_muted, palette.text, hover)
            },
        );
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() && index != selected {
            changed = Some(index);
        }
    }
    changed
}
