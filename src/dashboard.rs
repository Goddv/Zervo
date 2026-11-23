//! The widget shelf at the foot of the sidebar.
//!
//! Widgets fill the shelf width and can occupy more than one row.
//! Their positions and heights are saved in the settings.
//!
//! The media widgets are not mock-ups: Servo reports media session metadata,
//! playback state and position, and takes play/pause/track actions back, so
//! they drive whatever the page is playing.

use crate::l10n::text as tr;
use egui::{
    Align2, CornerRadius, CursorIcon, FontId, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use serde::{Deserialize, Serialize};

use crate::glass::{self, Glass};
use crate::icons::{self, Icon};
use crate::theme::{Palette, Tier};

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
            WidgetKind::Clock => tr("widgets-clock"),
            WidgetKind::NowPlaying => tr("common-now-playing"),
            WidgetKind::Transport => tr("widgets-player-controls"),
        }
    }
}

/// Shelf height as stored by current and older releases.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
enum Span {
    Cells(u8),
    Full,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Size {
    h: Span,
}

impl Size {
    const fn rows(rows: u8) -> Self {
        Self {
            h: Span::Cells(rows),
        }
    }

    fn height(self) -> u8 {
        match self.h {
            Span::Cells(rows) => rows.clamp(1, MAX_ROWS),
            Span::Full => 1,
        }
    }

    const CHOICES: [Self; 4] = [Self::rows(1), Self::rows(2), Self::rows(3), Self::rows(4)];
}

/// A widget placed on the shelf.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Placed {
    pub kind: WidgetKind,
    pub size: Size,
    /// Where it sits on the grid. Explicit rather than packed, so a widget can
    /// be put at the bottom of the shelf without having to fill the rows above
    /// it first.
    #[serde(default, rename = "col", skip_serializing)]
    legacy_column: u8,
    #[serde(default)]
    pub row: u8,
}

impl Placed {
    pub fn new(kind: WidgetKind) -> Self {
        Placed {
            kind,
            size: Size::rows(1),
            legacy_column: 0,
            row: 0,
        }
    }

    pub fn defaults() -> Vec<Placed> {
        let mut placed: Vec<Placed> = Vec::new();
        for kind in WidgetKind::ALL {
            let mut widget = Placed::new(kind);
            if let Some(row) = free_row(&placed) {
                widget.row = row;
                placed.push(widget);
            }
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
    Place { index: usize, row: u8 },
    Media(servo::MediaSessionActionType),
}

const CELL_HEIGHT: f32 = 52.0;
const GAP: f32 = 10.0;
const PAD: f32 = 6.0;
const MAX_ROWS: u8 = 4;
pub const MAX_HEIGHT: f32 = height_for_rows(MAX_ROWS);

const fn height_for_rows(rows: u8) -> f32 {
    extent(rows) + PAD * 2.0
}

const fn extent(rows: u8) -> f32 {
    rows as f32 * CELL_HEIGHT + rows.saturating_sub(1) as f32 * GAP
}

fn rows_needed(placed: &[Placed]) -> u8 {
    placed
        .iter()
        .map(|widget| widget.row.saturating_add(widget.size.height()))
        .max()
        .unwrap_or(1)
        .clamp(1, MAX_ROWS)
}

fn overlaps(row: u8, height: u8, other: &Placed) -> bool {
    row < other.row.saturating_add(other.size.height()) && other.row < row.saturating_add(height)
}

/// Stack legacy horizontal widgets in reading order.
pub fn normalize_layout(placed: &mut [Placed]) {
    let needs_reflow = placed.iter().any(|widget| widget.legacy_column != 0)
        || placed.iter().enumerate().any(|(index, widget)| {
            placed[..index]
                .iter()
                .any(|other| overlaps(widget.row, widget.size.height(), other))
        });
    if needs_reflow {
        let mut order: Vec<usize> = (0..placed.len()).collect();
        order.sort_by_key(|index| (placed[*index].row, placed[*index].legacy_column));
        let mut next_row = 0_u8;
        for index in order {
            let widget = &mut placed[index];
            widget.row = next_row;
            next_row = next_row.saturating_add(widget.size.height());
        }
    }
    for widget in placed {
        widget.legacy_column = 0;
        widget.size = Size::rows(widget.size.height());
    }
}

/// Insert one-row widgets into the first available row.
pub fn free_row(placed: &[Placed]) -> Option<u8> {
    (0..MAX_ROWS).find(|row| placed.iter().all(|widget| !overlaps(*row, 1, widget)))
}

fn max_widget_height(placed: &[Placed], index: usize) -> u8 {
    let row = placed[index].row;
    placed
        .iter()
        .filter(|widget| widget.row > row)
        .map(|widget| widget.row)
        .min()
        .unwrap_or(MAX_ROWS)
        .min(MAX_ROWS)
        .saturating_sub(row)
}

pub fn resize_widget(placed: &mut [Placed], index: usize, size: Size) -> bool {
    if index >= placed.len() || size.height() > max_widget_height(placed, index) {
        return false;
    }
    placed[index].size = size;
    true
}

pub fn swap_widgets(placed: &mut [Placed], a: usize, b: usize) -> bool {
    let (Some(first), Some(second)) = (placed.get(a), placed.get(b)) else {
        return false;
    };
    let first = Placed {
        row: second.row,
        ..*first
    };
    let second = Placed {
        row: placed[a].row,
        ..*second
    };
    if first.row.saturating_add(first.size.height()) > MAX_ROWS
        || second.row.saturating_add(second.size.height()) > MAX_ROWS
        || overlaps(first.row, first.size.height(), &second)
        || placed.iter().enumerate().any(|(index, other)| {
            index != a
                && index != b
                && (overlaps(first.row, first.size.height(), other)
                    || overlaps(second.row, second.size.height(), other))
        })
    {
        return false;
    }
    placed[a] = first;
    placed[b] = second;
    true
}

pub fn open_height(placed: &[Placed]) -> f32 {
    height_for_rows(rows_needed(placed))
}

fn widget_rect(inner: Rect, row: u8, height: u8) -> Rect {
    Rect::from_min_size(
        pos2(
            inner.left(),
            inner.top() + f32::from(row) * (CELL_HEIGHT + GAP),
        ),
        vec2(inner.width(), extent(height)),
    )
}

/// Draw the menu shared by the empty slot and the shelf handle.
pub fn widget_menu(ui: &mut Ui, placed: &[Placed]) -> Option<WidgetKind> {
    let has_room = free_row(placed).is_some();
    WidgetKind::ALL
        .into_iter()
        .find(|kind| {
            ui.add_enabled(has_room, egui::Button::new(kind.label()))
                .clicked()
        })
        .inspect(|_| ui.close())
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

    let drag_id = Id::unique("zervo_widget_drag");
    let sizes_id = Id::unique("zervo_widget_sizes");
    let resize_id = Id::unique("zervo_widget_resize");
    let resizing = ctx.data(|data| data.get_temp::<usize>(resize_id));
    let dragging = ctx.data(|data| data.get_temp::<usize>(drag_id));
    let pointer = ctx.input(|input| input.pointer.latest_pos());
    let inner = area.shrink(PAD);
    let slots: Vec<_> = placed
        .iter()
        .map(|widget| widget_rect(inner, widget.row, widget.size.height()))
        .collect();
    let grab_offset = vec2(inner.width(), CELL_HEIGHT) / 2.0;
    let visible_rows =
        (((inner.height() + GAP) / (CELL_HEIGHT + GAP)).floor() as u8).clamp(1, MAX_ROWS);
    let rows = rows_needed(placed).max(visible_rows);

    let snap = match (dragging, pointer) {
        (Some(held), Some(pos)) => placed.get(held).map(|widget| {
            let height = widget.size.height();
            let row = (((pos.y - grab_offset.y - inner.top()) / (CELL_HEIGHT + GAP)).round() as i32)
                .clamp(0, i32::from(rows.saturating_sub(height))) as u8;
            (row, widget_rect(inner, row, height))
        }),
        _ => None,
    };
    let target = match (dragging, snap) {
        (Some(held), Some((row, _))) => {
            let height = placed[held].size.height();
            placed
                .iter()
                .enumerate()
                .position(|(index, other)| index != held && overlaps(row, height, other))
        },
        _ => None,
    };

    if let Some((_, destination)) = snap {
        root.painter().rect_filled(
            destination,
            palette.corner(Tier::Card),
            palette.accent.gamma_multiply(0.08),
        );
        root.painter().rect_stroke(
            destination,
            palette.corner(Tier::Card),
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
                (None, Some((row, _))) => changes.push(Change::Place { index, row }),
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
            (true, Some(pos)) => Rect::from_min_size(pos - grab_offset, slot.size()),
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
                ctx.set_cursor_icon(CursorIcon::ResizeVertical);
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
                let max = max_widget_height(placed, index).max(1);
                let height = (((pos.y - slot.top() + GAP) / (CELL_HEIGHT + GAP)).round() as i32)
                    .clamp(1, i32::from(max)) as u8;
                let wanted = Size::rows(height);
                let preview = widget_rect(inner, widget.row, height);
                root.painter().rect_stroke(
                    preview,
                    palette.corner(Tier::Card),
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

    if dragging.is_none()
        && let Some(row) = free_row(placed).filter(|row| *row < visible_rows)
    {
        let slot = widget_rect(inner, row, 1);
        let response = root.put(slot, egui::Button::new(tr("chrome-add-a-widget")));
        if let Some(menu) = crate::overlay_glass::popup(egui::Popup::menu(&response), &ctx, |ui| {
            widget_menu(ui, placed)
        }) && let Some(kind) = menu.inner
        {
            changes.push(Change::Add(kind));
        }
    }

    // ── The size menu, for whichever widget asked for one.
    if let Some(index) = ctx.data(|data| data.get_temp::<usize>(sizes_id))
        && let Some(slot) = slots.get(index)
    {
        match draw_size_menu(
            root,
            palette,
            *slot,
            placed[index].size,
            max_widget_height(placed, index),
        ) {
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
    ctx.data(|data| data.get_temp::<Rect>(Id::unique("zervo_menu_rect")))
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
    let rect = crate::ui::anchored_popup(
        anchor,
        vec2(width, height + 12.0),
        ctx.content_rect(),
        false,
    );
    ctx.data_mut(|data| data.insert_temp(Id::unique("zervo_menu_rect"), rect));
    let mut chosen = None;
    egui::Area::new(Id::unique(id))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(&ctx, |ui| {
            let surface = crate::overlay_glass::Surface::begin(ui);
            let fallback = palette.reaching(crate::ui::PANEL_REACH).over(rect);
            let palette = if surface.is_native() {
                palette
            } else {
                &fallback
            };
            if !surface.is_native() {
                glass::paint(
                    ui.painter(),
                    rect,
                    palette,
                    Glass::of(crate::theme::Surface::Menu)
                        .opaque(palette.bg)
                        .border(palette.border),
                );
            }
            crate::overlay_glass::scroll_body(ui, rect.shrink(6.0), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for entry in rows {
                    let (row, response) = ui.allocate_exact_size(
                        vec2(ui.available_width(), entry.height()),
                        if matches!(entry, Row::Heading(_)) {
                            Sense::hover()
                        } else {
                            Sense::click()
                        },
                    );
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
                            if response.hovered() {
                                ui.painter().rect_filled(
                                    row,
                                    CornerRadius::same(palette.radius(crate::theme::Tier::Control)),
                                    palette.surface_hover,
                                );
                            }
                            crate::overlay_glass::row_label(
                                ui,
                                row.with_min_x(row.left() + 8.0)
                                    .with_max_x(row.right() - 22.0),
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
            });
            surface.finish(ui, rect, crate::overlay_glass::Role::Menu);
            ui.advance_cursor_after_rect(rect);
        });
    chosen
}

fn draw_size_menu(
    root: &mut Ui,
    palette: &Palette,
    anchor: Rect,
    current: Size,
    max: u8,
) -> Option<Size> {
    let rows: Vec<Row<Size>> = Size::CHOICES
        .iter()
        .take(usize::from(max))
        .enumerate()
        .map(|(index, size)| {
            Row::Item(
                crate::l10n::message("widgets-row-height", [("count", (index + 1).into())]),
                *size,
                *size == current,
            )
        })
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
/// `index` is the position in `Settings::sidebar_widgets`, and it is the
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
            let now = jiff::ZonedDateTime::now();
            let show_date = rect.width() >= 64.0;
            let size = ((rect.width() - 8.0) / 3.0).clamp(8.0, if roomy { 30.0 } else { 21.0 });
            let offset = if !show_date {
                0.0
            } else if roomy {
                12.0
            } else {
                7.0
            };
            painter.text(
                pos2(rect.center().x, rect.center().y - offset),
                Align2::CENTER_CENTER,
                now.strftime("%H:%M").to_string(),
                FontId::proportional(size),
                palette.text,
            );
            if show_date {
                painter.text(
                    pos2(
                        rect.center().x,
                        rect.center().y + if roomy { 16.0 } else { 12.0 },
                    ),
                    Align2::CENTER_CENTER,
                    now.strftime("%a %-d %b").to_string(),
                    FontId::proportional(11.5),
                    palette.text_muted,
                );
            }
            root.ctx()
                .request_repaint_after(std::time::Duration::from_secs(20));
        },
        WidgetKind::NowPlaying => {
            if media.is_idle() {
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    tr("widgets-nothing-playing"),
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
                painter.rect_filled(track, palette.corner(Tier::Hairline), palette.border);
                let played = (media.position / media.duration).clamp(0.0, 1.0) as f32;
                painter.rect_filled(
                    Rect::from_min_size(track.min, vec2(track.width() * played, track.height())),
                    palette.corner(Tier::Hairline),
                    palette.accent,
                );
            }
        },
        WidgetKind::Transport => {
            use servo::MediaSessionActionType;
            // The middle one is drawn rather than lettered — see the note on
            // `morphicons`. Two bars fold into a triangle, which is a state
            // change you can watch rather than one you have to notice.
            let buttons = [
                (Some(Icon::Back), MediaSessionActionType::PreviousTrack),
                (
                    None,
                    if media.playing {
                        MediaSessionActionType::Pause
                    } else {
                        MediaSessionActionType::Play
                    },
                ),
                (Some(Icon::Forward), MediaSessionActionType::NextTrack),
            ];
            for (index, (icon, action)) in buttons.into_iter().enumerate() {
                let centre = pos2(
                    rect.min.x + rect.width() * (0.25 + 0.25 * index as f32),
                    rect.center().y,
                );
                let hit = Rect::from_center_size(centre, vec2(30.0, 30.0));
                let response = root.interact(
                    hit,
                    Id::unique("zervo_transport").with((slot_index, index)),
                    Sense::click(),
                );
                if response.hovered() {
                    root.painter().rect_filled(
                        hit,
                        palette.corner(Tier::Row),
                        palette.surface_hover,
                    );
                }
                let tint = if media.is_idle() {
                    palette.text_muted.gamma_multiply(0.4)
                } else {
                    palette.text
                };
                let glyph = Rect::from_center_size(centre, vec2(16.0, 16.0));
                match icon {
                    Some(icon) => icons::draw_icon(root.painter(), glyph, icon, tint),
                    None => {
                        let cross = crate::glass::ease_out(root.ctx().animate_bool_with_time(
                            Id::unique("zervo_transport_morph").with(slot_index),
                            media.playing,
                            palette.material.animation,
                        ));
                        crate::morphicons::draw(
                            root.painter(),
                            glyph,
                            &crate::morphicons::PLAY,
                            &crate::morphicons::PAUSE,
                            cross,
                            tint,
                        );
                    },
                }
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
        palette.corner(Tier::Hairline),
        palette.text_muted.gamma_multiply(0.25 + 0.45 * emphasis),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_widgets_move_to_sidebar_rows_in_reading_order() {
        let mut widgets = Placed::defaults();
        for (index, widget) in widgets.iter_mut().enumerate() {
            widget.legacy_column = [3, 0, 1][index];
            widget.row = 0;
        }
        normalize_layout(&mut widgets);
        assert_eq!(
            widgets.iter().map(|widget| widget.row).collect::<Vec<_>>(),
            [2, 0, 1]
        );
        assert!(widgets.iter().all(|widget| widget.legacy_column == 0));
    }

    #[test]
    fn sidebar_widgets_swap_rows_and_cannot_resize_into_each_other() {
        let mut widgets = Placed::defaults();
        assert!(swap_widgets(&mut widgets, 0, 1));
        assert_eq!((widgets[0].row, widgets[1].row), (1, 0));
        assert!(!resize_widget(&mut widgets, 0, Size::rows(2)));
        assert!(resize_widget(&mut widgets, 2, Size::rows(2)));
        assert_eq!(free_row(&widgets), None);
    }

    #[test]
    fn valid_sidebar_positions_survive_migration() {
        let widgets = Placed::defaults();
        let mut migrated = widgets.clone();
        normalize_layout(&mut migrated);
        assert_eq!(migrated, widgets);
        assert_eq!(free_row(&migrated), Some(3));
    }
}
