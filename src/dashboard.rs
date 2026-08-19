//! The widget strip that a taller navigation bar uncovers.
//!
//! Widgets are laid out left to right, dragged to reorder, and added from a
//! menu at the end of the row. Which ones are placed, and in what order, is
//! remembered in the settings.
//!
//! The media widgets are not mock-ups: Servo reports media session metadata,
//! playback state and position, and takes play/pause/track actions back, so
//! they drive whatever the page is playing.

use egui::{
    Align2, CornerRadius, CursorIcon, FontId, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use serde::{Deserialize, Serialize};

use crate::glass::{self, Glass};
use crate::icons::{self, Icon};
use crate::theme::Palette;

/// What the page is playing, as far as the engine has told us.
#[derive(Clone, Default)]
pub struct Media {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub duration: f64,
    pub position: f64,
}

impl Media {
    pub fn is_idle(&self) -> bool {
        self.title.is_empty() && !self.playing
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum WidgetKind {
    Clock,
    NowPlaying,
    Transport,
}

impl WidgetKind {
    pub const ALL: [WidgetKind; 3] =
        [WidgetKind::Clock, WidgetKind::NowPlaying, WidgetKind::Transport];

    pub fn label(self) -> &'static str {
        match self {
            WidgetKind::Clock => "Clock",
            WidgetKind::NowPlaying => "Now playing",
            WidgetKind::Transport => "Player controls",
        }
    }

    /// Fixed widths keep reordering predictable: a widget does not change size
    /// depending on where it lands.
    fn width(self) -> f32 {
        match self {
            WidgetKind::Clock => 128.0,
            WidgetKind::NowPlaying => 260.0,
            WidgetKind::Transport => 150.0,
        }
    }
}

/// What the strip wants done, handed back rather than applied here so the
/// settings stay the one place that owns the arrangement.
pub enum Change {
    Add(WidgetKind),
    Remove(usize),
    Move { from: usize, to: usize },
    Media(servo::MediaSessionActionType),
}

const GAP: f32 = 10.0;
const PAD: f32 = 6.0;
/// Widgets are a fixed height, never stretched to whatever the shelf happens
/// to be. Dragging reveals more of a card that keeps its size, the way a card
/// slides out of a stack; a card that grows and shrinks with the drag reads as
/// a rubber sheet instead.
const WIDGET_HEIGHT: f32 = 52.0;

/// The bar height at which the shelf shows a whole widget.
pub const SHELF_OPEN_HEIGHT: f32 = WIDGET_HEIGHT + PAD * 2.0;

/// Draw the strip into `area`. Returns whatever the user asked for.
pub fn draw(
    root: &mut Ui,
    palette: &Palette,
    media: &Media,
    placed: &[WidgetKind],
    area: Rect,
) -> Vec<Change> {
    let mut changes = Vec::new();
    if area.height() < 24.0 {
        return changes;
    }

    let ctx = root.ctx().clone();
    // Everything is clipped to the shelf, so a half-open one shows the top of
    // a whole card rather than a squashed one — it is sliding out from under
    // the controls, not being compressed by them.
    let mut root = root.new_child(egui::UiBuilder::new().max_rect(area));
    root.set_clip_rect(area);
    let root = &mut root;
    let drag_id = Id::new("zervo_widget_drag");
    let dragging = ctx.data(|data| data.get_temp::<usize>(drag_id));

    // Slots first, so a dragged widget knows where it would land.
    let mut slots = Vec::new();
    let mut x = area.min.x + PAD;
    for kind in placed {
        let rect = Rect::from_min_size(
            pos2(x, area.min.y + PAD),
            vec2(kind.width(), WIDGET_HEIGHT),
        );
        slots.push(rect);
        x += kind.width() + GAP;
    }

    let pointer = ctx.input(|input| input.pointer.latest_pos());
    // Where a drag would drop: the slot whose centre the pointer is nearest.
    let target = pointer.map(|pos| {
        slots
            .iter()
            .position(|slot| pos.x < slot.center().x)
            .unwrap_or(slots.len().saturating_sub(1))
    });

    for (index, (kind, slot)) in placed.iter().zip(slots.iter()).enumerate() {
        let response = root.interact(*slot, drag_id.with(index), Sense::click_and_drag());
        let held = dragging == Some(index);

        if response.drag_started() {
            ctx.data_mut(|data| data.insert_temp(drag_id, index));
        }
        if held && response.drag_stopped() {
            ctx.data_mut(|data| data.remove::<usize>(drag_id));
            if let Some(to) = target
                && to != index
            {
                changes.push(Change::Move { from: index, to });
            }
        }
        if response.hovered() || held {
            ctx.set_cursor_icon(if held {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            });
        }

        // A held widget follows the pointer; the gap it left stays open so it
        // is obvious where it will land.
        let drawn = if held {
            slot.translate(vec2(
                pointer.map(|pos| pos.x - slot.center().x).unwrap_or(0.0),
                0.0,
            ))
        } else {
            *slot
        };

        if held {
            root.painter().rect_stroke(
                *slot,
                CornerRadius::same(10),
                Stroke::new(1.0_f32, palette.accent.gamma_multiply(0.6)),
                StrokeKind::Inside,
            );
        }

        draw_widget(root, palette, media, *kind, drawn, held, &mut changes);

        // Remove on hover, in the corner, so it is never in the way.
        if response.hovered() && !held {
            let close = Rect::from_center_size(
                pos2(drawn.max.x - 11.0, drawn.min.y + 11.0),
                vec2(16.0, 16.0),
            );
            icons::draw_icon(root.painter(), close.shrink(3.0), Icon::Close, palette.text_muted);
            if root
                .interact(close, drag_id.with(("close", index)), Sense::click())
                .clicked()
            {
                changes.push(Change::Remove(index));
            }
        }
    }

    // ── The add tile, at the end of the row.
    let add = Rect::from_min_size(
        pos2(x, area.min.y + PAD),
        vec2(38.0, WIDGET_HEIGHT),
    );
    if add.max.x <= area.max.x - PAD {
        let response = root.interact(add, Id::new("zervo_widget_add"), Sense::click());
        let open_id = Id::new("zervo_widget_menu");
        let open = ctx.data(|data| data.get_temp::<bool>(open_id)).unwrap_or(false);
        root.painter().rect_stroke(
            add,
            CornerRadius::same(10),
            Stroke::new(
                1.0_f32,
                palette
                    .border
                    .gamma_multiply(if response.hovered() { 1.6 } else { 1.0 }),
            ),
            StrokeKind::Inside,
        );
        icons::draw_icon(
            root.painter(),
            Rect::from_center_size(add.center(), vec2(15.0, 15.0)),
            Icon::Plus,
            palette.text_muted,
        );
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            ctx.data_mut(|data| data.insert_temp(open_id, !open));
        }
        if open && let Some(kind) = draw_add_menu(root, palette, add) {
            ctx.data_mut(|data| data.insert_temp(open_id, false));
            changes.push(Change::Add(kind));
        }
    }

    changes
}

fn draw_add_menu(root: &mut Ui, palette: &Palette, anchor: Rect) -> Option<WidgetKind> {
    const ROW: f32 = 30.0;
    let rect = Rect::from_min_size(
        pos2(anchor.min.x, anchor.max.y + 6.0),
        vec2(170.0, WidgetKind::ALL.len() as f32 * ROW + 12.0),
    );
    let mut chosen = None;
    egui::Area::new(Id::new("zervo_widget_menu_area"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(&root.ctx().clone(), |ui| {
            let painter = ui.painter();
            painter.rect_filled(rect, CornerRadius::same(10), palette.bg);
            for shape in glass::shapes(rect, palette, Glass::new(10)) {
                painter.add(shape);
            }
            painter.rect_stroke(
                rect,
                CornerRadius::same(10),
                Stroke::new(1.0_f32, palette.border),
                StrokeKind::Inside,
            );
            for (index, kind) in WidgetKind::ALL.iter().enumerate() {
                let row = Rect::from_min_size(
                    pos2(rect.min.x + 6.0, rect.min.y + 6.0 + index as f32 * ROW),
                    vec2(rect.width() - 12.0, ROW),
                );
                let response =
                    ui.interact(row, Id::new("zervo_widget_pick").with(index), Sense::click());
                if response.hovered() {
                    ui.painter()
                        .rect_filled(row, CornerRadius::same(7), palette.surface_hover);
                }
                ui.painter().text(
                    pos2(row.min.x + 8.0, row.center().y),
                    Align2::LEFT_CENTER,
                    kind.label(),
                    FontId::proportional(13.0),
                    palette.text,
                );
                if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    chosen = Some(*kind);
                }
            }
            ui.advance_cursor_after_rect(rect);
        });
    chosen
}

fn draw_widget(
    root: &mut Ui,
    palette: &Palette,
    media: &Media,
    kind: WidgetKind,
    rect: Rect,
    held: bool,
    changes: &mut Vec<Change>,
) {
    let painter = root.painter();
    // Opaque, with the shadow left on: these stack over the content card, and
    // a translucent card in a pile does not read as a card.
    painter.rect_filled(rect, CornerRadius::same(10), palette.bg);
    for shape in glass::shapes(
        rect,
        palette,
        Glass::new(10).strength(if held { 1.0 } else { 0.85 }),
    ) {
        painter.add(shape);
    }

    match kind {
        WidgetKind::Clock => {
            let now = chrono::Local::now();
            painter.text(
                pos2(rect.center().x, rect.center().y - 8.0),
                Align2::CENTER_CENTER,
                now.format("%H:%M").to_string(),
                FontId::proportional(22.0),
                palette.text,
            );
            painter.text(
                pos2(rect.center().x, rect.center().y + 12.0),
                Align2::CENTER_CENTER,
                now.format("%a %-d %b").to_string(),
                FontId::proportional(11.5),
                palette.text_muted,
            );
            root.ctx().request_repaint_after(std::time::Duration::from_secs(20));
        },
        WidgetKind::NowPlaying => {
            if media.is_idle() {
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Nothing playing",
                    FontId::proportional(12.5),
                    palette.text_muted,
                );
                return;
            }
            painter.text(
                pos2(rect.min.x + 12.0, rect.min.y + 14.0),
                Align2::LEFT_CENTER,
                crate::ui::ellipsize(&media.title, 28),
                FontId::proportional(13.0),
                palette.text,
            );
            painter.text(
                pos2(rect.min.x + 12.0, rect.min.y + 31.0),
                Align2::LEFT_CENTER,
                crate::ui::ellipsize(&media.artist, 30),
                FontId::proportional(11.5),
                palette.text_muted,
            );
            if media.duration > 0.0 {
                let track = Rect::from_min_size(
                    pos2(rect.min.x + 12.0, rect.max.y - 14.0),
                    vec2(rect.width() - 24.0, 3.0),
                );
                painter.rect_filled(track, CornerRadius::same(2), palette.border);
                let played = (media.position / media.duration).clamp(0.0, 1.0) as f32;
                painter.rect_filled(
                    Rect::from_min_size(track.min, vec2(track.width() * played, track.height())),
                    CornerRadius::same(2),
                    palette.accent,
                );
            }
        },
        WidgetKind::Transport => {
            use servo::MediaSessionActionType;
            let buttons = [
                (Icon::Back, MediaSessionActionType::PreviousTrack),
                (
                    if media.playing { Icon::Pause } else { Icon::Play },
                    if media.playing {
                        MediaSessionActionType::Pause
                    } else {
                        MediaSessionActionType::Play
                    },
                ),
                (Icon::Forward, MediaSessionActionType::NextTrack),
            ];
            for (index, (icon, action)) in buttons.into_iter().enumerate() {
                let centre = pos2(
                    rect.min.x + rect.width() * (0.25 + 0.25 * index as f32),
                    rect.center().y,
                );
                let hit = Rect::from_center_size(centre, vec2(30.0, 30.0));
                let response = root.interact(
                    hit,
                    Id::new("zervo_transport").with((rect.min.x as i32, index)),
                    Sense::click(),
                );
                if response.hovered() {
                    root.painter().rect_filled(
                        hit,
                        CornerRadius::same(8),
                        palette.surface_hover,
                    );
                }
                icons::draw_icon(
                    root.painter(),
                    Rect::from_center_size(centre, vec2(16.0, 16.0)),
                    icon,
                    if media.is_idle() {
                        palette.text_muted.gamma_multiply(0.4)
                    } else {
                        palette.text
                    },
                );
                if !media.is_idle()
                    && response.on_hover_cursor(CursorIcon::PointingHand).clicked()
                {
                    changes.push(Change::Media(action));
                }
            }
        },
    }
}

/// The mark on the bar's bottom edge that says it can be dragged.
///
/// Always drawn, not only on hover: an affordance nobody can see is not an
/// affordance, and there is no other clue that the widgets are down there.
pub fn draw_grabber(painter: &egui::Painter, palette: &Palette, edge: Rect, emphasis: f32) {
    let handle = Rect::from_center_size(
        pos2(edge.center().x, edge.max.y - 3.0),
        vec2(30.0 + 10.0 * emphasis, 3.0),
    );
    painter.rect_filled(
        handle,
        CornerRadius::same(2),
        palette
            .text_muted
            .gamma_multiply(0.25 + 0.45 * emphasis),
    );
}
