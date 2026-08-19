//! The widget shelf that a taller navigation bar uncovers.
//!
//! Widgets sit on a grid. A widget is some number of cells wide and some
//! number tall, or spans the shelf entirely in either direction, and is packed
//! into the first place it fits. Sizes and order are remembered in the
//! settings.
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

    /// What it is placed at before anyone resizes it.
    pub fn default_size(self) -> Size {
        match self {
            WidgetKind::Clock => Size::cells(2, 1),
            WidgetKind::NowPlaying => Size::cells(4, 1),
            WidgetKind::Transport => Size::cells(3, 1),
        }
    }
}

/// One dimension of a widget: a number of cells, or the whole shelf.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Span {
    Cells(u8),
    Full,
}

impl Span {
    fn label(self) -> String {
        match self {
            Span::Cells(n) => n.to_string(),
            Span::Full => "full".to_owned(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Size {
    pub w: Span,
    pub h: Span,
}

impl Size {
    pub const fn cells(w: u8, h: u8) -> Self {
        Size {
            w: Span::Cells(w),
            h: Span::Cells(h),
        }
    }

    pub fn label(self) -> String {
        format!("{}×{}", self.w.label(), self.h.label())
    }

    /// The sizes offered in the menu. Not every combination — a list nobody
    /// can scan is worse than a short one, and the grid takes any pair.
    pub const CHOICES: [Size; 9] = [
        Size::cells(1, 1),
        Size::cells(2, 1),
        Size::cells(3, 1),
        Size::cells(4, 1),
        Size::cells(2, 2),
        Size::cells(4, 2),
        Size {
            w: Span::Full,
            h: Span::Cells(1),
        },
        Size {
            w: Span::Full,
            h: Span::Cells(2),
        },
        Size {
            w: Span::Cells(2),
            h: Span::Full,
        },
    ];
}

/// A widget placed on the shelf.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Placed {
    pub kind: WidgetKind,
    pub size: Size,
    /// Where it sits on the grid. Explicit rather than packed, so a widget can
    /// be put at the bottom of the shelf without having to fill the rows above
    /// it first.
    #[serde(default)]
    pub col: u8,
    #[serde(default)]
    pub row: u8,
}

impl Placed {
    pub fn new(kind: WidgetKind) -> Self {
        Placed {
            kind,
            size: kind.default_size(),
            col: 0,
            row: 0,
        }
    }

    pub fn defaults() -> Vec<Placed> {
        let mut placed: Vec<Placed> = Vec::new();
        for kind in WidgetKind::ALL {
            let mut widget = Placed::new(kind);
            (widget.col, widget.row) = free_cell(&placed, widget.size);
            placed.push(widget);
        }
        placed
    }
}

/// What the shelf wants done, handed back rather than applied here so the
/// settings stay the one place that owns the arrangement.
pub enum Change {
    Add(WidgetKind),
    Remove(usize),
    Resize(usize, Size),
    Swap { a: usize, b: usize },
    Place { index: usize, col: u8, row: u8 },
    Media(servo::MediaSessionActionType),
}

/// One grid cell, and the space between cells.
const CELL: f32 = 52.0;
const GAP: f32 = 10.0;
const PAD: f32 = 6.0;

fn extent(cells: u8) -> f32 {
    cells as f32 * CELL + cells.saturating_sub(1) as f32 * GAP
}

/// How many rows the arrangement needs. A `Full` height resolves to this, so a
/// full-height widget is as tall as the tallest thing beside it.
fn rows_needed(placed: &[Placed]) -> u8 {
    placed
        .iter()
        .map(|widget| {
            widget.row
                + match widget.size.h {
                    Span::Cells(n) => n.max(1),
                    Span::Full => 1,
                }
        })
        .max()
        .unwrap_or(1)
        .clamp(1, 4)
}

/// Cells a widget of `size` occupies at a position, for collision checks.
fn footprint(widget: &Placed, columns: u8, rows: u8) -> (u8, u8, u8, u8) {
    let width = match widget.size.w {
        Span::Cells(n) => n.clamp(1, columns),
        Span::Full => columns,
    };
    let height = match widget.size.h {
        Span::Cells(n) => n.clamp(1, rows),
        Span::Full => rows,
    };
    (
        widget.col.min(columns.saturating_sub(width)),
        widget.row.min(rows.saturating_sub(height)),
        width,
        height,
    )
}

/// The first cell a new widget fits in, so adding one does not drop it on top
/// of something else.
pub fn free_cell(placed: &[Placed], size: Size) -> (u8, u8) {
    const COLUMNS: u8 = 12;
    let rows = 4;
    let width = match size.w {
        Span::Cells(n) => n.clamp(1, COLUMNS),
        Span::Full => COLUMNS,
    };
    let height = match size.h {
        Span::Cells(n) => n.clamp(1, rows),
        Span::Full => 1,
    };
    for row in 0..=rows.saturating_sub(height) {
        'next: for col in 0..=COLUMNS.saturating_sub(width) {
            for other in placed {
                let (ocol, orow, owidth, oheight) = footprint(other, COLUMNS, rows);
                let overlaps = col < ocol + owidth
                    && ocol < col + width
                    && row < orow + oheight
                    && orow < row + height;
                if overlaps {
                    continue 'next;
                }
            }
            return (col, row);
        }
    }
    (0, 0)
}

/// The bar height at which the shelf shows every row of this arrangement.
pub fn open_height(placed: &[Placed]) -> f32 {
    extent(rows_needed(placed)) + PAD * 2.0
}

/// Where each widget sits, from its own coordinates. Nothing is packed: a
/// widget stays where it was put.
fn visible_rows(area: Rect) -> u8 {
    let inner = area.shrink(PAD);
    (((inner.height() + GAP) / (CELL + GAP)).floor() as u8).clamp(1, 4)
}

fn grid_rows(placed: &[Placed], area: Rect) -> u8 {
    rows_needed(placed).max(visible_rows(area))
}

fn layout(placed: &[Placed], area: Rect) -> Vec<Rect> {
    let rows = grid_rows(placed, area);
    let inner = area.shrink(PAD);
    let columns = (((inner.width() + GAP) / (CELL + GAP)).floor() as u8).max(1);

    placed
        .iter()
        .map(|widget| {
            let (col, row, width, height) = footprint(widget, columns, rows);
            Rect::from_min_size(
                pos2(
                    inner.min.x + col as f32 * (CELL + GAP),
                    inner.min.y + row as f32 * (CELL + GAP),
                ),
                vec2(extent(width), extent(height)),
            )
        })
        .collect()
}

/// The cell the pointer is over, clamped so the widget stays on the grid.
fn cell_at(area: Rect, pos: egui::Pos2, size: Size, columns: u8, rows: u8) -> (u8, u8) {
    let inner = area.shrink(PAD);
    let width = match size.w {
        Span::Cells(n) => n.clamp(1, columns),
        Span::Full => columns,
    };
    let height = match size.h {
        Span::Cells(n) => n.clamp(1, rows),
        Span::Full => rows,
    };
    let col = (((pos.x - inner.min.x) / (CELL + GAP)).round() as i32)
        .clamp(0, columns.saturating_sub(width) as i32) as u8;
    let row = (((pos.y - inner.min.y) / (CELL + GAP)).round() as i32)
        .clamp(0, rows.saturating_sub(height) as i32) as u8;
    (col, row)
}

/// Draw the shelf into `area`. Returns whatever the user asked for.
pub fn draw(
    root: &mut Ui,
    palette: &Palette,
    media: &Media,
    placed: &[Placed],
    area: Rect,
) -> Vec<Change> {
    let mut changes = Vec::new();
    if area.height() < 20.0 {
        return changes;
    }

    let ctx = root.ctx().clone();
    // Everything is clipped to the shelf, so a half-open one shows the top of
    // whole cards rather than squashed ones — it uncovers the strip rather
    // than compressing it.
    let mut root = root.new_child(egui::UiBuilder::new().max_rect(area));
    root.set_clip_rect(area);
    let root = &mut root;

    let drag_id = Id::new("zervo_widget_drag");
    let sizes_id = Id::new("zervo_widget_sizes");
    let dragging = ctx.data(|data| data.get_temp::<usize>(drag_id));
    let slots = layout(placed, area);
    let pointer = ctx.input(|input| input.pointer.latest_pos());

    // The widget under the pointer is the one that would be swapped with.
    // Nothing under the pointer means nothing happens, which is easier to
    // predict than a drop that lands somewhere by proximity.
    let inner = area.shrink(PAD);
    let columns = (((inner.width() + GAP) / (CELL + GAP)).floor() as u8).max(1);
    let rows = grid_rows(placed, area);

    // Where a held widget would land: the cell under the pointer, snapped.
    // Free placement, so it can go anywhere on the shelf — including a row
    // with nothing above it.
    let snap = match (dragging, pointer) {
        (Some(held), Some(pos)) => placed.get(held).map(|widget| {
            let (col, row) = cell_at(area, pos - vec2(CELL, CELL) / 2.0, widget.size, columns, rows);
            let (_, _, width, height) = footprint(
                &Placed { col, row, ..*widget },
                columns,
                rows,
            );
            (
                col,
                row,
                Rect::from_min_size(
                    pos2(
                        inner.min.x + col as f32 * (CELL + GAP),
                        inner.min.y + row as f32 * (CELL + GAP),
                    ),
                    vec2(extent(width), extent(height)),
                ),
            )
        }),
        _ => None,
    };

    // Anything already sitting where it would land gets traded with.
    let target = match (dragging, snap) {
        (Some(held), Some((col, row, _))) => {
            let (_, _, width, height) = footprint(
                &Placed {
                    col,
                    row,
                    ..placed[held]
                },
                columns,
                rows,
            );
            placed.iter().enumerate().position(|(index, other)| {
                if index == held {
                    return false;
                }
                let (ocol, orow, owidth, oheight) = footprint(other, columns, rows);
                col < ocol + owidth
                    && ocol < col + width
                    && row < orow + oheight
                    && orow < row + height
            })
        },
        _ => None,
    };

    // The surface the widget would take, outlined with a thin line. Shown
    // wherever it would land, whether or not anything is there.
    if let Some((_, _, destination)) = snap {
        root.painter().rect_filled(
            destination,
            CornerRadius::same(10),
            palette.accent.gamma_multiply(0.08),
        );
        root.painter().rect_stroke(
            destination,
            CornerRadius::same(10),
            Stroke::new(1.0_f32, palette.accent.gamma_multiply(0.85)),
            StrokeKind::Inside,
        );
    }

    let mut held_later = None;
    for (index, (widget, slot)) in placed.iter().zip(slots.iter()).enumerate() {
        let response = root.interact(*slot, drag_id.with(index), Sense::click_and_drag());
        let held = dragging == Some(index);

        if response.drag_started() {
            ctx.data_mut(|data| data.insert_temp(drag_id, index));
        }
        if held && response.drag_stopped() {
            ctx.data_mut(|data| data.remove::<usize>(drag_id));
            match (target, snap) {
                (Some(over), _) => changes.push(Change::Swap { a: index, b: over }),
                (None, Some((col, row, _))) => {
                    changes.push(Change::Place { index, col, row })
                },
                _ => {},
            }
        }
        if response.hovered() || held {
            ctx.set_cursor_icon(if held {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            });
        }

        let drawn = match (held, pointer) {
            (true, Some(pos)) => Rect::from_min_size(
                pos - vec2(CELL, CELL) / 2.0,
                slot.size(),
            ),
            _ => *slot,
        };
        if held {
            held_later = Some((widget.kind, drawn));
        } else {
            draw_widget(
                root,
                palette,
                media,
                widget.kind,
                drawn,
                Look::of(false, target == Some(index)),
                &mut changes,
            );
        }

        // Remove and resize appear on hover, so they are never in the way.
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

            let resize = Rect::from_center_size(
                pos2(drawn.max.x - 11.0, drawn.max.y - 11.0),
                vec2(16.0, 16.0),
            );
            icons::draw_icon(
                root.painter(),
                resize.shrink(3.0),
                Icon::Sliders,
                palette.text_muted,
            );
            if root
                .interact(resize, drag_id.with(("size", index)), Sense::click())
                .on_hover_text(format!("Size — {}", widget.size.label()))
                .clicked()
            {
                ctx.data_mut(|data| data.insert_temp(sizes_id, index));
            }
        }
    }

    if let Some((kind, drawn)) = held_later {
        draw_widget(root, palette, media, kind, drawn, Look::Held, &mut changes);
    }

    // ── The size menu, for whichever widget asked for one.
    if let Some(index) = ctx.data(|data| data.get_temp::<usize>(sizes_id))
        && let Some(slot) = slots.get(index)
        && let Some(size) = draw_size_menu(root, palette, *slot, placed[index].size)
    {
        ctx.data_mut(|data| data.remove::<usize>(sizes_id));
        changes.push(Change::Resize(index, size));
    }

    // ── The add tile, after the last widget.
    let inner = area.shrink(PAD);
    let after = slots.iter().map(|slot| slot.max.x).fold(inner.min.x, f32::max);
    let add = Rect::from_min_size(
        pos2(
            if slots.is_empty() { inner.min.x } else { after + GAP },
            inner.min.y,
        ),
        vec2(38.0, CELL),
    );
    if add.max.x <= inner.max.x {
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

/// A floating list, used for both menus.
fn menu<T: Copy>(
    root: &mut Ui,
    palette: &Palette,
    id: &str,
    anchor: Rect,
    width: f32,
    rows: &[(String, T, bool)],
) -> Option<T> {
    const ROW: f32 = 28.0;
    let ctx = root.ctx().clone();
    let rect = crate::ui::clamp_into(
        Rect::from_min_size(
            pos2(anchor.min.x, anchor.max.y + 6.0),
            vec2(width, rows.len() as f32 * ROW + 12.0),
        ),
        ctx.content_rect(),
    );
    let mut chosen = None;
    egui::Area::new(Id::new(id))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(&ctx, |ui| {
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
            for (index, (label, value, current)) in rows.iter().enumerate() {
                let row = Rect::from_min_size(
                    pos2(rect.min.x + 6.0, rect.min.y + 6.0 + index as f32 * ROW),
                    vec2(rect.width() - 12.0, ROW),
                );
                let response = ui.interact(row, Id::new(id).with(index), Sense::click());
                if response.hovered() {
                    ui.painter()
                        .rect_filled(row, CornerRadius::same(7), palette.surface_hover);
                }
                ui.painter().text(
                    pos2(row.min.x + 8.0, row.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(13.0),
                    if *current { palette.accent } else { palette.text },
                );
                if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    chosen = Some(*value);
                }
            }
            ui.advance_cursor_after_rect(rect);
        });
    chosen
}

fn draw_add_menu(root: &mut Ui, palette: &Palette, anchor: Rect) -> Option<WidgetKind> {
    let rows: Vec<(String, WidgetKind, bool)> = WidgetKind::ALL
        .iter()
        .map(|kind| (kind.label().to_owned(), *kind, false))
        .collect();
    menu(root, palette, "zervo_widget_menu_area", anchor, 170.0, &rows)
}

fn draw_size_menu(root: &mut Ui, palette: &Palette, anchor: Rect, current: Size) -> Option<Size> {
    let rows: Vec<(String, Size, bool)> = Size::CHOICES
        .iter()
        .map(|size| (size.label(), *size, *size == current))
        .collect();
    menu(root, palette, "zervo_widget_size_area", anchor, 120.0, &rows)
}

/// How a card is drawn: ordinary, carried by the pointer, or the one a drop
/// would trade places with.
#[derive(Clone, Copy, PartialEq)]
enum Look {
    Normal,
    Held,
    Target,
}

impl Look {
    fn of(held: bool, target: bool) -> Self {
        match (held, target) {
            (true, _) => Look::Held,
            (_, true) => Look::Target,
            _ => Look::Normal,
        }
    }
}

fn draw_widget(
    root: &mut Ui,
    palette: &Palette,
    media: &Media,
    kind: WidgetKind,
    rect: Rect,
    look: Look,
    changes: &mut Vec<Change>,
) {
    let painter = root.painter();
    // Opaque, with the shadow left on: these stack over the content card, and
    // a translucent card in a pile does not read as a card.
    painter.rect_filled(rect, CornerRadius::same(10), palette.bg);
    let material = match look {
        Look::Held => Glass::new(10),
        // Lit, so it is obvious which card is being traded with.
        Look::Target => Glass::new(10).tint(crate::theme::mix(
            palette.surface,
            palette.accent,
            0.30,
        )),
        Look::Normal => Glass::new(10).strength(0.85),
    };
    for shape in glass::shapes(rect, palette, material) {
        painter.add(shape);
    }

    // A one-cell widget has no room for two lines, so they show less rather
    // than overflowing.
    let roomy = rect.height() > CELL * 1.4;

    match kind {
        WidgetKind::Clock => {
            let now = chrono::Local::now();
            painter.text(
                pos2(
                    rect.center().x,
                    rect.center().y - if roomy { 12.0 } else { 7.0 },
                ),
                Align2::CENTER_CENTER,
                now.format("%H:%M").to_string(),
                FontId::proportional(if roomy { 30.0 } else { 21.0 }),
                palette.text,
            );
            painter.text(
                pos2(
                    rect.center().x,
                    rect.center().y + if roomy { 16.0 } else { 12.0 },
                ),
                Align2::CENTER_CENTER,
                now.format("%a %-d %b").to_string(),
                FontId::proportional(11.5),
                palette.text_muted,
            );
            root.ctx()
                .request_repaint_after(std::time::Duration::from_secs(20));
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
            // Text fits the card it was given rather than a fixed guess.
            let chars = ((rect.width() - 24.0) / 7.0) as usize;
            painter.text(
                pos2(rect.min.x + 12.0, rect.min.y + 16.0),
                Align2::LEFT_CENTER,
                crate::ui::ellipsize(&media.title, chars),
                FontId::proportional(13.0),
                palette.text,
            );
            painter.text(
                pos2(rect.min.x + 12.0, rect.min.y + 33.0),
                Align2::LEFT_CENTER,
                crate::ui::ellipsize(&media.artist, chars),
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
                    root.painter()
                        .rect_filled(hit, CornerRadius::same(8), palette.surface_hover);
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
                if !media.is_idle() && response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
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
        palette.text_muted.gamma_multiply(0.25 + 0.45 * emphasis),
    );
}
