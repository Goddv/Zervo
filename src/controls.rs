//! Page-initiated UI.
//!
//! Servo hands the embedder an [`EmbedderControl`] for everything a page asks
//! for that the engine cannot draw itself: `<select>` popups, `alert`,
//! `confirm`, `prompt`, the colour picker, the context menu, file pickers.
//!
//! Every control must be answered or dropped, and dropping one takes its safe
//! default, which is "the user cancelled". That makes ignoring them look like
//! nothing happening: a `<select>` that never opens, a `confirm()` that is
//! always false, a right-click that does nothing.
//!
//! Dialogs carry messages written by the page, so they are drawn in a way that
//! cannot be mistaken for Zervo's own UI: labelled with the origin that raised
//! them, and confined to the content card rather than floating over the chrome.

use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Rect, Sense, Stroke, StrokeKind,
    TextEdit, Ui, pos2, vec2,
};
use servo::{
    ContextMenuAction, ContextMenuItem, EmbedderControl, EmbedderControlId, RgbColor,
    SelectElementOptionOrOptgroup, SimpleDialog,
};

use crate::glass::{self, Glass};
use crate::theme::Palette;

/// Controls waiting for the user, oldest first.
#[derive(Default)]
pub struct Controls {
    pending: Vec<EmbedderControl>,
    /// The control drawn on the previous frame. A popup must survive its own
    /// opening click: the press that asked for a context menu can still be in
    /// egui's input when the menu first appears, and would dismiss it as a
    /// click outside on the very frame it opened.
    drawn: Option<EmbedderControlId>,
}

/// What the user did with the control on screen.
enum Resolution {
    /// OK, Submit, or a chosen option.
    Accept,
    /// Cancel, Escape, or a click outside.
    Cancel,
    /// A context menu entry.
    Menu(ContextMenuAction),
}

impl Controls {
    pub fn push(&mut self, control: EmbedderControl) {
        self.pending.push(control);
    }

    /// The engine withdrew a request. Dropping it cancels, which is what
    /// withdrawing means.
    pub fn hide(&mut self, id: EmbedderControlId) {
        self.pending.retain(|control| control.id() != id);
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Draw the most recent control. Anything queued behind it waits its turn:
    /// stacked page dialogs are a nuisance, and a page cannot reasonably need
    /// two answers at once.
    ///
    /// `origin` labels dialogs. `scale` and `content_rect` convert the device
    /// pixel positions Servo reports, which are relative to the webview, into
    /// window points.
    pub fn draw(
        &mut self,
        root: &mut Ui,
        palette: &Palette,
        content_rect: Rect,
        scale: f32,
        origin: &str,
    ) {
        let Some(index) = self.pending.len().checked_sub(1) else {
            return;
        };

        let settled = self.drawn == Some(self.pending[index].id());
        self.drawn = Some(self.pending[index].id());

        let ctx = root.ctx().clone();
        // Text measurement needs a painter; `Context::fonts` hands out a view
        // that cannot lay out.
        let measure = root.painter().clone();
        let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let enter = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        let mut resolution = None;

        // Servo reports positions in device pixels relative to the webview.
        let to_window = |rect: servo::DeviceIntRect| {
            Rect::from_min_max(
                content_rect.min + vec2(rect.min.x as f32 / scale, rect.min.y as f32 / scale),
                content_rect.min + vec2(rect.max.x as f32 / scale, rect.max.y as f32 / scale),
            )
        };

        match &mut self.pending[index] {
            EmbedderControl::SimpleDialog(dialog) => {
                resolution = draw_dialog(
                    &ctx,
                    &measure,
                    palette,
                    content_rect,
                    origin,
                    dialog,
                    escape,
                    enter,
                );
            },
            EmbedderControl::SelectElement(select) => {
                let anchor = to_window(select.position());
                let options = select.options().to_vec();
                let multiple = select.allow_select_multiple();
                let mut chosen = select.selected_options();
                let picked = draw_select(
                    &ctx,
                    palette,
                    content_rect,
                    anchor,
                    &options,
                    &chosen,
                    multiple,
                    settled,
                );
                match picked {
                    Some(Picked::Option(id)) => {
                        if multiple {
                            if let Some(at) = chosen.iter().position(|value| *value == id) {
                                chosen.remove(at);
                            } else {
                                chosen.push(id);
                            }
                            select.select(chosen);
                        } else {
                            select.select(vec![id]);
                            resolution = Some(Resolution::Accept);
                        }
                    },
                    Some(Picked::Done) => resolution = Some(Resolution::Accept),
                    Some(Picked::Cancel) => resolution = Some(Resolution::Cancel),
                    None => {},
                }
                if escape {
                    resolution = Some(Resolution::Cancel);
                }
            },
            EmbedderControl::ColorPicker(picker) => {
                let anchor = to_window(picker.position());
                match draw_color_picker(
                    &ctx,
                    palette,
                    content_rect,
                    anchor,
                    picker.current_color(),
                    settled,
                ) {
                    Some(Some(colour)) => {
                        picker.select(Some(colour));
                        resolution = Some(Resolution::Accept);
                    },
                    Some(None) => resolution = Some(Resolution::Cancel),
                    None => {},
                }
                if escape {
                    resolution = Some(Resolution::Cancel);
                }
            },
            EmbedderControl::ContextMenu(menu) => {
                let anchor = to_window(menu.position());
                match draw_context_menu(
                    &ctx,
                    &measure,
                    palette,
                    content_rect,
                    anchor,
                    menu.items(),
                    settled,
                ) {
                    Some(Some(action)) => resolution = Some(Resolution::Menu(action)),
                    Some(None) => resolution = Some(Resolution::Cancel),
                    None => {},
                }
                if escape {
                    resolution = Some(Resolution::Cancel);
                }
            },
            // The file picker is a native panel, answered before it ever gets
            // here, and the IME control drives winit rather than being drawn.
            EmbedderControl::FilePicker(_) | EmbedderControl::InputMethod(_) => {
                resolution = Some(Resolution::Cancel);
            },
        }

        let Some(resolution) = resolution else {
            return;
        };
        let control = self.pending.remove(index);
        match (control, resolution) {
            (EmbedderControl::SimpleDialog(dialog), Resolution::Accept) => dialog.confirm(),
            (EmbedderControl::SimpleDialog(dialog), _) => dialog.dismiss(),
            (EmbedderControl::SelectElement(select), Resolution::Accept) => select.submit(),
            (EmbedderControl::ColorPicker(picker), Resolution::Accept) => picker.submit(),
            (EmbedderControl::ContextMenu(menu), Resolution::Menu(action)) => menu.select(action),
            (EmbedderControl::ContextMenu(menu), _) => menu.dismiss(),
            // Everything else cancels by being dropped.
            _ => {},
        }
    }
}

/// A floating panel: scrim over the page, card on top. Returns the card's `Ui`.
fn panel<R>(
    ctx: &egui::Context,
    id: &str,
    palette: &Palette,
    scrim: Option<Rect>,
    rect: Rect,
    radius: u8,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let contrasted;
    let palette = match scrim {
        Some(_) => palette,
        None => {
            contrasted = palette.over(rect);
            &contrasted
        },
    };
    egui::Area::new(Id::new(id))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(ctx, |ui| {
            if let Some(scrim) = scrim {
                ui.painter()
                    .rect_filled(scrim, CornerRadius::ZERO, dim(palette));
            }
            let painter = ui.painter();
            // Backed opaquely: these float over live page content and the
            // glass material is only about 95% opaque, which is fine over the
            // chrome but lets page text ghost through a menu.
            for shape in glass::shapes(
                rect,
                palette,
                Glass::of(crate::theme::Surface::Menu)
                    .radius_exact(radius)
                    .opaque(palette.bg)
                    .border(palette.border),
            ) {
                painter.add(shape);
            }
            let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(14.0)));
            let value = add(&mut inner);
            ui.advance_cursor_after_rect(rect);
            value
        })
        .inner
}

fn dim(palette: &Palette) -> Color32 {
    if palette.dark {
        Color32::from_black_alpha(120)
    } else {
        Color32::from_black_alpha(70)
    }
}

/// A button. Returns true when clicked.
fn button(ui: &mut Ui, label: &str, palette: &Palette, primary: bool) -> bool {
    let width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(13.0), palette.text)
        .size()
        .x
        + 26.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click());
    let hovered = response.hovered();
    let fill = match (primary, hovered) {
        (true, true) => palette.accent,
        (true, false) => palette.accent.gamma_multiply(0.85),
        (false, true) => palette.surface_hover,
        (false, false) => palette.surface,
    };
    ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
    if !primary {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(7),
            Stroke::new(1.0_f32, palette.border),
            StrokeKind::Inside,
        );
    }
    let text = if primary {
        Color32::WHITE
    } else {
        palette.text
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(13.0),
        text,
    );
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

fn draw_dialog(
    ctx: &egui::Context,
    measure: &egui::Painter,
    palette: &Palette,
    content_rect: Rect,
    origin: &str,
    dialog: &mut SimpleDialog,
    escape: bool,
    enter: bool,
) -> Option<Resolution> {
    let width = 420.0_f32.min(content_rect.width() - 48.0);
    let message = dialog.message().to_owned();
    let galley = measure.layout(
        message.clone(),
        FontId::proportional(13.5),
        palette.text,
        width - 28.0,
    );
    let prompt = matches!(dialog, SimpleDialog::Prompt(_));
    let height = 28.0 + 18.0 + galley.size().y + if prompt { 42.0 } else { 0.0 } + 40.0;
    let rect = Rect::from_center_size(content_rect.center(), vec2(width, height));

    let mut resolution = None;
    panel(
        ctx,
        "zervo_page_dialog",
        palette,
        Some(content_rect),
        rect,
        14,
        |ui| {
            // Say who is asking. These messages are written by the page, and
            // must not read as though Zervo is asking.
            ui.painter().text(
                ui.max_rect().min,
                Align2::LEFT_TOP,
                format!("{origin} says"),
                FontId::proportional(11.5),
                palette.text_muted,
            );
            ui.painter().galley(
                ui.max_rect().min + vec2(0.0, 18.0),
                galley.clone(),
                palette.text,
            );

            let mut confirmed = enter;
            if prompt {
                let field = Rect::from_min_size(
                    ui.max_rect().min + vec2(0.0, 22.0 + galley.size().y),
                    vec2(ui.max_rect().width(), 30.0),
                );
                let SimpleDialog::Prompt(prompt) = dialog else {
                    unreachable!("checked above")
                };
                let mut value = prompt.current_value().to_owned();
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
                let response = child.add_sized(
                    field.size(),
                    TextEdit::singleline(&mut value).font(FontId::proportional(13.0)),
                );
                if response.changed() {
                    prompt.set_current_value(&value);
                }
                if !response.has_focus() && !response.lost_focus() {
                    response.request_focus();
                }
                confirmed |= response.lost_focus() && enter;
            }

            // Buttons, bottom right, primary last.
            let row = Rect::from_min_max(
                pos2(ui.max_rect().min.x, ui.max_rect().max.y - 28.0),
                ui.max_rect().max,
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(row)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            if button(&mut child, "OK", palette, true) || confirmed {
                resolution = Some(Resolution::Accept);
            }
            let cancellable = !matches!(dialog, SimpleDialog::Alert(_));
            if cancellable {
                child.add_space(8.0);
                if button(&mut child, "Cancel", palette, false) || escape {
                    resolution = Some(Resolution::Cancel);
                }
            } else if escape {
                resolution = Some(Resolution::Accept);
            }
        },
    );
    resolution
}

enum Picked {
    Option(usize),
    Done,
    Cancel,
}

fn draw_select(
    ctx: &egui::Context,
    palette: &Palette,
    content_rect: Rect,
    anchor: Rect,
    options: &[SelectElementOptionOrOptgroup],
    selected: &[usize],
    multiple: bool,
    settled: bool,
) -> Option<Picked> {
    const ROW: f32 = 26.0;
    let mut rows = 0.0;
    for option in options {
        rows += match option {
            SelectElementOptionOrOptgroup::Option(_) => 1.0,
            SelectElementOptionOrOptgroup::Optgroup { options, .. } => options.len() as f32 + 1.0,
        };
    }
    let width = anchor.width().max(180.0).min(content_rect.width() - 24.0);
    let height =
        (rows * ROW + 16.0 + if multiple { 36.0 } else { 0.0 }).min(content_rect.height() * 0.7);
    let rect = clamp_to(
        Rect::from_min_size(pos2(anchor.min.x, anchor.max.y + 4.0), vec2(width, height)),
        content_rect,
    );

    let mut picked = None;
    panel(ctx, "zervo_select", palette, None, rect, 10, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for entry in options {
                    match entry {
                        SelectElementOptionOrOptgroup::Option(option) => {
                            if select_row(
                                ui,
                                palette,
                                &option.label,
                                option.is_disabled,
                                selected.contains(&option.id),
                                0.0,
                            ) {
                                picked = Some(Picked::Option(option.id));
                            }
                        },
                        SelectElementOptionOrOptgroup::Optgroup { label, options } => {
                            let (rect, _) = ui.allocate_exact_size(
                                vec2(ui.available_width(), ROW),
                                Sense::hover(),
                            );
                            ui.painter().text(
                                pos2(rect.min.x + 6.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                label,
                                FontId::proportional(11.5),
                                palette.text_muted,
                            );
                            for option in options {
                                if select_row(
                                    ui,
                                    palette,
                                    &option.label,
                                    option.is_disabled,
                                    selected.contains(&option.id),
                                    12.0,
                                ) {
                                    picked = Some(Picked::Option(option.id));
                                }
                            }
                        },
                    }
                }
            });
        if multiple {
            let row = Rect::from_min_max(
                pos2(ui.max_rect().min.x, ui.max_rect().max.y - 28.0),
                ui.max_rect().max,
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(row)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            if button(&mut child, "Done", palette, true) {
                picked = Some(Picked::Done);
            }
        }
    });
    if picked.is_none() && settled && clicked_outside(ctx, rect) {
        picked = Some(Picked::Cancel);
    }
    picked
}

fn select_row(
    ui: &mut Ui,
    palette: &Palette,
    label: &str,
    disabled: bool,
    selected: bool,
    indent: f32,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), 26.0),
        if disabled {
            Sense::hover()
        } else {
            Sense::click()
        },
    );
    if selected {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(6),
            palette.accent.gamma_multiply(0.28),
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), palette.surface_hover);
    }
    let colour = if disabled {
        palette.text_muted.gamma_multiply(0.6)
    } else {
        palette.text
    };
    ui.painter().text(
        pos2(rect.min.x + 8.0 + indent, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        colour,
    );
    !disabled && response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// `Some(Some(colour))` to accept, `Some(None)` to cancel, `None` while open.
fn draw_color_picker(
    ctx: &egui::Context,
    palette: &Palette,
    content_rect: Rect,
    anchor: Rect,
    current: Option<RgbColor>,
    settled: bool,
) -> Option<Option<RgbColor>> {
    // A fixed palette rather than a colour wheel: enough for the handful of
    // pages that use `<input type=color>`, and it cannot be got subtly wrong.
    const SWATCHES: [(u8, u8, u8); 24] = [
        (0, 0, 0),
        (68, 68, 68),
        (102, 102, 102),
        (153, 153, 153),
        (204, 204, 204),
        (255, 255, 255),
        (152, 0, 0),
        (255, 0, 0),
        (255, 153, 0),
        (255, 255, 0),
        (0, 255, 0),
        (0, 255, 255),
        (74, 134, 232),
        (0, 0, 255),
        (153, 0, 255),
        (255, 0, 255),
        (230, 184, 175),
        (244, 204, 204),
        (252, 229, 205),
        (255, 242, 204),
        (217, 234, 211),
        (208, 224, 227),
        (201, 218, 248),
        (217, 210, 233),
    ];
    const CELL: f32 = 26.0;
    const COLUMNS: usize = 6;

    let rows = SWATCHES.len().div_ceil(COLUMNS) as f32;
    let rect = clamp_to(
        Rect::from_min_size(
            pos2(anchor.min.x, anchor.max.y + 4.0),
            vec2(COLUMNS as f32 * CELL + 28.0, rows * CELL + 28.0 + 36.0),
        ),
        content_rect,
    );

    let mut result = None;
    panel(ctx, "zervo_color_picker", palette, None, rect, 10, |ui| {
        for (index, (red, green, blue)) in SWATCHES.iter().enumerate() {
            let column = index % COLUMNS;
            let row = index / COLUMNS;
            let cell = Rect::from_min_size(
                ui.max_rect().min + vec2(column as f32 * CELL, row as f32 * CELL),
                vec2(CELL - 4.0, CELL - 4.0),
            );
            let response = ui.interact(cell, ui.id().with(index), Sense::click());
            ui.painter().rect_filled(
                cell,
                CornerRadius::same(5),
                Color32::from_rgb(*red, *green, *blue),
            );
            let chosen = current.is_some_and(|colour| {
                (colour.red, colour.green, colour.blue) == (*red, *green, *blue)
            });
            if chosen || response.hovered() {
                ui.painter().rect_stroke(
                    cell,
                    CornerRadius::same(5),
                    Stroke::new(2.0_f32, palette.accent),
                    StrokeKind::Outside,
                );
            }
            if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                result = Some(Some(RgbColor {
                    red: *red,
                    green: *green,
                    blue: *blue,
                }));
            }
        }
    });
    if result.is_none() && settled && clicked_outside(ctx, rect) {
        result = Some(None);
    }
    result
}

/// `Some(Some(action))` when chosen, `Some(None)` to dismiss, `None` while open.
fn draw_context_menu(
    ctx: &egui::Context,
    measure: &egui::Painter,
    palette: &Palette,
    content_rect: Rect,
    anchor: Rect,
    items: &[ContextMenuItem],
    settled: bool,
) -> Option<Option<ContextMenuAction>> {
    const ROW: f32 = 28.0;
    const SEPARATOR: f32 = 9.0;

    let mut height = 12.0;
    let mut width: f32 = 170.0;
    for item in items {
        match item {
            ContextMenuItem::Separator => height += SEPARATOR,
            ContextMenuItem::Item { label, .. } => {
                height += ROW;
                let measured = measure
                    .layout_no_wrap(label.clone(), FontId::proportional(13.0), palette.text)
                    .size()
                    .x;
                width = width.max(measured + 34.0);
            },
        }
    }
    let rect = clamp_to(
        Rect::from_min_size(anchor.min, vec2(width, height)),
        content_rect,
    );

    let mut result = None;
    panel(ctx, "zervo_context_menu", palette, None, rect, 10, |ui| {
        for (index, item) in items.iter().enumerate() {
            match item {
                ContextMenuItem::Separator => {
                    let (rect, _) = ui
                        .allocate_exact_size(vec2(ui.available_width(), SEPARATOR), Sense::hover());
                    ui.painter().hline(
                        rect.x_range(),
                        rect.center().y,
                        Stroke::new(1.0_f32, palette.border),
                    );
                },
                ContextMenuItem::Item {
                    label,
                    action,
                    enabled,
                } => {
                    let (rect, response) = ui.allocate_exact_size(
                        vec2(ui.available_width(), ROW),
                        if *enabled {
                            Sense::click()
                        } else {
                            Sense::hover()
                        },
                    );
                    if *enabled && response.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            CornerRadius::same(6),
                            palette.surface_hover,
                        );
                    }
                    let colour = if *enabled {
                        palette.text
                    } else {
                        palette.text_muted.gamma_multiply(0.6)
                    };
                    ui.painter().text(
                        pos2(rect.min.x + 8.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        label,
                        FontId::proportional(13.0),
                        colour,
                    );
                    let _ = index;
                    if *enabled && response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                        result = Some(Some(*action));
                    }
                },
            }
        }
    });
    if result.is_none() && settled && clicked_outside(ctx, rect) {
        result = Some(None);
    }
    result
}

/// Keep a popup inside the content card, flipping it above its anchor when
/// there is no room below.
fn clamp_to(rect: Rect, bounds: Rect) -> Rect {
    let mut min = rect.min;
    if rect.max.x > bounds.max.x - 8.0 {
        min.x = (bounds.max.x - 8.0 - rect.width()).max(bounds.min.x + 8.0);
    }
    if rect.max.y > bounds.max.y - 8.0 {
        min.y = (bounds.max.y - 8.0 - rect.height()).max(bounds.min.y + 8.0);
    }
    min.x = min.x.max(bounds.min.x + 8.0);
    min.y = min.y.max(bounds.min.y + 8.0);
    Rect::from_min_size(min, rect.size())
}

fn clicked_outside(ctx: &egui::Context, rect: Rect) -> bool {
    ctx.input(|input| {
        input.pointer.any_pressed()
            && input
                .pointer
                .interact_pos()
                .is_some_and(|pos| !rect.contains(pos))
    })
}
