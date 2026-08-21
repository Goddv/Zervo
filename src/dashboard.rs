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
    pub const ALL: [WidgetKind; 3] = [
        WidgetKind::Clock,
        WidgetKind::NowPlaying,
        WidgetKind::Transport,
    ];

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
            WidgetKind::Clock => Size::cells(1, 1),
            WidgetKind::NowPlaying => Size::cells(3, 1),
            WidgetKind::Transport => Size::cells(2, 1),
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
    pub const CHOICES: [Size; 10] = [
        Size::cells(1, 1),
        Size::cells(2, 1),
        Size::cells(3, 1),
        Size::cells(4, 1),
        Size::cells(6, 1),
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
    Remove(usize),
    Resize(usize, Size),
    Swap { a: usize, b: usize },
    Place { index: usize, col: u8, row: u8 },
    Media(servo::MediaSessionActionType),
}

/// A cell is a fixed height and a share of the width. The column count is
/// constant, so a widget at column six is at column six in any window: the
/// cells narrow with the window rather than the grid losing columns, which
/// would push everything on the right into the same place.
const CELL_HEIGHT: f32 = 52.0;
pub const COLUMNS: u8 = 12;
const GAP: f32 = 10.0;
const PAD: f32 = 6.0;
/// The most rows the shelf will ever offer.
pub const MAX_ROWS: u8 = 4;

/// How wide one cell is in `area`.
fn cell_width(area: Rect) -> f32 {
    let inner = area.shrink(PAD).width();
    ((inner - (COLUMNS - 1) as f32 * GAP) / COLUMNS as f32).max(12.0)
}

/// The width `cells` columns come to.
fn extent_w(cells: u8, cell: f32) -> f32 {
    cells as f32 * cell + cells.saturating_sub(1) as f32 * GAP
}

/// The shelf height that shows `rows` whole rows. The bar's limits are derived
/// from this rather than guessed, so the tallest bar always fits a whole number
/// of rows instead of ending part-way through one.
pub const fn height_for_rows(rows: u8) -> f32 {
    rows as f32 * CELL_HEIGHT + (rows as f32 - 1.0) * GAP + PAD * 2.0
}

/// The height `cells` rows come to.
fn extent(cells: u8) -> f32 {
    cells as f32 * CELL_HEIGHT + cells.saturating_sub(1) as f32 * GAP
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
        .clamp(1, MAX_ROWS)
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
    let rows = MAX_ROWS;
    let width = match size.w {
        Span::Cells(n) => n.clamp(1, COLUMNS),
        Span::Full => COLUMNS,
    };
    let height = match size.h {
        Span::Cells(n) => n.clamp(1, rows),
        // `rows`, the same as `footprint` just below resolves it to, and not 1.
        // Resolving it to 1 here meant a full-height widget went looking for a
        // one-row hole and then occupied every row of the column it found —
        // landing on top of whatever was already under it.
        //
        // `rows_needed` does resolve `Full` to 1, and that one is right: it is
        // computing the very number `Full` resolves *to*, so it has to break
        // the circle somewhere, and a full-height widget contributing only its
        // own row is what makes it "as tall as the tallest thing beside it".
        Span::Full => rows,
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
    (((inner.height() + GAP) / (CELL_HEIGHT + GAP)).floor() as u8).clamp(1, MAX_ROWS)
}

fn grid_rows(placed: &[Placed], area: Rect) -> u8 {
    rows_needed(placed).max(visible_rows(area))
}

fn layout(placed: &[Placed], area: Rect) -> Vec<Rect> {
    let rows = grid_rows(placed, area);
    let inner = area.shrink(PAD);
    let cell = cell_width(area);

    placed
        .iter()
        .map(|widget| {
            let (col, row, width, height) = footprint(widget, COLUMNS, rows);
            Rect::from_min_size(
                pos2(
                    inner.min.x + col as f32 * (cell + GAP),
                    inner.min.y + row as f32 * (CELL_HEIGHT + GAP),
                ),
                vec2(extent_w(width, cell), extent(height)),
            )
        })
        .collect()
}

/// The cell the pointer is over, clamped so the widget stays on the grid.
fn cell_at(area: Rect, pos: egui::Pos2, size: Size, columns: u8, rows: u8) -> (u8, u8) {
    let inner = area.shrink(PAD);
    let cell = cell_width(area);
    let width = match size.w {
        Span::Cells(n) => n.clamp(1, columns),
        Span::Full => columns,
    };
    let height = match size.h {
        Span::Cells(n) => n.clamp(1, rows),
        Span::Full => rows,
    };
    let col = (((pos.x - inner.min.x) / (cell + GAP)).round() as i32)
        .clamp(0, columns.saturating_sub(width) as i32) as u8;
    let row = (((pos.y - inner.min.y) / (CELL_HEIGHT + GAP)).round() as i32)
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

    // The shelf sits just above the new tab page rather than on it, so on its
    // own it had nothing to frost against and came out flat while everything
    // below it was glass. Letting the page's picture reach up over the shelf
    // is what makes the two read as one surface — the sampler clamps, so what
    // arrives is the page's own top edge carried upward.
    let reaching = palette.reaching(area.height() + 24.0);
    let palette = &reaching;

    let ctx = root.ctx().clone();
    // Everything is clipped to the shelf, so a half-open one shows the top of
    // whole cards rather than squashed ones — it uncovers the strip rather
    // than compressing it.
    let mut root = root.new_child(egui::UiBuilder::new().max_rect(area));
    // Room for the shadows, which are painted outside their widgets. Clipping
    // tight to the shelf scissored them off along its edges, so a card in the
    // strip had three sides of shadow and one hard one. Sideways and upward
    // only: the bottom edge is where the strip is uncovered from, and letting
    // anything past it shows the cards before they have arrived.
    let room = glass::room(palette.radius(crate::theme::Tier::Card));
    root.set_clip_rect(Rect::from_min_max(
        pos2(area.min.x - room, area.min.y - room),
        pos2(area.max.x + room, area.max.y),
    ));
    let root = &mut root;

    let drag_id = Id::new("zervo_widget_drag");
    let sizes_id = Id::new("zervo_widget_sizes");
    let resize_id = Id::new("zervo_widget_resize");
    let resizing = ctx.data(|data| data.get_temp::<usize>(resize_id));
    let dragging = ctx.data(|data| data.get_temp::<usize>(drag_id));
    let slots = layout(placed, area);
    let pointer = ctx.input(|input| input.pointer.latest_pos());

    // The widget under the pointer is the one that would be swapped with.
    // Nothing under the pointer means nothing happens, which is easier to
    // predict than a drop that lands somewhere by proximity.
    let inner = area.shrink(PAD);
    let cell = cell_width(area);
    let columns = COLUMNS;
    let rows = grid_rows(placed, area);

    // Where a held widget would land: the cell under the pointer, snapped.
    // Free placement, so it can go anywhere on the shelf — including a row
    // with nothing above it.
    let snap = match (dragging, pointer) {
        (Some(held), Some(pos)) => placed.get(held).map(|widget| {
            let (col, row) = cell_at(
                area,
                pos - vec2(cell, CELL_HEIGHT) / 2.0,
                widget.size,
                columns,
                rows,
            );
            let (_, _, width, height) = footprint(
                &Placed {
                    col,
                    row,
                    ..*widget
                },
                columns,
                rows,
            );
            (
                col,
                row,
                Rect::from_min_size(
                    pos2(
                        inner.min.x + col as f32 * (cell + GAP),
                        inner.min.y + row as f32 * (CELL_HEIGHT + GAP),
                    ),
                    vec2(extent_w(width, cell), extent(height)),
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
        // Hover is taken from the pointer, not from the card's own response.
        // Registering the small controls on top of the card makes the card
        // itself stop counting as hovered, which hid them again on the very
        // next frame — they flickered, and could not be clicked.
        let over = pointer.is_some_and(|pos| slot.contains(pos));

        if response.drag_started() {
            ctx.data_mut(|data| data.insert_temp(drag_id, index));
        }
        if held && response.drag_stopped() {
            ctx.data_mut(|data| data.remove::<usize>(drag_id));
            match (target, snap) {
                (Some(over), _) => changes.push(Change::Swap { a: index, b: over }),
                (None, Some((col, row, _))) => changes.push(Change::Place { index, col, row }),
                _ => {},
            }
        }
        if over || held {
            ctx.set_cursor_icon(if held {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            });
        }

        let drawn = match (held, pointer) {
            (true, Some(pos)) => {
                Rect::from_min_size(pos - vec2(cell, CELL_HEIGHT) / 2.0, slot.size())
            },
            _ => *slot,
        };
        if held {
            held_later = Some((index, widget.kind, drawn));
        } else {
            draw_widget(
                root,
                palette,
                media,
                WidgetAt {
                    index,
                    kind: widget.kind,
                    rect: drawn,
                    look: Look::of(false, target == Some(index)),
                },
                &mut changes,
            );
        }

        // Remove and resize appear on hover, so they are never in the way.
        if over && !held {
            let close = Rect::from_center_size(
                pos2(drawn.max.x - 11.0, drawn.min.y + 11.0),
                vec2(16.0, 16.0),
            );
            icons::draw_icon(
                root.painter(),
                close.shrink(3.0),
                Icon::Close,
                palette.text_muted,
            );
            if root
                .interact(close, drag_id.with(("close", index)), Sense::click())
                .clicked()
            {
                changes.push(Change::Remove(index));
            }

            draw_resize_handle(root, palette, drawn);
        }

        // The corner both drags and clicks: drag to resize by cells, click for
        // the list of sizes. One affordance, because two in the same corner is
        // one too many.
        if over || resizing == Some(index) {
            let corner = Rect::from_center_size(
                pos2(slot.max.x - 11.0, slot.max.y - 11.0),
                vec2(20.0, 20.0),
            );
            let handle = root.interact(
                corner,
                drag_id.with(("size", index)),
                Sense::click_and_drag(),
            );
            if handle.hovered() || resizing == Some(index) {
                ctx.set_cursor_icon(CursorIcon::ResizeNwSe);
            }
            if handle.drag_started() {
                ctx.data_mut(|data| data.insert_temp(resize_id, index));
            }
            if handle.clicked() {
                ctx.data_mut(|data| data.insert_temp(sizes_id, index));
            }
            if resizing == Some(index)
                && let Some(pos) = pointer
            {
                let wanted = size_from_corner(*slot, pos, cell, columns, rows);
                let (_, _, width, height) = footprint(
                    &Placed {
                        size: wanted,
                        ..*widget
                    },
                    columns,
                    rows,
                );
                let preview =
                    Rect::from_min_size(slot.min, vec2(extent_w(width, cell), extent(height)));
                root.painter().rect_stroke(
                    preview,
                    CornerRadius::same(10),
                    Stroke::new(1.0_f32, palette.accent.gamma_multiply(0.85)),
                    StrokeKind::Inside,
                );
                if handle.drag_stopped() {
                    ctx.data_mut(|data| data.remove::<usize>(resize_id));
                    if wanted != widget.size {
                        changes.push(Change::Resize(index, wanted));
                    }
                }
            }
        }
    }

    if let Some((index, kind, drawn)) = held_later {
        draw_widget(
            root,
            palette,
            media,
            WidgetAt {
                index,
                kind,
                rect: drawn,
                look: Look::Held,
            },
            &mut changes,
        );
    }

    // ── The size menu, for whichever widget asked for one.
    if let Some(index) = ctx.data(|data| data.get_temp::<usize>(sizes_id))
        && let Some(slot) = slots.get(index)
    {
        match draw_size_menu(root, palette, *slot, placed[index].size) {
            Some(size) => {
                ctx.data_mut(|data| data.remove::<usize>(sizes_id));
                changes.push(Change::Resize(index, size));
            },
            // A press that did not land on the menu dismisses it, or it stays
            // up until something is picked.
            None if ctx.input(|input| input.pointer.any_pressed()) && !over_menu(&ctx) => {
                ctx.data_mut(|data| data.remove::<usize>(sizes_id));
            },
            None => {},
        }
    }

    changes
}

/// The size a corner drag is asking for, in whole cells.
fn size_from_corner(slot: Rect, pos: egui::Pos2, cell: f32, columns: u8, rows: u8) -> Size {
    let width =
        (((pos.x - slot.min.x + GAP) / (cell + GAP)).round() as i32).clamp(1, columns as i32);
    let height =
        (((pos.y - slot.min.y + GAP) / (CELL_HEIGHT + GAP)).round() as i32).clamp(1, rows as i32);
    Size::cells(width as u8, height as u8)
}

/// The mark in a card's corner that says it can be resized.
fn draw_resize_handle(root: &mut Ui, palette: &Palette, rect: Rect) {
    let corner = pos2(rect.max.x - 6.0, rect.max.y - 6.0);
    for step in 0..3 {
        let offset = step as f32 * 4.0;
        root.painter().line_segment(
            [
                pos2(corner.x - offset, corner.y),
                pos2(corner.x, corner.y - offset),
            ],
            Stroke::new(1.2_f32, palette.text_muted.gamma_multiply(0.7)),
        );
    }
}

/// Whether the pointer is over whichever menu is up, so a press elsewhere can
/// dismiss it.
pub fn over_menu(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<Rect>(Id::new("zervo_menu_rect")))
        .zip(ctx.input(|input| input.pointer.interact_pos()))
        .is_some_and(|(rect, pos)| rect.contains(pos))
}

/// A floating list, used for every menu the chrome draws itself: the shelf's
/// two, and the new tab page's.
/// One line of a floating list: something to pick, or a heading over the next
/// few things to pick.
pub enum Row<T> {
    Heading(&'static str),
    Item(String, T, bool),
}

impl<T> Row<T> {
    /// Headings are shorter than items — they carry small type and no hit
    /// target, so giving them a whole row leaves a hole in the list.
    fn height(&self) -> f32 {
        match self {
            Row::Heading(_) => 22.0,
            Row::Item(..) => 28.0,
        }
    }
}

pub fn menu<T: Copy>(
    root: &mut Ui,
    palette: &Palette,
    id: &str,
    anchor: Rect,
    width: f32,
    rows: &[Row<T>],
) -> Option<T> {
    let ctx = root.ctx().clone();
    let height: f32 = rows.iter().map(Row::height).sum();
    let rect = crate::ui::clamp_into(
        Rect::from_min_size(
            pos2(anchor.min.x, anchor.max.y + 6.0),
            vec2(width, height + 12.0),
        ),
        ctx.content_rect(),
    );
    ctx.data_mut(|data| data.insert_temp(Id::new("zervo_menu_rect"), rect));
    let palette = &palette.reaching(crate::ui::PANEL_REACH).over(rect);
    let mut chosen = None;
    egui::Area::new(Id::new(id))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(&ctx, |ui| {
            let painter = ui.painter();
            for shape in glass::shapes(
                rect,
                palette,
                Glass::of(crate::theme::Surface::Menu)
                    .opaque(palette.bg)
                    .border(palette.border),
            ) {
                painter.add(shape);
            }
            let mut y = rect.min.y + 6.0;
            for (index, entry) in rows.iter().enumerate() {
                let row = Rect::from_min_size(
                    pos2(rect.min.x + 6.0, y),
                    vec2(rect.width() - 12.0, entry.height()),
                );
                y += entry.height();
                match entry {
                    Row::Heading(label) => {
                        ui.painter().text(
                            pos2(row.min.x + 8.0, row.center().y + 2.0),
                            Align2::LEFT_CENTER,
                            label.to_uppercase(),
                            FontId::proportional(10.0),
                            palette.text_muted,
                        );
                    },
                    Row::Item(label, value, current) => {
                        let response = ui.interact(row, Id::new(id).with(index), Sense::click());
                        if response.hovered() {
                            ui.painter().rect_filled(
                                row,
                                CornerRadius::same(palette.radius(crate::theme::Tier::Control)),
                                palette.surface_hover,
                            );
                        }
                        ui.painter().text(
                            pos2(row.min.x + 8.0, row.center().y),
                            Align2::LEFT_CENTER,
                            label,
                            FontId::proportional(13.0),
                            if *current {
                                palette.accent
                            } else {
                                palette.text
                            },
                        );
                        if *current {
                            icons::draw_icon(
                                ui.painter(),
                                Rect::from_center_size(
                                    pos2(row.max.x - 12.0, row.center().y),
                                    vec2(11.0, 11.0),
                                ),
                                Icon::Check,
                                palette.accent,
                            );
                        }
                        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                            chosen = Some(*value);
                        }
                    },
                }
            }
            ui.advance_cursor_after_rect(rect);
        });
    chosen
}

pub fn add_menu(root: &mut Ui, palette: &Palette, anchor: Rect) -> Option<WidgetKind> {
    let rows: Vec<Row<WidgetKind>> = WidgetKind::ALL
        .iter()
        .map(|kind| Row::Item(kind.label().to_owned(), *kind, false))
        .collect();
    menu(
        root,
        palette,
        "zervo_widget_menu_area",
        anchor,
        170.0,
        &rows,
    )
}

fn draw_size_menu(root: &mut Ui, palette: &Palette, anchor: Rect, current: Size) -> Option<Size> {
    let rows: Vec<Row<Size>> = Size::CHOICES
        .iter()
        .map(|size| Row::Item(size.label(), *size, *size == current))
        .collect();
    menu(
        root,
        palette,
        "zervo_widget_size_area",
        anchor,
        120.0,
        &rows,
    )
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

/// One widget as the shelf is about to draw it.
///
/// `index` is the position in `Settings::navbar_widgets`, and it is the
/// widget's identity: anything interactive *inside* a widget hangs its egui id
/// on it. The transport's three buttons used to key theirs on
/// `rect.min.x as i32` instead, which is wrong twice over — two widgets of the
/// same width stacked in one column share an x and so share ids, and a widget
/// being dragged has a new x every frame, so its buttons lost their hover state
/// on each one.
struct WidgetAt {
    index: usize,
    kind: WidgetKind,
    rect: Rect,
    look: Look,
}

fn draw_widget(
    root: &mut Ui,
    palette: &Palette,
    media: &Media,
    at: WidgetAt,
    changes: &mut Vec<Change>,
) {
    let WidgetAt {
        index: slot_index,
        kind,
        rect,
        look,
    } = at;
    let painter = root.painter();
    // Opaque, with the shadow left on: these stack over the content card, and
    // a translucent card in a pile does not read as a card.
    let material = match look {
        Look::Held => Glass::tier(crate::theme::Tier::Card),
        // Lit, so it is obvious which card is being traded with.
        Look::Target => Glass::tier(crate::theme::Tier::Card).tint(crate::theme::mix(
            palette.surface,
            palette.accent,
            0.30,
        )),
        Look::Normal => Glass::tier(crate::theme::Tier::Card).strength(0.85),
    }
    .opaque(palette.bg);
    for shape in glass::shapes(rect, palette, material) {
        painter.add(shape);
    }

    // A one-cell widget has no room for two lines, so they show less rather
    // than overflowing.
    let roomy = rect.height() > CELL_HEIGHT * 1.4;

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
                    if media.playing {
                        Icon::Pause
                    } else {
                        Icon::Play
                    },
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
                    Id::new("zervo_transport").with((slot_index, index)),
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
                if !media.is_idle() && response.on_hover_cursor(CursorIcon::PointingHand).clicked()
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
        palette.text_muted.gamma_multiply(0.25 + 0.45 * emphasis),
    );
}
