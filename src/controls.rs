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

use crate::l10n::text as tr;
use egui::{
    Align2, Color32, CursorIcon, FontId, Id, Rect, Sense, Stroke, StrokeKind, TextEdit, Ui, pos2,
    vec2,
};
use servo::{
    AllowOrDenyRequest, AuthenticationRequest, ContextMenuAction, ContextMenuItem, EmbedderControl,
    EmbedderControlId, RgbColor, SelectElementOptionOrOptgroup, SimpleDialog,
};

use crate::glass::{self, Glass};
use crate::theme::{Palette, Tier};

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
    pub response: servo::UnsupportedResponse,
    pub filename: String,
}

/// A `beforeunload` the page wants confirmed before it is navigated away from.
struct Unload(AllowOrDenyRequest);

/// A capability a page asked for, and the host that asked.
struct Permission {
    request: servo::PermissionRequest,
    host: String,
}

pub struct PageCoordinates {
    pub bounds: Rect,
    pub points_per_device_pixel: f32,
    pub points_per_css_pixel: f32,
}

impl PageCoordinates {
    fn to_window(&self, anchor: servo::DeviceIntRect, points_per_pixel: f32) -> Rect {
        Rect::from_min_max(
            self.bounds.min + vec2(anchor.min.x as f32, anchor.min.y as f32) * points_per_pixel,
            self.bounds.min + vec2(anchor.max.x as f32, anchor.max.y as f32) * points_per_pixel,
        )
    }
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
    pub fn push_offer(&mut self, mut response: servo::UnsupportedResponse, filename: String) {
        if self.offer.is_some() {
            response.decline();
            return;
        }
        let host = response
            .url
            .host_str()
            .unwrap_or(tr("dialog-this-page"))
            .to_owned();
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
            |ui| {
                let top = ui.max_rect().min;
                ui.painter().text(
                    top,
                    Align2::LEFT_TOP,
                    tr("dialog-leave-this-page"),
                    FontId::proportional(14.0),
                    palette.text,
                );
                ui.painter().text(
                    top + vec2(0.0, 26.0),
                    Align2::LEFT_TOP,
                    tr("dialog-anything-you-have-typed-and-not-sent"),
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
                stay |= button(&mut child, tr("dialog-stay"), palette);
                child.add_space(8.0);
                leave = button(&mut child, tr("dialog-leave"), palette);
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
        let prompt = permission_prompt(pending.request.feature());
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
            |ui| {
                let top = ui.max_rect().min;
                ui.painter().text(
                    top,
                    Align2::LEFT_TOP,
                    crate::l10n::message(prompt, [("host", host.into())]),
                    FontId::proportional(14.0),
                    palette.text,
                );
                ui.painter().text(
                    top + vec2(0.0, 26.0),
                    Align2::LEFT_TOP,
                    tr("dialog-you-can-change-your-mind-later-in"),
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
                allow = button(&mut child, tr("dialog-allow"), palette);
                child.add_space(8.0);
                deny |= button(&mut child, tr("dialog-block"), palette);
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
            |ui| {
                let top = ui.max_rect().min;
                ui.painter().text(
                    top,
                    Align2::LEFT_TOP,
                    crate::l10n::message("dialog-site-download", [("host", host.into())]),
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
                    tr("dialog-it-goes-to-your-downloads-folder"),
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
                allow = button(&mut child, tr("common-save"), palette);
                child.add_space(8.0);
                deny |= button(&mut child, tr("common-cancel"), palette);
            },
        );

        if !(allow || deny) {
            return;
        }
        let Some(offer) = self.offer.take() else {
            return;
        };
        let mut response = offer.response;
        if !allow {
            response.decline();
            return;
        }
        self.accepted.push(Accepted {
            response,
            filename: offer.filename,
        });
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
            |ui| {
                let top = ui.max_rect().min;
                ui.painter().text(
                    top,
                    Align2::LEFT_TOP,
                    crate::l10n::message("dialog-http-auth", [("host", asking.as_str().into())]),
                    FontId::proportional(14.0),
                    palette.text,
                );
                // Both names, always. When they differ it is because a login
                // saved for a domain is being offered to a host underneath it,
                // and that is the case worth looking at twice.
                let detail = if saved_for == asking {
                    crate::l10n::message(
                        "dialog-http-saved-password",
                        [("username", username.into())],
                    )
                } else {
                    crate::l10n::message(
                        "dialog-http-saved-origin",
                        [("username", username.into()), ("host", saved_for.into())],
                    )
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
                    tr("dialog-the-page-asked-for-this-not-zervo"),
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
                allow = button(&mut child, tr("dialog-sign-in"), palette);
                child.add_space(8.0);
                deny |= button(&mut child, tr("dialog-not-now"), palette);
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
    /// `origin` labels dialogs, and `page` maps Servo's anchors to window points.
    pub fn draw(
        &mut self,
        root: &mut Ui,
        palette: &Palette,
        page: PageCoordinates,
        origin: &str,
        vault: &crate::passwords::Vault,
    ) {
        let content_rect = page.bounds;
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
                // Servo currently sends element bounds as CSS pixels.
                let anchor = page.to_window(select.position(), page.points_per_css_pixel);
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
                let anchor = page.to_window(picker.position(), page.points_per_css_pixel);
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
                let anchor = page.to_window(menu.position(), page.points_per_device_pixel);
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

const PANEL_PADDING: f32 = 14.0;

/// A floating glass panel with an optional scrim beneath it.
fn panel<R>(
    ctx: &egui::Context,
    id: &str,
    palette: &Palette,
    scrim: Option<Rect>,
    rect: Rect,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let fallback = scrim.map_or_else(|| palette.over(rect), |_| *palette);
    egui::Area::new(Id::unique(id))
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
                    crate::theme::content_corners(palette),
                    dim(palette),
                );
            }
            let role = if scrim.is_some() {
                crate::overlay_glass::Role::Dialog
            } else {
                crate::overlay_glass::Role::Menu
            };
            let surface = crate::overlay_glass::Surface::begin(ui);
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
                        .radius_exact(role.radius())
                        .opaque(palette.bg)
                        .border(palette.border),
                );
            }
            let mut inner =
                ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(PANEL_PADDING)));
            let value = add(&mut inner);
            surface.finish(ui, rect, role);
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
fn button(ui: &mut Ui, label: &str, palette: &Palette) -> bool {
    crate::widgets::label_button(ui, palette, label, 28.0, 26.0, false).clicked()
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
    let galley = measure.layout(
        dialog.message().to_owned(),
        FontId::proportional(13.5),
        palette.text,
        width - 28.0,
    );
    let prompt = matches!(dialog, SimpleDialog::Prompt(_));
    let fixed = PANEL_PADDING * 2.0 + 18.0 + if prompt { 42.0 } else { 0.0 } + 40.0;
    let max_height = (content_rect.height() - 24.0).min(480.0);
    let body_height = galley.size().y.min((max_height - fixed).max(0.0));
    let rect = Rect::from_center_size(content_rect.center(), vec2(width, fixed + body_height));

    let mut resolution = None;
    panel(
        ctx,
        "zervo_page_dialog",
        palette,
        Some(content_rect),
        rect,
        |ui| {
            // Say who is asking. These messages are written by the page, and
            // must not read as though Zervo is asking.
            ui.painter().text(
                ui.max_rect().min,
                Align2::LEFT_TOP,
                crate::l10n::message("dialog-site-says", [("origin", origin.into())]),
                FontId::proportional(11.5),
                palette.text_muted,
            );
            let message = Rect::from_min_size(
                ui.max_rect().min + vec2(0.0, 18.0),
                vec2(ui.max_rect().width(), body_height),
            );
            if galley.size().y > body_height {
                crate::overlay_glass::scroll_body(ui, message, |ui| {
                    ui.label(
                        egui::RichText::new(dialog.message())
                            .size(13.5)
                            .color(palette.text),
                    );
                });
            } else {
                ui.painter()
                    .galley(message.min, galley.clone(), palette.text);
            }

            let mut confirmed = enter;
            if prompt {
                let field = Rect::from_min_size(
                    ui.max_rect().min + vec2(0.0, 22.0 + body_height),
                    vec2(ui.max_rect().width(), 30.0),
                );
                let SimpleDialog::Prompt(prompt) = dialog else {
                    unreachable!("checked above")
                };
                let mut value = prompt.current_value().to_owned();
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
                let material = crate::overlay_glass::Surface::begin(&child);
                let response = child.add_sized(
                    field.size(),
                    material.text_edit(
                        TextEdit::singleline(&mut value).font(FontId::proportional(13.0)),
                    ),
                );
                material.finish(&child, field, crate::overlay_glass::Role::Field);
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
            if button(&mut child, tr("common-ok"), palette) || confirmed {
                resolution = Some(Resolution::Accept);
            }
            let cancellable = !matches!(dialog, SimpleDialog::Alert(_));
            if cancellable {
                child.add_space(8.0);
                if button(&mut child, tr("common-cancel"), palette) || escape {
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
    let footer = if multiple { 36.0 } else { 0.0 };
    let max_height = (content_rect.height() * 0.65).min(352.0);
    let height = (rows.max(1.0) * ROW + PANEL_PADDING * 2.0 + footer).min(max_height);
    let rect = crate::ui::anchored_popup(anchor, vec2(width, height), content_rect, false);

    let mut picked = None;
    panel(ctx, "zervo_select", palette, None, rect, |ui| {
        let list = ui.max_rect().with_max_y(ui.max_rect().bottom() - footer);
        crate::overlay_glass::scroll_body(ui, list, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
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
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
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
            if button(&mut child, tr("common-done"), palette) {
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
    let colour = crate::widgets::button_ink(ui, &response, rect, palette, selected);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::ListBoxOption, !disabled, selected, label)
    });
    crate::overlay_glass::row_label(
        ui,
        rect.with_min_x(rect.left() + 8.0 + indent)
            .with_max_x(rect.right() - 8.0),
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
    let columns =
        (((content_rect.width() - 16.0 - 2.0 * PANEL_PADDING + 4.0) / CELL) as usize).clamp(1, 6);

    let rows = SWATCHES.len().div_ceil(columns) as f32;
    let rect = crate::ui::anchored_popup(
        anchor,
        vec2(
            columns as f32 * CELL + 2.0 * PANEL_PADDING - 4.0,
            rows * CELL + 2.0 * PANEL_PADDING - 4.0,
        ),
        content_rect,
        false,
    );

    let mut result = None;
    panel(ctx, "zervo_color_picker", palette, None, rect, |ui| {
        for (index, (red, green, blue)) in SWATCHES.iter().enumerate() {
            let column = index % columns;
            let row = index / columns;
            let cell = Rect::from_min_size(
                ui.max_rect().min + vec2(column as f32 * CELL, row as f32 * CELL),
                vec2(CELL - 4.0, CELL - 4.0),
            );
            let response = ui.interact(cell, ui.scope_id().with(index), Sense::click());
            ui.painter().rect_filled(
                cell,
                palette.corner(Tier::Control),
                Color32::from_rgb(*red, *green, *blue),
            );
            let chosen = current.is_some_and(|colour| {
                (colour.red, colour.green, colour.blue) == (*red, *green, *blue)
            });
            if chosen || response.hovered() {
                ui.painter().rect_stroke(
                    cell,
                    palette.corner(Tier::Control),
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

    let mut height = PANEL_PADDING * 2.0;
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
                width = width.max(measured + 2.0 * PANEL_PADDING + 16.0);
            },
        }
    }
    let rect = crate::ui::clamp_into(
        Rect::from_min_size(anchor.min, vec2(width.min(420.0), height)),
        content_rect,
    );

    let mut result = None;
    panel(ctx, "zervo_context_menu", palette, None, rect, |ui| {
        crate::overlay_glass::scroll_body(ui, ui.max_rect(), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for item in items {
                match item {
                    ContextMenuItem::Separator => {
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), SEPARATOR),
                            Sense::hover(),
                        );
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
                                palette.corner(Tier::Control),
                                palette.surface_hover,
                            );
                        }
                        let colour = if *enabled {
                            palette.text
                        } else {
                            palette.text_muted.gamma_multiply(0.6)
                        };
                        crate::overlay_glass::row_label(
                            ui,
                            rect.shrink2(vec2(8.0, 0.0)),
                            label,
                            FontId::proportional(13.0),
                            colour,
                        );
                        if *enabled && response.on_hover_cursor(CursorIcon::PointingHand).clicked()
                        {
                            result = Some(Some(*action));
                        }
                    },
                }
            }
        });
    });
    if result.is_none() && settled && clicked_outside(ctx, rect) {
        result = Some(None);
    }
    result
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
fn permission_prompt(feature: servo::PermissionFeature) -> &'static str {
    use servo::PermissionFeature as F;
    match feature {
        F::Geolocation => "dialog-permission-location",
        F::Notifications => "dialog-permission-notifications",
        F::Push => "dialog-permission-push",
        F::Midi => "dialog-permission-midi",
        F::Camera => "dialog-permission-camera",
        F::Microphone => "dialog-permission-microphone",
        F::Speaker => "dialog-permission-speaker",
        F::DeviceInfo => "dialog-permission-device-info",
        F::BackgroundSync => "dialog-permission-background-sync",
        F::Bluetooth => "dialog-permission-bluetooth",
        F::PersistentStorage => "dialog-permission-persistent-storage",
        F::ScreenWakeLock(_) => "dialog-permission-screen-wake-lock",
        F::Gamepad => "dialog-permission-gamepad",
        // Deliberately exhaustive, with no catch-all: when Servo adds a
        // feature this stops compiling, which is the moment to decide what to
        // call it rather than shipping "a permission".
    }
}
