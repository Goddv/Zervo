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
    AllowOrDenyRequest, AuthenticationRequest, ContextMenuAction, ContextMenuItem, EmbedderControl,
    EmbedderControlId, RgbColor, SelectElementOptionOrOptgroup, SimpleDialog,
};

use crate::glass::{self, Glass};
use crate::theme::Palette;

/// An HTTP authentication challenge that a saved login matches.
///
/// Held rather than answered, because answering it sends the password. The
/// challenge is unsolicited — any subresource can provoke one — and a login
/// saved for a domain covers every host beneath it, so which of those two
/// facts applies is a question only the person sitting there can settle.
struct Auth {
    request: AuthenticationRequest,
    /// The host that raised the challenge.
    host: String,
    /// The saved login on offer. Its `site` may be a parent domain of `host` —
    /// which is exactly why the prompt shows both.
    login: crate::passwords::Login,
}

/// A response Servo cannot render, offered to the user before a byte of it is
/// written anywhere.
///
/// Accepting used to happen inline, in the delegate callback, before anything
/// appeared on screen. Since the engine offers the embedder anything carrying
/// `Content-Disposition: attachment`, that let any page put a file of any size
/// into the downloads folder with no interaction at all.
#[cfg(feature = "engine-downloads")]
struct Offer {
    response: servo::UnsupportedResponse,
    host: String,
    filename: String,
}

/// An offer the user accepted, waiting for the download manager to pick it up.
#[cfg(feature = "engine-downloads")]
pub struct Accepted {
    pub request_id: servo::RequestId,
    pub url: String,
    pub filename: String,
}

/// A `beforeunload` the page wants confirmed before it is navigated away from.
struct Unload(AllowOrDenyRequest);

/// A capability a page asked for, and the host that asked.
struct Permission {
    request: servo::PermissionRequest,
    host: String,
}

/// Controls waiting for the user, oldest first.
#[derive(Default)]
pub struct Controls {
    pending: Vec<EmbedderControl>,
    /// An authentication challenge on screen. One at a time.
    auth: Option<Auth>,
    /// A `beforeunload` confirmation.
    unload: Option<Unload>,
    /// A permission the page asked for.
    permission: Option<Permission>,
    /// A download waiting to be allowed.
    #[cfg(feature = "engine-downloads")]
    offer: Option<Offer>,
    /// Downloads the user allowed, for `main.rs` to start.
    #[cfg(feature = "engine-downloads")]
    accepted: Vec<Accepted>,
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

    /// Offer a saved login for a challenge, once the caller has established
    /// that the connection is worth sending one over.
    pub fn push_auth(
        &mut self,
        request: AuthenticationRequest,
        host: String,
        login: crate::passwords::Login,
    ) {
        // One at a time. A second challenge arriving while the first is still
        // on screen is dropped, and dropping cancels — which is the safe answer
        // for a request nobody has looked at.
        if self.auth.is_none() {
            self.auth = Some(Auth {
                request,
                host,
                login,
            });
        }
    }

    /// The engine withdrew a request. Dropping it cancels, which is what
    /// withdrawing means.
    pub fn hide(&mut self, id: EmbedderControlId) {
        self.pending.retain(|control| control.id() != id);
    }

    /// Offer a download. Nothing is accepted and nothing is written until the
    /// user says so.
    #[cfg(feature = "engine-downloads")]
    pub fn push_offer(&mut self, response: servo::UnsupportedResponse, filename: String) {
        // One at a time; a second offer arriving while the first is on screen
        // is dropped, and dropping declines.
        if self.offer.is_some() {
            return;
        }
        let host = response.url.host_str().unwrap_or("This page").to_owned();
        self.offer = Some(Offer {
            response,
            host,
            filename,
        });
    }

    /// Downloads the user allowed since this was last called.
    #[cfg(feature = "engine-downloads")]
    pub fn take_accepted(&mut self) -> Vec<Accepted> {
        std::mem::take(&mut self.accepted)
    }

    /// Queue a `beforeunload` confirmation.
    pub fn push_unload(&mut self, request: AllowOrDenyRequest) {
        // A second one while the first is up is dropped, and dropping allows —
        // which is right here: the page has already been told once.
        if self.unload.is_none() {
            self.unload = Some(Unload(request));
        }
    }

    /// Queue a permission request.
    pub fn push_permission(&mut self, request: servo::PermissionRequest, host: String) {
        // Dropping denies, which is the safe answer for one nobody saw.
        if self.permission.is_none() {
            self.permission = Some(Permission { request, host });
        }
    }

    /// The `beforeunload` confirmation: leave, or stay.
    fn draw_unload(&mut self, root: &mut Ui, palette: &Palette, content_rect: Rect) {
        if self.unload.is_none() {
            return;
        }
        let ctx = root.ctx().clone();
        let mut stay = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let mut leave = false;
        let rect = Rect::from_center_size(
            content_rect.center(),
            vec2(420.0_f32.min(content_rect.width() - 48.0), 140.0),
        );
        panel(
            &ctx,
            "zervo_unload",
            palette,
            Some(content_rect),
            rect,
            14,
            |ui| {
                let top = ui.max_rect().min;
                ui.painter().text(
                    top,
                    Align2::LEFT_TOP,
                    "Leave this page?",
                    FontId::proportional(14.0),
                    palette.text,
                );
                ui.painter().text(
                    top + vec2(0.0, 26.0),
                    Align2::LEFT_TOP,
                    "Anything you have typed and not sent will be lost.",
                    FontId::proportional(12.5),
                    palette.text_muted,
                );
                let row = Rect::from_min_max(
                    pos2(ui.max_rect().min.x, ui.max_rect().max.y - 28.0),
                    ui.max_rect().max,
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row)
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                // Staying is the primary action: it is the one that cannot lose
                // anything, and the page only asked because there is something to
                // lose.
                stay |= button(&mut child, "Stay", palette, true);
                child.add_space(8.0);
                leave = button(&mut child, "Leave", palette, false);
            },
        );
        if !(stay || leave) {
            return;
        }
        let Some(Unload(request)) = self.unload.take() else {
            return;
        };
        if leave {
            request.allow();
        } else {
            request.deny();
        }
    }

    /// A capability the page asked for.
    fn draw_permission(&mut self, root: &mut Ui, palette: &Palette, content_rect: Rect) {
        let Some(pending) = &self.permission else {
            return;
        };
        let ctx = root.ctx().clone();
        let mut deny = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let mut allow = false;
        let host = pending.host.clone();
        let what = feature_name(pending.request.feature());
        let rect = Rect::from_center_size(
            content_rect.center(),
            vec2(420.0_f32.min(content_rect.width() - 48.0), 148.0),
        );
        panel(
            &ctx,
            "zervo_permission",
            palette,
            Some(content_rect),
            rect,
            14,
            |ui| {
                let top = ui.max_rect().min;
                ui.painter().text(
                    top,
                    Align2::LEFT_TOP,
                    format!("{host} wants {what}"),
                    FontId::proportional(14.0),
                    palette.text,
                );
                ui.painter().text(
                    top + vec2(0.0, 26.0),
                    Align2::LEFT_TOP,
                    "You can change your mind later in Settings.",
                    FontId::proportional(12.5),
                    palette.text_muted,
                );
                let row = Rect::from_min_max(
                    pos2(ui.max_rect().min.x, ui.max_rect().max.y - 28.0),
                    ui.max_rect().max,
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row)
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                allow = button(&mut child, "Allow", palette, true);
                child.add_space(8.0);
                deny |= button(&mut child, "Block", palette, false);
            },
        );
        if !(allow || deny) {
            return;
        }
        let Some(pending) = self.permission.take() else {
            return;
        };
        if allow {
            pending.request.allow();
        } else {
            pending.request.deny();
        }
    }

    pub fn is_empty(&self) -> bool {
        #[cfg(feature = "engine-downloads")]
        if self.offer.is_some() {
            return false;
        }
        self.pending.is_empty()
            && self.auth.is_none()
            && self.unload.is_none()
            && self.permission.is_none()
    }

    /// The download prompt: who is offering, what it is called, two buttons.
    #[cfg(feature = "engine-downloads")]
    fn draw_offer(&mut self, root: &mut Ui, palette: &Palette, content_rect: Rect) {
        let Some(offer) = &self.offer else {
            return;
        };
        let ctx = root.ctx().clone();
        let mut deny = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let mut allow = false;

        let width = 420.0_f32.min(content_rect.width() - 48.0);
        let rect = Rect::from_center_size(content_rect.center(), vec2(width, 148.0));
        let host = offer.host.clone();
        let filename = offer.filename.clone();

        panel(
            &ctx,
            "zervo_download_offer",
            palette,
            Some(content_rect),
            rect,
            14,
            |ui| {
                let top = ui.max_rect().min;
                ui.painter().text(
                    top,
                    Align2::LEFT_TOP,
                    format!("{host} wants to save a file"),
                    FontId::proportional(14.0),
                    palette.text,
                );
                ui.painter().text(
                    top + vec2(0.0, 26.0),
                    Align2::LEFT_TOP,
                    crate::ui::ellipsize(&filename, 46),
                    FontId::proportional(12.5),
                    palette.text_muted,
                );
                ui.painter().text(
                    top + vec2(0.0, 50.0),
                    Align2::LEFT_TOP,
                    "It goes to your Downloads folder.",
                    FontId::proportional(11.5),
                    palette.text_muted,
                );

                let row = Rect::from_min_max(
                    pos2(ui.max_rect().min.x, ui.max_rect().max.y - 28.0),
                    ui.max_rect().max,
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row)
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                allow = button(&mut child, "Save", palette, true);
                child.add_space(8.0);
                deny |= button(&mut child, "Cancel", palette, false);
            },
        );

        if !(allow || deny) {
            return;
        }
        let Some(offer) = self.offer.take() else {
            return;
        };
        if !allow {
            // Dropping the response declines it.
            return;
        }
        let mut response = offer.response;
        let record = Accepted {
            request_id: response.request_id,
            url: response.url.to_string(),
            filename: offer.filename,
        };
        response.accept();
        self.accepted.push(record);
    }

    /// The authentication prompt: who is asking, which login is on offer, and
    /// two buttons. Nothing is sent until one of them is pressed.
    fn draw_auth(
        &mut self,
        root: &mut Ui,
        palette: &Palette,
        content_rect: Rect,
        vault: &crate::passwords::Vault,
    ) {
        let Some(auth) = &self.auth else {
            return;
        };
        let ctx = root.ctx().clone();
        let mut deny = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let mut allow = false;

        let width = 420.0_f32.min(content_rect.width() - 48.0);
        let rect = Rect::from_center_size(content_rect.center(), vec2(width, 148.0));
        let asking = auth.host.clone();
        let saved_for = auth.login.site.clone();
        let username = auth.login.username.clone();

        panel(
            &ctx,
            "zervo_auth",
            palette,
            Some(content_rect),
            rect,
            14,
            |ui| {
                let top = ui.max_rect().min;
                ui.painter().text(
                    top,
                    Align2::LEFT_TOP,
                    format!("{asking} wants a password"),
                    FontId::proportional(14.0),
                    palette.text,
                );
                // Both names, always. When they differ it is because a login
                // saved for a domain is being offered to a host underneath it,
                // and that is the case worth looking at twice.
                let detail = if saved_for == asking {
                    format!("Send your saved password for {username}?")
                } else {
                    format!("Send the password saved for {username} at {saved_for}?")
                };
                ui.painter().text(
                    top + vec2(0.0, 26.0),
                    Align2::LEFT_TOP,
                    detail,
                    FontId::proportional(12.5),
                    palette.text_muted,
                );
                ui.painter().text(
                    top + vec2(0.0, 50.0),
                    Align2::LEFT_TOP,
                    "The page asked for this, not Zervo.",
                    FontId::proportional(11.5),
                    palette.text_muted,
                );

                let row = Rect::from_min_max(
                    pos2(ui.max_rect().min.x, ui.max_rect().max.y - 28.0),
                    ui.max_rect().max,
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row)
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                // No Enter shortcut on the primary button here, unlike the page
                // dialogs: a stray Return meant for the page should not be able
                // to hand a password over.
                allow = button(&mut child, "Sign in", palette, true);
                child.add_space(8.0);
                deny |= button(&mut child, "Not now", palette, false);
            },
        );

        if !(allow || deny) {
            return;
        }
        // Taken either way. Dropping the request is what cancels it, so a
        // refusal needs no further action.
        let Some(auth) = self.auth.take() else {
            return;
        };
        if !allow {
            return;
        }
        match vault.secret(&auth.login) {
            Some(password) => auth.request.authenticate(auth.login.username, password),
            None => log::warn!(
                "the keychain would not give up the password for {}",
                auth.login.site
            ),
        }
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
        vault: &crate::passwords::Vault,
    ) {
        // An authentication challenge takes the screen on its own: it is the
        // one dialog here whose OK hands something away.
        if self.auth.is_some() {
            self.draw_auth(root, palette, content_rect, vault);
            return;
        }
        #[cfg(feature = "engine-downloads")]
        if self.offer.is_some() {
            self.draw_offer(root, palette, content_rect);
            return;
        }
        if self.permission.is_some() {
            self.draw_permission(root, palette, content_rect);
            return;
        }
        if self.unload.is_some() {
            self.draw_unload(root, palette, content_rect);
            return;
        }
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
                    Keys { escape, enter },
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
                    Select {
                        options: &options,
                        selected: &chosen,
                        multiple,
                    },
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
                // Rounded to the content card, not square. The scrim covers the
                // card exactly, and the card has rounded corners — so a square
                // one laid four dark right-angles over them, which reads as the
                // card having grown black corners for as long as the dialog is
                // up. Rare when the only dialogs were a page's own `alert` and
                // `confirm`; not rare at all now that a download asks first.
                ui.painter().rect_filled(
                    scrim,
                    CornerRadius::same(crate::theme::CONTENT_RADIUS as u8),
                    dim(palette),
                );
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

/// The two keys a dialog answers to.
///
/// Named rather than passed as two adjacent `bool`s: transposing them swaps
/// Cancel and OK, and nothing would have complained.
#[derive(Clone, Copy)]
struct Keys {
    escape: bool,
    enter: bool,
}

fn draw_dialog(
    ctx: &egui::Context,
    measure: &egui::Painter,
    palette: &Palette,
    content_rect: Rect,
    origin: &str,
    dialog: &mut SimpleDialog,
    keys: Keys,
) -> Option<Resolution> {
    let Keys { escape, enter } = keys;
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

/// The `<select>` being drawn: what is in it, what is chosen, and whether more
/// than one thing may be.
struct Select<'a> {
    options: &'a [SelectElementOptionOrOptgroup],
    selected: &'a [usize],
    multiple: bool,
}

fn draw_select(
    ctx: &egui::Context,
    palette: &Palette,
    content_rect: Rect,
    anchor: Rect,
    select: Select<'_>,
    settled: bool,
) -> Option<Picked> {
    let Select {
        options,
        selected,
        multiple,
    } = select;
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

/// What to call a capability on screen. Phrased to finish "example.com
/// wants …", so a person reads a sentence rather than a spec term.
fn feature_name(feature: servo::PermissionFeature) -> &'static str {
    use servo::PermissionFeature as F;
    match feature {
        F::Geolocation => "your location",
        F::Notifications => "to send you notifications",
        F::Push => "to send you push messages",
        F::Midi => "to use your MIDI devices",
        F::Camera => "to use your camera",
        F::Microphone => "to use your microphone",
        F::Speaker => "to use your speakers",
        F::DeviceInfo => "to see your devices",
        F::BackgroundSync => "to sync in the background",
        F::Bluetooth => "to use Bluetooth",
        F::PersistentStorage => "to store data on this device",
        F::ScreenWakeLock(_) => "to keep the screen awake",
        F::Gamepad => "to use your gamepad",
        // Deliberately exhaustive, with no catch-all: when Servo adds a
        // feature this stops compiling, which is the moment to decide what to
        // call it rather than shipping "a permission".
    }
}
