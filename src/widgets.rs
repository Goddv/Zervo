//! Consistent, palette-driven form controls. egui's stock checkbox, radio and
//! slider each carry their own visual language; these replace them with one
//! family of controls that matches the rest of the chrome — same radii,
//! same accent, same animation timings.

use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Rect, Sense, Stroke, StrokeKind, Ui,
    WidgetInfo, WidgetType, pos2, vec2,
};

use crate::glass;
use crate::theme::{self, Palette, Tier};

/// Row height shared by every control, so stacked settings line up. The
/// material's, so a theme that wants roomier controls says so once.
const ROW_HEIGHT: f32 = crate::theme::Material::ZERVO.row_height;

/// An iOS-style switch with a label. Returns true when toggled.
pub fn toggle(ui: &mut Ui, value: &mut bool, label: &str, palette: &Palette) -> bool {
    let (rect, mut response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    let changed = response.clicked();
    if changed {
        *value = !*value;
        // Without this `Response::changed()` is false however the value moved,
        // so nothing downstream -- egui's own change plumbing included -- can
        // tell that it did.
        response.mark_changed();
    }
    // What this control *is*, for the accessibility tree. A hand-drawn widget
    // that only paints is an unlabelled generic node to a screen reader; egui
    // fills in the role, label and value from here and nowhere else.
    response
        .widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, ui.is_enabled(), *value, label));

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
    // Half the track's own height, which is what makes a capsule a capsule —
    // not a rung of the material's ladder, and it must not follow one. A
    // switch rounded to Flat's corner scale is a rectangle with a circle
    // sliding inside it.
    let capsule = CornerRadius::same((track.height() * 0.5) as u8);
    painter.rect_filled(track, capsule, track_color);
    if on < 1.0 {
        painter.rect_stroke(
            track,
            capsule,
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

/// What a slider did this frame.
///
/// The two are separate because these controls write straight into `Settings`,
/// and `Settings` is a file. A slider held for a second reports a change on
/// every one of sixty frames, and persisting each one is sixty synchronous
/// rewrites of the whole settings file. The chrome reads the value out of
/// `Settings` either way, so the picture follows the drag regardless of what
/// this says; `settled` decides only when the *file* is written.
///
/// `ui.rs` already does this by hand for the sidebar width and the navigation
/// bar's height, each with a comment saying "written once the drag ends, not
/// every frame of it". The sliders were the ones that missed out.
///
/// There is deliberately no `changed` beside it. The value is written straight
/// through the `&mut f32`, and the chrome reads it from there, so every caller
/// so far wants exactly one thing from the return: whether to write the file.
/// A struct rather than a bare `bool` because `.settled` says which question is
/// being answered, and a bare `bool` here once meant the other one.
#[derive(Clone, Copy, Default)]
pub struct SliderOut {
    /// The interaction finished, so this is a value worth keeping.
    pub settled: bool,
}

/// A labelled slider with an accent-filled track.
pub fn slider(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    palette: &Palette,
) -> SliderOut {
    let (rect, mut response) = ui.allocate_exact_size(
        vec2(ui.available_width(), ROW_HEIGHT),
        Sense::click_and_drag(),
    );

    let (min, max) = (*range.start(), *range.end());
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), 5.0));

    let mut changed = false;
    // A click lands on its value at once and is over; a drag is only worth
    // keeping when the pointer comes up.
    let mut settled = response.drag_stopped() || response.clicked();
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

    // Arrow keys, once focused. A slider that answers only to a pointer cannot
    // be set at all without one.
    if response.has_focus() {
        let step = (max - min) / 50.0;
        let nudge = ui.input(|input| {
            let back =
                input.key_pressed(egui::Key::ArrowLeft) || input.key_pressed(egui::Key::ArrowDown);
            let on =
                input.key_pressed(egui::Key::ArrowRight) || input.key_pressed(egui::Key::ArrowUp);
            f32::from(on) - f32::from(back)
        });
        if nudge != 0.0 {
            let next = (*value + nudge * step).clamp(min, max);
            if (next - *value).abs() > f32::EPSILON {
                *value = next;
                changed = true;
                // A keypress is a whole gesture on its own, so it is already
                // settled -- there is no release to wait for.
                settled = true;
            }
        }
    }

    if changed {
        response.mark_changed();
    }
    response.widget_info(|| WidgetInfo::slider(ui.is_enabled(), f64::from(*value), ""));

    let t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
    let hover = glass::ease_out(ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() || response.dragged(),
        0.12,
    ));

    let painter = ui.painter();
    // Half the track's height again: a slider's groove is a capsule.
    let groove = CornerRadius::same((track.height() * 0.5) as u8);
    painter.rect_filled(track, groove, palette.surface_hover);
    let filled = Rect::from_min_max(
        track.min,
        pos2(track.min.x + track.width() * t, track.max.y),
    );
    if filled.width() > 0.0 {
        painter.rect_filled(filled, groove, palette.accent);
    }
    let knob = pos2(track.min.x + track.width() * t, track.center().y);
    painter.circle_filled(knob, 9.0 + hover, palette.shadow.gamma_multiply(0.5));
    painter.circle_filled(knob, 8.0 + hover * 0.5, Color32::WHITE);

    response.on_hover_cursor(CursorIcon::ResizeHorizontal);
    SliderOut { settled }
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
        palette.corner(Tier::Row),
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
                palette.corner(Tier::Control),
                palette.accent.gamma_multiply(0.85 * on),
            );
        } else if hover > 0.0 {
            ui.painter().rect_filled(
                slot_rect.shrink(3.0),
                palette.corner(Tier::Control),
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
        let mut response = response.on_hover_cursor(CursorIcon::PointingHand);
        response.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::RadioButton,
                ui.is_enabled(),
                index == selected,
                *option,
            )
        });
        if response.clicked() && index != selected {
            response.mark_changed();
            changed = Some(index);
        }
    }
    changed
}
