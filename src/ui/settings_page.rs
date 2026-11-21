//! `zervo://settings`.
//!
//! Every section of it — appearance, general, layout, passwords, about — and
//! the navigation column down the left. This is where the great majority of
//! `chrome.settings` is read and written; almost nowhere else touches it.

use crate::l10n::text as tr;
use egui::{
    Align2, CornerRadius, CursorIcon, FontId, Frame, Id, Margin, Rect, RichText, Sense, Shape,
    Stroke, TextEdit, Ui, pos2, vec2,
};

use crate::glass::{self, Glass};
use crate::icons::{self, Icon};
use crate::settings::{AppIcon, NewTabBackground, NewTabPage, SearchEngine};
use crate::theme::{self, AccentColor, Palette, ThemeMode, Tier};
use crate::widgets;

use super::*;

/// How tall the pinned specimen is on the Appearance page.
///
/// Enough for a card, an input and a strip of chrome side by side at a
/// readable size, and no more: it is a reference, not the page.
const SPECIMEN_HEIGHT: f32 = 190.0;

pub(crate) fn draw_settings_page(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    content_rect: Rect,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let painter = root.ctx().layer_painter(egui::LayerId::background());
    let card_radius = theme::content_corners(&palette);
    painter.rect_filled(content_rect, card_radius, palette.surface);

    // The page's own contents start below the window's controls. The colour
    // above still runs to the window's edge — it is the *heading* that has to
    // move, not the page.
    let laid_out = Rect::from_min_max(
        pos2(
            content_rect.min.x,
            content_rect.min.y + chrome.window_controls_room(),
        ),
        content_rect.max,
    );

    // ── Left navigation column, Chrome-style: categories on the left, the
    // selected category's panel on the right.
    let nav_width = 188.0_f32.min(content_rect.width() * 0.34);
    let nav_rect = Rect::from_min_size(laid_out.min, vec2(nav_width, laid_out.height()));
    painter.rect_filled(
        // The column's colour runs the full height of the page even where its
        // contents start lower, or full-page mode would show a bite out of the
        // top of it.
        Rect::from_min_max(content_rect.min, pos2(nav_rect.max.x, content_rect.max.y)),
        CornerRadius {
            nw: card_radius.nw,
            sw: card_radius.sw,
            ne: 0,
            se: 0,
        },
        theme::mix(palette.surface, palette.bg, 0.75),
    );
    // The page as a menu opened over it will see it: the base and the
    // navigation column, and none of the controls that are about to be drawn
    // on them.
    if let Some(capture) = &chrome.capture {
        crate::backdrop::capture_into(&painter, content_rect, capture);
    }

    let mut nav_ui = root.new_child(
        egui::UiBuilder::new()
            .max_rect(nav_rect.shrink2(vec2(10.0, 14.0)))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    {
        let ui = &mut nav_ui;
        ui.label(
            RichText::new(tr("common-settings"))
                .size(17.0)
                .strong()
                .color(palette.text),
        );
        ui.add_space(10.0);
        for section in crate::state::SettingsSection::ALL {
            let selected = chrome.browser.settings_section == section;
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
            let hover_t = glass::ease_out(ui.ctx().animate_bool_with_time(
                ui.id().with(("settings_nav", section)),
                response.hovered() && !selected,
                0.12,
            ));
            let select_t = glass::ease_out(ui.ctx().animate_bool_with_time(
                ui.id().with(("settings_nav_sel", section)),
                selected,
                0.18,
            ));
            if hover_t > 0.0 {
                ui.painter().rect_filled(
                    rect,
                    palette.corner(Tier::Row),
                    palette.surface_hover.gamma_multiply(hover_t),
                );
            }
            if select_t > 0.0 {
                ui.painter().rect_filled(
                    rect,
                    palette.corner(Tier::Row),
                    palette.active.gamma_multiply(select_t),
                );
            }
            let ink = if selected {
                palette.text
            } else {
                palette.text_muted
            };
            let mut label = egui::text::LayoutJob::simple_singleline(
                section.label().to_owned(),
                FontId::proportional(13.5),
                ink,
            );
            label.wrap =
                egui::text::TextWrapping::truncate_at_width((rect.width() - 20.0).max(0.0));
            let label = ui.painter().layout_job(label);
            let response = if label.elided {
                response.on_hover_text(section.label())
            } else {
                response
            };
            ui.painter().galley(
                pos2(rect.min.x + 12.0, rect.center().y - label.size().y / 2.0),
                label,
                ink,
            );
            if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                chrome.browser.settings_section = section;
            }
        }
    }

    // ── Content pane: a pinned header, and sections that scroll under it.
    let pane_rect = Rect::from_min_max(pos2(nav_rect.max.x, laid_out.min.y), laid_out.max)
        .shrink2(vec2(6.0, 10.0));
    let column = (pane_rect.width() - 40.0).clamp(240.0, 520.0);
    let margin = ((pane_rect.width() - column) * 0.5).max(0.0);

    // The title sits outside the scroll area, and on Appearance the specimen
    // sits under it. Blur, fill, sheen and the edge are invisible on a page
    // made of settings rows — you need a card, a menu and something to type
    // into, side by side, to judge any of them — and a control adjusted with
    // nothing to look at is a control adjusted blind.
    let specimen = matches!(
        chrome.browser.settings_section,
        crate::state::SettingsSection::Appearance
    );
    let head_height = 14.0
        + 26.0
        + if specimen {
            12.0 + SPECIMEN_HEIGHT
        } else {
            0.0
        };
    {
        let mut head = root.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    pos2(pane_rect.min.x + margin, pane_rect.min.y + 14.0),
                    pos2(
                        pane_rect.min.x + margin + column,
                        pane_rect.min.y + head_height,
                    ),
                ))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        head.set_width(column);
        head.label(
            RichText::new(chrome.browser.settings_section.label())
                .size(21.0)
                .strong()
                .color(palette.text),
        );
        if specimen {
            head.add_space(12.0);
            let (rect, _) = head.allocate_exact_size(vec2(column, SPECIMEN_HEIGHT), Sense::hover());
            appearance_specimen(&head, &palette, rect);
        }
    }

    // A gap between the pinned header and what scrolls under it.
    //
    // They used to touch exactly, which is not the same as not overlapping:
    // every section is a glass card and glass casts a shadow *outside* itself,
    // so the first card's — and then every card's, as they scrolled past —
    // fell across the specimen's bottom edge and smeared it. The gap is
    // outside the scroll viewport, so it survives scrolling; a space inside it
    // would scroll away with the first card.
    const HEADER_GAP: f32 = 16.0;
    let mut pane_ui = root.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(pane_rect.min.x, pane_rect.min.y + head_height + HEADER_GAP),
                pane_rect.max,
            ))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let ui = &mut pane_ui;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_width(column);
                    ui.add_space(14.0);
                    match chrome.browser.settings_section {
                        crate::state::SettingsSection::Appearance => {
                            settings_appearance(ui, chrome, &palette, actions);
                        },
                        crate::state::SettingsSection::General => {
                            settings_general(ui, chrome, &palette, actions);
                        },
                        crate::state::SettingsSection::Layout => {
                            settings_layout(ui, chrome, &palette, actions);
                        },
                        crate::state::SettingsSection::Passwords => {
                            settings_passwords(ui, chrome, actions);
                        },
                        crate::state::SettingsSection::About => settings_about(ui, &palette),
                    }
                    ui.add_space(18.0);
                });
            });
        });
}

/// "Save as a preset…", and the field it opens.
///
/// The five presets are constants and stay constants — they are the ones the
/// study argues for. This is the other half of the answer to "a theme is a
/// Rust constant": not only is every value settable, an arrangement somebody
/// tuned can be put somewhere and come back.
fn save_as_preset(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    let open_id = Id::new("zervo_preset_naming");
    let draft_id = Id::new("zervo_preset_name");
    if ghost_button(ui, palette, Icon::Plus, tr("settings-save-as-a-preset")).clicked() {
        let open = ui.ctx().data(|data| data.get_temp::<bool>(open_id)) == Some(true);
        ui.ctx().data_mut(|data| data.insert_temp(open_id, !open));
    }
    if ui.ctx().data(|data| data.get_temp::<bool>(open_id)) != Some(true) {
        return;
    }
    let existing = ui.ctx().data(|data| data.get_temp::<String>(draft_id));
    // Only on the frame it opens. Asking every frame would mean the field
    // snatching focus back the moment anybody clicked away from it, which is a
    // text box holding the page hostage over a name it does not need.
    let opening = existing.is_none();
    let mut name = existing.unwrap_or_default();
    let field = ui.add(
        TextEdit::singleline(&mut name)
            .font(FontId::proportional(12.5))
            .hint_text(RichText::new(tr("settings-name-it")).color(palette.text_muted))
            .desired_width(150.0),
    );
    if opening {
        field.request_focus();
    }
    let entered = field.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
    if entered && !name.trim().is_empty() {
        let mut appearance = chrome.settings.appearance;
        // A saved arrangement is the reader's own by definition. Carrying a
        // preset tag into one would have it claim to be a preset it is only
        // descended from.
        appearance.customised();
        chrome.settings.saved.push(crate::settings::Saved {
            name: name.trim().to_owned(),
            appearance,
        });
        actions.push(UiAction::SettingsChanged);
        ui.ctx().data_mut(|data| {
            data.insert_temp(open_id, false);
            // Removed rather than blanked, so reopening the field is an
            // opening again and takes focus.
            data.remove_temp::<String>(draft_id);
        });
    } else {
        ui.ctx().data_mut(|data| data.insert_temp(draft_id, name));
    }
}

/// A pill button in the preset row.
///
/// Not `widgets::segmented`: five names of unequal length in equal slots reads
/// as a table of contents rather than as five things to press, and the row has
/// to wrap on a narrow window.
fn preset_button(ui: &mut Ui, palette: &Palette, label: &str, selected: bool) -> egui::Response {
    let text =
        ui.painter()
            .layout_no_wrap(label.to_owned(), FontId::proportional(12.0), palette.text);
    let (rect, response) = ui.allocate_exact_size(vec2(text.size().x + 26.0, 30.0), Sense::click());
    let lit = glass::ease_out(ui.ctx().animate_bool_with_time(
        response.id,
        selected,
        palette.material.animation,
    ));
    let hover = glass::ease_out(ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() && !selected,
        0.12,
    ));
    if lit > 0.0 || hover > 0.0 {
        glass::paint(
            ui.painter(),
            rect,
            palette,
            Glass::tier(Tier::Row)
                .strength(0.55 + 0.45 * lit.max(hover))
                .tint(if lit > 0.5 {
                    palette.active
                } else {
                    palette.surface
                })
                .glow(palette.appearance.glow * lit)
                .no_shadow(),
        );
    }
    ui.painter().galley(
        pos2(
            rect.center().x - text.size().x * 0.5,
            rect.center().y - text.size().y * 0.5,
        ),
        text,
        if selected {
            palette.text
        } else {
            palette.text_muted
        },
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// The muted line that goes under a control and says what it does.
fn note(ui: &mut Ui, palette: &Palette, text: impl Into<String>) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(text.into())
            .size(11.5)
            .color(palette.text_muted),
    );
}

/// A heading inside a section, for the sections that hold more than one thing.
fn subhead(ui: &mut Ui, palette: &Palette, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(palette.text));
    ui.add_space(6.0);
}

/// A segmented control over a fixed set of variants. True when it moved.
fn choose<T: PartialEq + Copy>(
    ui: &mut Ui,
    palette: &Palette,
    value: &mut T,
    options: &[T],
    label: impl Fn(T) -> &'static str,
) -> bool {
    let labels: Vec<&str> = options.iter().map(|option| label(*option)).collect();
    let current = options
        .iter()
        .position(|option| option == value)
        .unwrap_or(0);
    match widgets::segmented(ui, current, &labels, palette) {
        Some(index) if options[index] != *value => {
            *value = options[index];
            true
        },
        _ => false,
    }
}

/// A labelled slider with its reading in the corner.
///
/// The reading is painted after the slider has run rather than before it: on a
/// drag the two are otherwise a frame apart, and a number trailing the handle
/// it belongs to is the kind of small wrongness nobody can name and everybody
/// sees. Same reserve-and-backfill the section cards use.
fn tuning(
    ui: &mut Ui,
    palette: &Palette,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    reading: impl Fn(f32) -> String,
) -> widgets::SliderOut {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    ui.painter().text(
        pos2(row.min.x, row.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        palette.text,
    );
    let slot = ui.painter().add(Shape::Noop);
    let out = widgets::slider(ui, value, range, palette);
    let galley =
        ui.painter()
            .layout_no_wrap(reading(*value), FontId::monospace(11.0), palette.accent);
    ui.painter().set(
        slot,
        Shape::galley(
            pos2(
                row.max.x - galley.size().x,
                row.center().y - galley.size().y * 0.5,
            ),
            galley,
            palette.accent,
        ),
    );
    out
}

/// A ghost button in the "This arrangement" row.
fn ghost_button(ui: &mut Ui, palette: &Palette, icon: Icon, label: &str) -> egui::Response {
    let text =
        ui.painter()
            .layout_no_wrap(label.to_owned(), FontId::proportional(12.0), palette.text);
    let (rect, response) = ui.allocate_exact_size(vec2(text.size().x + 44.0, 30.0), Sense::click());
    let hover = glass::ease_out(ui.ctx().animate_bool_with_time(
        response.id,
        response.hovered(),
        0.12,
    ));
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(palette.radius(Tier::Row)),
        Stroke::new(1.0_f32, palette.border),
        egui::StrokeKind::Inside,
    );
    if hover > 0.0 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(palette.radius(Tier::Row)),
            palette.surface_hover.gamma_multiply(hover * 0.7),
        );
    }
    icons::draw_icon(
        ui.painter(),
        Rect::from_center_size(pos2(rect.min.x + 17.0, rect.center().y), vec2(13.0, 13.0)),
        icon,
        palette.text_muted,
    );
    ui.painter().galley(
        pos2(rect.min.x + 30.0, rect.center().y - text.size().y * 0.5),
        text,
        palette.text,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// The specimen that sits pinned under the Appearance title.
///
/// Blur, fill, sheen and the edge are invisible on a page made of settings
/// rows — you need a card, a menu and something to type into, side by side, to
/// judge any of them. It is the real material rather than a picture of one:
/// every surface below goes through `glass::shapes` with the palette the
/// chrome itself is being drawn with, so it is wrong exactly when the chrome
/// is wrong.
pub(crate) fn appearance_specimen(ui: &Ui, palette: &Palette, rect: Rect) {
    let painter = ui.painter().with_clip_rect(rect);
    // The specimen is a picture of a *window*, so its outer corner is the
    // window's — the platform's, where the platform has one. It is therefore
    // the one corner on this page that does not move with the corner scale,
    // which is exactly true of the real window as well.
    //
    // It is drawn against the arrangement rather than the layout: a reader
    // looking at this while in full-page mode is being shown what the seam
    // setting does, not where their window happens to be.
    let framed = palette.filling_window(false);
    let palette = &framed;
    let radius = theme::window_radius(palette) as u8;
    // A stand-in for a wallpaper: the accent going one way and the chrome the
    // other, so there is something behind the glass worth seeing through it.
    //
    // Twenty-four vertical bands, and only the two on the ends have corners.
    // They used to be square to a band, painted over a rounded ground and
    // followed by `rect_filled(rect, corner, Color32::TRANSPARENT)` — which
    // draws nothing at all, transparent fill being no pixels. So the specimen
    // was a rectangle with rounded furniture inside it, and every inner
    // surface's corner had the ground's square one showing beside it.
    const BANDS: usize = 24;
    for step in 0..BANDS {
        // The band's own span, and separately how far through the gradient it
        // is. Running one number for both is what left the right-hand corners
        // square: at `t = step / (BANDS - 1)` the last band starts exactly at
        // the right edge and is drawn entirely outside the specimen, so the
        // band that actually paints that edge — the one before it — carried no
        // corners at all.
        let from = step as f32 / BANDS as f32;
        let to = (step + 1) as f32 / BANDS as f32;
        let band = Rect::from_min_max(
            pos2(rect.min.x + rect.width() * from, rect.min.y),
            pos2(rect.min.x + rect.width() * to, rect.max.y),
        );
        let last = step + 1 == BANDS;
        let ends = CornerRadius {
            nw: if step == 0 { radius } else { 0 },
            sw: if step == 0 { radius } else { 0 },
            ne: if last { radius } else { 0 },
            se: if last { radius } else { 0 },
        };
        let shade = step as f32 / (BANDS - 1) as f32;
        painter.rect_filled(
            band,
            ends,
            theme::mix(
                theme::mix(palette.bg, palette.accent, 0.55),
                theme::mix(palette.bg, theme::workspace_color(2), 0.45),
                shade,
            )
            .gamma_multiply(0.9),
        );
    }

    let seam = palette.appearance.seam;
    let gap = theme::content_margin(palette) * 0.6;
    let chrome_width = 104.0;
    let chrome = Rect::from_min_max(rect.min, pos2(rect.min.x + chrome_width, rect.max.y));
    // The chrome's own tint, laid on the same ground the page is on — which is
    // the whole of what the seam setting decides.
    painter.rect_filled(
        chrome,
        CornerRadius {
            nw: radius,
            sw: radius,
            ne: 0,
            se: 0,
        },
        palette.bg.gamma_multiply(if seam.chrome_floats() {
            palette.chrome_tint().max(0.42)
        } else {
            1.0
        }),
    );

    let page = Rect::from_min_max(
        pos2(
            chrome.max.x + if seam.closes_gap() { 0.0 } else { gap },
            rect.min.y + gap,
        ),
        pos2(rect.max.x - gap, rect.max.y - gap),
    );
    if seam.page_paints_base() {
        // The same rule `theme::content_corners` states for the real page: an
        // inset card rounds on all four at the panel's radius; one flush with
        // the window rounds only where it touches the window, and squares off
        // against the chrome beside it.
        let page_corner = if seam.closes_gap() {
            CornerRadius {
                nw: 0,
                sw: 0,
                ne: radius,
                se: radius,
            }
        } else {
            CornerRadius::same(palette.radius(Tier::Panel))
        };
        painter.rect_filled(page, page_corner, theme::page_base(palette));
    }

    // Left: the chrome. Mini traffic lights, a pill, a lit tab, two rows.
    let mut y = rect.min.y + 10.0;
    for (index, colour) in [
        Color32::from_rgb(255, 95, 87),
        Color32::from_rgb(254, 188, 46),
        Color32::from_rgb(40, 200, 64),
    ]
    .into_iter()
    .enumerate()
    {
        painter.circle_filled(
            pos2(chrome.min.x + 12.0 + index as f32 * 11.0, y + 3.5),
            3.5,
            colour,
        );
    }
    y += 18.0;
    let inset = |top: f32, height: f32| {
        Rect::from_min_max(
            pos2(chrome.min.x + 7.0, top),
            pos2(chrome.max.x - 7.0, top + height),
        )
    };
    glass::paint(
        &painter,
        inset(y, 19.0),
        palette,
        Glass::of(Surface::Input).no_shadow(),
    );
    painter.text(
        pos2(chrome.min.x + 13.0, y + 9.5),
        Align2::LEFT_CENTER,
        "zervo://newtab",
        FontId::proportional(8.0),
        palette.text_muted,
    );
    y += 24.0;
    glass::paint(
        &painter,
        inset(y, 19.0),
        palette,
        Glass::tier(Tier::Row)
            .tint(palette.active)
            .glow(palette.appearance.glow)
            .no_shadow(),
    );
    painter.text(
        pos2(chrome.min.x + 13.0, y + 9.5),
        Align2::LEFT_CENTER,
        tr("settings-servo-aims-to"),
        FontId::proportional(8.0),
        palette.text,
    );
    for label in [tr("common-settings"), tr("common-new-tab")] {
        y += 21.0;
        painter.text(
            pos2(chrome.min.x + 13.0, y + 8.5),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(8.0),
            palette.text_muted,
        );
    }

    // Right: the page. A card and something to type into, which between them
    // show every value the sliders move.
    let ink = palette.over(page);
    let card = Rect::from_min_max(
        pos2(page.min.x + 16.0, page.center().y - 34.0),
        pos2(page.max.x - 16.0, page.center().y + 4.0),
    );
    glass::paint(&painter, card, &ink, Glass::of(Surface::Card));
    painter.text(
        pos2(card.min.x + 11.0, card.center().y),
        Align2::LEFT_CENTER,
        tr("common-most-visited"),
        FontId::proportional(9.0),
        ink.text_muted,
    );
    let field = Rect::from_min_max(
        pos2(page.min.x + 16.0, page.center().y + 14.0),
        pos2(page.max.x - 16.0, page.center().y + 40.0),
    );
    glass::paint(&painter, field, &ink, Glass::of(Surface::Input));
    painter.text(
        pos2(field.min.x + 11.0, field.center().y),
        Align2::LEFT_CENTER,
        tr("common-search-hint"),
        FontId::proportional(9.0),
        ink.text_muted,
    );

    painter.text(
        pos2(rect.max.x - 9.0, rect.max.y - 8.0),
        Align2::RIGHT_BOTTOM,
        palette.appearance.preset_label().to_uppercase(),
        FontId::monospace(9.0),
        ink.text_muted,
    );
}

pub(crate) fn settings_appearance(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    // Every control below writes one field of the arrangement, and the moment
    // any of them moves, the row of presets at the top is naming something
    // that is no longer true. Rather than have twenty call sites remember to
    // say so, the page is compared against what it started the frame as.
    let opened_as = chrome.settings.appearance;
    let mut chose_preset = false;

    settings_section(ui, palette, tr("settings-preset"), |ui| {
        ui.horizontal_wrapped(|ui| {
            for preset in theme::Preset::ALL {
                let selected = chrome.settings.appearance.preset == Some(preset);
                if preset_button(ui, palette, preset.label(), selected).clicked() && !selected {
                    chrome.settings.appearance = preset.appearance();
                    chose_preset = true;
                    actions.push(UiAction::SettingsChanged);
                }
            }
        });
        // The reader's own, after the five. Each carries its own way of being
        // thrown away, because a list that can only grow is a list that stops
        // being used.
        if !chrome.settings.saved.is_empty() {
            ui.add_space(8.0);
            let mut remove = None;
            let mut apply = None;
            ui.horizontal_wrapped(|ui| {
                for (index, saved) in chrome.settings.saved.iter().enumerate() {
                    let selected = chrome.settings.appearance.same_look(&saved.appearance);
                    let response = preset_button(ui, palette, &saved.name, selected);
                    if response.clicked() && !selected {
                        apply = Some(saved.appearance);
                    }
                    if response.secondary_clicked() {
                        remove = Some(index);
                    }
                    response.on_hover_text(tr("settings-right-click-to-forget-this-one"));
                }
            });
            if let Some(appearance) = apply {
                chrome.settings.appearance = appearance;
                chose_preset = true;
                actions.push(UiAction::SettingsChanged);
            }
            if let Some(index) = remove {
                chrome.settings.saved.remove(index);
                actions.push(UiAction::SettingsChanged);
            }
        }
        note(
            ui,
            palette,
            match chrome.settings.appearance.preset {
                Some(preset) => preset.note(),
                None => tr("settings-custom-every-value-below-is-yours-pick"),
            },
        );
    });

    settings_section(ui, palette, tr("common-theme"), |ui| {
        if choose(
            ui,
            palette,
            &mut chrome.settings.theme,
            &ThemeMode::ALL,
            |mode| mode.label(),
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        note(
            ui,
            palette,
            tr("settings-auto-follows-the-system-appearance-and-day"),
        );
    });

    settings_section(ui, palette, tr("settings-accent-colour"), |ui| {
        accent_swatches(ui, chrome, palette, actions);

        ui.add_space(10.0);
        if widgets::toggle(
            ui,
            &mut chrome.settings.appearance.workspace_accent,
            tr("settings-take-the-accent-from-the-active-workspace"),
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        note(
            ui,
            palette,
            tr("settings-the-workspace-s-own-colour-instead-of"),
        );

        ui.add_space(10.0);
        let candy = &mut chrome.settings.appearance.candy;
        if tuning(
            ui,
            palette,
            tr("settings-accent-strength-in-the-chrome"),
            candy,
            0.0..=0.4,
            |value| format!("{value:.3}"),
        )
        .settled
        {
            actions.push(UiAction::SettingsChanged);
        }
        let candy = chrome.settings.appearance.candy;
        note(
            ui,
            palette,
            if candy <= 0.06 {
                tr("settings-what-shipped-is-0-045-quiet-enough")
            } else if candy >= 0.24 {
                tr("settings-the-chrome-is-unmistakably-coloured-and-the")
            } else {
                tr("settings-the-accent-reads-as-a-room-rather")
            },
        );
    });

    settings_section(
        ui,
        palette,
        tr("settings-seam-between-chrome-and-page"),
        |ui| {
            if choose(
                ui,
                palette,
                &mut chrome.settings.appearance.seam,
                &theme::Seam::ALL,
                zervo_core::theme::Seam::label,
            ) {
                actions.push(UiAction::SettingsChanged);
            }
            note(ui, palette, chrome.settings.appearance.seam.note());

            if !chrome.settings.appearance.seam.closes_gap() {
                ui.add_space(10.0);
                if tuning(
                    ui,
                    palette,
                    tr("settings-gap-around-the-content"),
                    &mut chrome.settings.appearance.gap,
                    0.0..=20.0,
                    |value| format!("{value:.0}pt"),
                )
                .settled
                {
                    actions.push(UiAction::SettingsChanged);
                }
                note(
                    ui,
                    palette,
                    tr("settings-content-margin-zero-puts-the-page-against"),
                );
            }
        },
    );

    settings_section(ui, palette, tr("settings-material"), |ui| {
        if choose(
            ui,
            palette,
            &mut chrome.settings.appearance.translucency,
            &theme::Translucency::ALL,
            zervo_core::theme::Translucency::label,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        let translucency = chrome.settings.appearance.translucency;
        let material_note = match translucency {
            _ if translucency.is_translucent()
                && palette.translucency == theme::Translucency::Solid =>
            {
                tr("settings-macos-reduce-transparency")
            },
            theme::Translucency::Frosted
                if chrome.native_glass.sidebar_available && chrome.settings.layout.is_sidebar() =>
            {
                tr("settings-native-docked-sidebar")
            },
            theme::Translucency::LiquidGlass
                if chrome.native_glass.sidebar_available && chrome.native_glass.glass_available =>
            {
                if chrome.settings.layout.is_sidebar() {
                    tr("settings-native-docked-sidebar-and-address-field")
                } else {
                    tr("settings-dock-sidebar-for-liquid-glass")
                }
            },
            theme::Translucency::LiquidGlass => {
                tr("settings-zervo-s-frosted-material-is-used-here")
            },
            _ => translucency.note(),
        };
        note(ui, palette, material_note);
        if chrome.settings.appearance.frost_is_capped() {
            note(
                ui,
                palette,
                tr("settings-this-arrangement-s-fill-is-opaque-so"),
            );
        }

        ui.add_space(10.0);
        if tuning(
            ui,
            palette,
            tr("settings-blur"),
            &mut chrome.settings.appearance.blur,
            0.0..=30.0,
            |value| format!("{value:.1}px"),
        )
        .settled
        {
            actions.push(UiAction::SettingsChanged);
        }
        note(
            ui,
            palette,
            tr("settings-controls-the-blur-of-zervo-s-frosted"),
        );

        ui.add_space(10.0);
        if tuning(
            ui,
            palette,
            tr("settings-fill"),
            &mut chrome.settings.appearance.fill,
            0.2..=1.0,
            |value| format!("{value:.2}"),
        )
        .settled
        {
            actions.push(UiAction::SettingsChanged);
        }
        note(ui, palette, tr("settings-a-card-s-own-alpha-a-menu"));

        ui.add_space(10.0);
        if tuning(
            ui,
            palette,
            tr("settings-sheen"),
            &mut chrome.settings.appearance.sheen,
            0.0..=60.0,
            |value| format!("{value:.0}/255"),
        )
        .settled
        {
            actions.push(UiAction::SettingsChanged);
        }
        note(ui, palette, tr("settings-white-laid-over-the-fill-out-of"));

        ui.add_space(12.0);
        subhead(ui, palette, tr("settings-edge"));
        if choose(
            ui,
            palette,
            &mut chrome.settings.appearance.edge,
            &theme::Edge::ALL,
            zervo_core::theme::Edge::label,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        note(ui, palette, chrome.settings.appearance.edge.note());

        ui.add_space(10.0);
        if tuning(
            ui,
            palette,
            tr("settings-corner-scale"),
            &mut chrome.settings.appearance.corners,
            0.0..=2.0,
            |value| {
                if value <= 0.02 {
                    "square".to_owned()
                } else {
                    format!("×{value:.2}")
                }
            },
        )
        .settled
        {
            actions.push(UiAction::SettingsChanged);
        }
        note(
            ui,
            palette,
            tr("settings-multiplies-the-whole-ladder-at-once-2"),
        );

        ui.add_space(10.0);
        if tuning(
            ui,
            palette,
            tr("settings-glow-on-the-focused-surface"),
            &mut chrome.settings.appearance.glow,
            0.0..=1.0,
            |value| format!("{value:.2}"),
        )
        .settled
        {
            actions.push(UiAction::SettingsChanged);
        }
        note(ui, palette, tr("settings-the-accent-as-a-lamp-inside-the"));
    });

    settings_section(ui, palette, tr("settings-motion"), |ui| {
        if tuning(
            ui,
            palette,
            tr("settings-settle-time"),
            &mut chrome.settings.appearance.motion,
            0.0..=0.4,
            |value| format!("{value:.2}s"),
        )
        .settled
        {
            actions.push(UiAction::SettingsChanged);
        }
        let motion = chrome.settings.appearance.motion;
        note(
            ui,
            palette,
            if motion <= 0.001 {
                tr("settings-off-nothing-animates-what-reduce-motion-should")
            } else if motion < 0.1 {
                tr("settings-under-a-tenth-of-a-second-reads")
            } else if motion > 0.3 {
                tr("settings-slow-enough-to-notice-you-are-waiting")
            } else {
                tr("settings-what-shipped-is-0-14-hovers-selections")
            },
        );

        ui.add_space(10.0);
        if widgets::toggle(
            ui,
            &mut chrome.settings.appearance.sweep,
            tr("settings-specular-sweep-when-a-surface-resizes"),
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        note(
            ui,
            palette,
            tr("settings-one-pass-of-light-across-any-surface"),
        );

        ui.add_space(8.0);
        if widgets::toggle(
            ui,
            &mut chrome.settings.appearance.pill_progress,
            tr("settings-load-in-the-address-pill-not-a"),
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        note(ui, palette, tr("settings-gives-the-pill-back-the-width-it"));
    });

    settings_section(ui, palette, tr("settings-chrome"), |ui| {
        subhead(
            ui,
            palette,
            tr("settings-what-a-hidden-sidebar-leaves-at-the"),
        );
        if choose(
            ui,
            palette,
            &mut chrome.settings.appearance.spine,
            &theme::Spine::ALL,
            zervo_core::theme::Spine::label,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        note(ui, palette, chrome.settings.appearance.spine.note());

        if cfg!(target_os = "macos") {
            ui.add_space(12.0);
            if widgets::toggle(
                ui,
                &mut chrome.settings.appearance.align_nav,
                tr("settings-centre-the-nav-row-on-the-window"),
                palette,
            ) {
                actions.push(UiAction::SettingsChanged);
            }
            note(ui, palette, tr("settings-native-window-control-alignment"));
        }
    });

    settings_window_section(ui, chrome, palette, actions);

    settings_section(ui, palette, tr("settings-this-arrangement"), |ui| {
        ui.horizontal_wrapped(|ui| {
            if ghost_button(ui, palette, Icon::Reset, tr("settings-reset-to-zervo")).clicked() {
                chrome.settings.appearance = theme::Appearance::classic();
                chose_preset = true;
                actions.push(UiAction::SettingsChanged);
            }
            if ghost_button(ui, palette, Icon::Copy, tr("settings-copy-as-a-material")).clicked() {
                ui.ctx()
                    .copy_text(chrome.settings.appearance.material().as_rust());
            }
            if ghost_button(
                ui,
                palette,
                Icon::FileArrowDown,
                tr("settings-copy-as-json"),
            )
            .clicked()
            {
                ui.ctx().copy_text(chrome.settings.appearance.as_json());
            }
            save_as_preset(ui, chrome, palette, actions);
        });
        note(ui, palette, tr("settings-a-theme-used-to-be-a-rust"));
    });

    if !chose_preset && chrome.settings.appearance != opened_as {
        chrome.settings.appearance.customised();
    }
}

/// The row of accent swatches: the reader's own first, then the presets.
///
/// Lifted out of the section that used to hold it so the Accent section can
/// also carry the workspace switch and the strength slider without becoming a
/// hundred lines of closure.
fn accent_swatches(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    // A swatch, drawn the same way whether it is a preset or the reader's
    // own: one row of circles, one of which happens to open a picker.
    let swatch = |ui: &mut Ui, color: egui::Color32, selected: bool, custom: bool| {
        let (rect, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
        let centre = rect.center();
        let t = ui.ctx().animate_bool(response.id, selected);
        ui.painter().circle_filled(centre, 10.0 + 2.0 * t, color);
        if custom {
            // A ring, so the one that opens a picker does not read as
            // simply another colour to choose.
            ui.painter()
                .circle_stroke(centre, 10.0 + 2.0 * t, Stroke::new(1.5_f32, palette.bg));
            icons::draw_icon(
                ui.painter(),
                Rect::from_center_size(centre, vec2(11.0, 11.0)),
                Icon::Pencil,
                if color_is_light(color) {
                    egui::Color32::from_black_alpha(150)
                } else {
                    egui::Color32::from_white_alpha(200)
                },
            );
        }
        if t > 0.0 {
            ui.painter()
                .circle_stroke(centre, 14.0, Stroke::new(1.5 + 0.5 * t, palette.text));
        }
        response
    };

    ui.horizontal(|ui| {
        // ── The reader's own, first.
        let custom_selected = matches!(chrome.settings.accent, AccentColor::Custom(..));
        let mut rgb = chrome.settings.accent.rgb(palette.dark);
        let response = swatch(
            ui,
            egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
            custom_selected,
            true,
        );
        let picker = Id::new("zervo_accent_picker");
        if response
            .clone()
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(tr("settings-mix-your-own"))
            .clicked()
        {
            // Opening it also selects it, seeded from whatever is in force
            // — so the picker opens on the colour being replaced rather
            // than on an arbitrary one.
            chrome.settings.accent = AccentColor::Custom(rgb[0], rgb[1], rgb[2]);
            actions.push(UiAction::SettingsChanged);
            let open = ui.ctx().data(|data| data.get_temp::<bool>(picker)) == Some(true);
            ui.ctx().data_mut(|data| data.insert_temp(picker, !open));
        }
        if ui.ctx().data(|data| data.get_temp::<bool>(picker)) == Some(true) {
            let mut colour = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            egui::Area::new(picker.with("area"))
                .order(egui::Order::Foreground)
                .fixed_pos(response.rect.left_bottom() + vec2(0.0, 6.0))
                .constrain(true)
                .show(ui.ctx(), |ui| {
                    Frame::popup(ui.style()).show(ui, |ui| {
                        if egui::color_picker::color_picker_color32(
                            ui,
                            &mut colour,
                            egui::color_picker::Alpha::Opaque,
                        ) {
                            rgb = [colour.r(), colour.g(), colour.b()];
                            chrome.settings.accent = AccentColor::Custom(rgb[0], rgb[1], rgb[2]);
                            actions.push(UiAction::SettingsChanged);
                        }
                        if ui.button(tr("common-done")).clicked() {
                            ui.ctx().data_mut(|data| data.insert_temp(picker, false));
                        }
                    });
                });
        }

        // ── The presets.
        for accent in AccentColor::PRESETS {
            let selected = chrome.settings.accent == accent;
            if swatch(ui, accent.color(palette.dark), selected, false)
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(accent.label())
                .clicked()
                && !selected
            {
                chrome.settings.accent = accent;
                actions.push(UiAction::SettingsChanged);
            }
        }
    });
}

/// The Dock icon and the content card's own frame.
///
/// Not part of the material — these decide what the window wears rather than
/// what a surface is made of — but they belong on the same page, and turn 7's
/// rule holds here too: nothing was deleted.
fn settings_window_section(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    settings_section(ui, palette, tr("settings-app-icon"), |ui| {
        let icons = AppIcon::ALL;
        let labels: Vec<&str> = icons.iter().map(crate::settings::AppIcon::label).collect();
        let current = icons
            .iter()
            .position(|icon| *icon == chrome.settings.app_icon)
            .unwrap_or(0);
        if let Some(index) = widgets::segmented(ui, current, &labels, palette) {
            chrome.settings.app_icon = icons[index];
            actions.push(UiAction::SettingsChanged);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(tr("settings-transparent-lets-the-desktop-show-through-the"))
                .size(11.5)
                .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, tr("common-window"), |ui| {
        ui.label(
            RichText::new(tr("settings-top-glow-strip"))
                .size(13.0)
                .color(palette.text),
        );
        // `.settled` rather than `.changed`: the chrome reads the value live, so
        // the picture follows the drag either way, and this decides only when
        // the settings file is rewritten. See `widgets::SliderOut`.
        if widgets::slider(ui, &mut chrome.settings.top_glow, 0.0..=1.0, palette).settled {
            actions.push(UiAction::SettingsChanged);
        }
        ui.label(
            RichText::new(if chrome.settings.top_glow <= 0.0 {
                tr("settings-off-flat-chrome-no-light-across-the").to_owned()
            } else {
                format!(
                    "{:.0}% — accent-tinted light across the top of the window.",
                    chrome.settings.top_glow * 100.0
                )
            })
            .size(11.5)
            .color(palette.text_muted),
        );
        ui.add_space(10.0);

        if widgets::toggle(
            ui,
            &mut chrome.settings.content_border,
            tr("settings-outline-around-content"),
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.label(
            RichText::new(tr("settings-accent-tinted-edge-framing-the-web-page"))
                .size(11.5)
                .color(palette.text_muted),
        );
        ui.add_space(10.0);

        if widgets::toggle(
            ui,
            &mut chrome.settings.content_shadow,
            tr("settings-shadow-around-content"),
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.label(
            RichText::new(tr("settings-depth-under-the-card-on-a-window"))
                .size(11.5)
                .color(palette.text_muted),
        );
        if chrome.settings.content_shadow {
            ui.add_space(6.0);
            if widgets::slider(
                ui,
                &mut chrome.settings.content_shadow_amount,
                0.2..=2.0,
                palette,
            )
            .settled
            {
                actions.push(UiAction::SettingsChanged);
            }
            ui.label(
                RichText::new(spread_note(chrome.settings.content_shadow_amount))
                    .size(11.5)
                    .color(palette.text_muted),
            );
        }
        ui.add_space(10.0);

        if widgets::toggle(
            ui,
            &mut chrome.settings.content_halo,
            tr("settings-halo-around-content"),
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.label(
            RichText::new(tr("settings-a-glow-spreading-out-from-the-card"))
                .size(11.5)
                .color(palette.text_muted),
        );
        if chrome.settings.content_halo {
            ui.add_space(8.0);
            let labels: Vec<&str> = crate::settings::HaloTint::ALL
                .iter()
                .map(|t| t.label())
                .collect();
            let current = crate::settings::HaloTint::ALL
                .iter()
                .position(|t| *t == chrome.settings.content_halo_tint)
                .unwrap_or(0);
            if let Some(picked) = widgets::segmented(ui, current, &labels, palette) {
                chrome.settings.content_halo_tint = crate::settings::HaloTint::ALL[picked];
                actions.push(UiAction::SettingsChanged);
            }
            ui.add_space(6.0);
            if widgets::slider(
                ui,
                &mut chrome.settings.content_halo_amount,
                0.2..=2.0,
                palette,
            )
            .settled
            {
                actions.push(UiAction::SettingsChanged);
            }
            ui.label(
                RichText::new(spread_note(chrome.settings.content_halo_amount))
                    .size(11.5)
                    .color(palette.text_muted),
            );
        }
    });
}

pub(crate) fn settings_general(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    settings_section(ui, palette, tr("settings-language"), |ui| {
        language_picker(ui, chrome.settings, actions);
        note(ui, palette, tr("language-fallback"));
    });

    settings_section(ui, palette, tr("settings-startup"), |ui| {
        ui.label(
            RichText::new(tr("settings-homepage"))
                .size(12.0)
                .color(palette.text_muted),
        );
        let response = ui.add(
            TextEdit::singleline(&mut chrome.settings.homepage)
                .font(FontId::proportional(13.0))
                .desired_width(f32::INFINITY),
        );
        if response.lost_focus() {
            actions.push(UiAction::SettingsChanged);
        }
    });

    settings_section(ui, palette, tr("common-search"), |ui| {
        ui.label(
            RichText::new(tr("settings-search-engine"))
                .size(12.0)
                .color(palette.text_muted),
        );
        egui::ComboBox::from_id_salt("search_engine")
            .selected_text(chrome.settings.search_engine.label())
            .width(200.0)
            .show_ui(ui, |ui| {
                for engine in SearchEngine::ALL {
                    if ui
                        .selectable_value(
                            &mut chrome.settings.search_engine,
                            engine,
                            engine.label(),
                        )
                        .changed()
                    {
                        actions.push(UiAction::SettingsChanged);
                    }
                }
            });
    });

    settings_section(ui, palette, tr("common-downloads"), |ui| {
        if widgets::toggle(
            ui,
            &mut chrome.settings.downloads_auto,
            tr("settings-save-files-without-asking-where"),
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(crate::l10n::message(
                "settings-download-location",
                [(
                    "path",
                    crate::downloads::downloads_dir()
                        .display()
                        .to_string()
                        .into(),
                )],
            ))
            .size(11.5)
            .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, tr("settings-compatibility"), |ui| {
        if widgets::toggle(
            ui,
            &mut chrome.settings.user_agent_compat,
            tr("settings-present-as-plain-firefox"),
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(tr("settings-servo-s-own-user-agent-already-claims"))
                .size(11.5)
                .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, tr("settings-new-tabs"), |ui| {
        ui.label(
            RichText::new(tr("settings-open-with"))
                .size(12.0)
                .color(palette.text_muted),
        );
        let pages = [NewTabPage::ZervoPage, NewTabPage::Homepage];
        let current = pages
            .iter()
            .position(|page| *page == chrome.settings.new_tab_page)
            .unwrap_or(0);
        if let Some(index) = widgets::segmented(
            ui,
            current,
            &[tr("settings-zervo-page"), tr("settings-homepage")],
            palette,
        ) {
            chrome.settings.new_tab_page = pages[index];
            actions.push(UiAction::SettingsChanged);
        }
    });
}

/// One swipe direction and what it does.
pub(crate) fn gesture_row(
    ui: &mut Ui,
    label: &str,
    hint: &str,
    slot: &mut crate::gestures::GestureAction,
    palette: &Palette,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(13.0).color(palette.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::ComboBox::from_id_salt(("gesture", label))
                .selected_text(slot.label())
                .show_ui(ui, |ui| {
                    for option in crate::gestures::GestureAction::ALL {
                        if ui
                            .selectable_label(*slot == option, option.label())
                            .clicked()
                        {
                            *slot = option;
                            changed = true;
                        }
                    }
                });
        });
    });
    if !hint.is_empty() {
        ui.label(RichText::new(hint).size(11.0).color(palette.text_muted));
    }
    ui.add_space(6.0);
    changed
}

pub(crate) fn settings_layout(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    super::layout_editor::draw(ui, chrome.settings, palette, actions);
    settings_section(ui, palette, tr("settings-toolbar"), |ui| {
        for (value, label) in [
            (
                &mut chrome.settings.show_forward_button,
                tr("settings-show-forward-button"),
            ),
            (
                &mut chrome.settings.show_reload_button,
                tr("settings-show-reload-button"),
            ),
        ] {
            if widgets::toggle(ui, value, label, palette) {
                actions.push(UiAction::SettingsChanged);
            }
        }
    });

    settings_section(ui, palette, tr("common-sidebar"), |ui| {
        for (value, label) in [
            (
                &mut chrome.settings.show_tab_counts,
                tr("settings-show-workspace-tab-counts"),
            ),
            (
                &mut chrome.settings.always_show_tab_close,
                tr("settings-always-show-tab-close-buttons"),
            ),
            (
                &mut chrome.settings.compact_sidebar,
                tr("settings-compact-rows"),
            ),
        ] {
            if widgets::toggle(ui, value, label, palette) {
                actions.push(UiAction::SettingsChanged);
            }
        }
    });
    settings_section(ui, palette, tr("settings-new-tab-page"), |ui| {
        if choose(
            ui,
            palette,
            &mut chrome.settings.newtab_home,
            &crate::settings::NewTabHome::ALL,
            crate::settings::NewTabHome::label,
        ) {
            actions.push(UiAction::PersistSettings);
        }
        ui.add_space(4.0);
        note(ui, palette, chrome.settings.newtab_home.note());
        ui.add_space(10.0);
        ui.label(
            RichText::new(tr("settings-both-pages-are-the-same-page-nothing"))
                .size(11.5)
                .color(palette.text_muted),
        );
        ui.add_space(10.0);
        ui.label(
            RichText::new(tr("settings-custom-greeting"))
                .size(12.0)
                .color(palette.text_muted),
        );
        let response = ui.add(
            TextEdit::singleline(&mut chrome.settings.newtab_message)
                .font(FontId::proportional(13.0))
                .hint_text(tr("settings-leave-empty-for-the-time-of-day"))
                .desired_width(f32::INFINITY),
        );
        if response.lost_focus() {
            actions.push(UiAction::PersistSettings);
        }
    });

    settings_section(ui, palette, tr("common-world-clocks"), |ui| {
        let mut remove = None;
        for (index, zone) in chrome.settings.newtab_world_clocks.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&zone.label).size(13.0).color(palette.text));
                ui.label(
                    RichText::new(&zone.name)
                        .size(11.5)
                        .color(palette.text_muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::icon_button(ui, Icon::Close, 13.0, palette, true)
                        .on_hover_text(tr("settings-take-this-city-off"))
                        .clicked()
                    {
                        remove = Some(index);
                    }
                });
            });
        }
        if let Some(index) = remove {
            chrome.settings.newtab_world_clocks.remove(index);
            actions.push(UiAction::PersistSettings);
        }
        if chrome.settings.newtab_world_clocks.is_empty() {
            ui.label(
                RichText::new(tr("settings-no-cities-the-card-says-so-rather"))
                    .size(11.5)
                    .color(palette.text_muted),
            );
        }
        ui.add_space(6.0);
        egui::ComboBox::from_id_salt("zervo_world_clock_add")
            .selected_text(tr("settings-add-a-city"))
            .width(220.0)
            .show_ui(ui, |ui| {
                for (label, name) in crate::newtab::Zone::CATALOGUE {
                    let already = chrome
                        .settings
                        .newtab_world_clocks
                        .iter()
                        .any(|zone| zone.name == name);
                    if already {
                        continue;
                    }
                    if ui.selectable_label(false, label).clicked() {
                        chrome
                            .settings
                            .newtab_world_clocks
                            .push(crate::newtab::Zone {
                                label: label.to_owned(),
                                name: name.to_owned(),
                            });
                        actions.push(UiAction::PersistSettings);
                    }
                }
            });
        ui.add_space(4.0);
        ui.label(
            RichText::new(tr("settings-each-clock-reads-the-zone-from-the"))
                .size(11.5)
                .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, tr("common-wallpaper"), |ui| {
        use crate::wallpaper::{Cadence, Source, Subject};
        let photo = chrome.settings.new_tab_background == NewTabBackground::Photo;
        let mut wants_photo = photo;
        if widgets::toggle(
            ui,
            &mut wants_photo,
            tr("settings-show-a-photograph"),
            palette,
        ) {
            chrome.settings.new_tab_background = if wants_photo {
                NewTabBackground::Photo
            } else {
                NewTabBackground::Aurora
            };
            actions.push(UiAction::PersistSettings);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(tr(
                "settings-pictures-come-from-wikimedia-commons-and-openverse",
            ))
            .size(11.5)
            .color(palette.text_muted),
        );

        ui.add_space(10.0);
        ui.label(
            RichText::new(tr("settings-source"))
                .size(12.0)
                .color(palette.text_muted),
        );
        let mut sources: Vec<(String, Source)> = vec![(
            tr("settings-commons-picture-of-the-day").to_owned(),
            Source::Commons,
        )];
        sources.extend(Subject::ALL.iter().map(|subject| {
            (
                format!("Openverse — {}", subject.label().to_lowercase()),
                Source::Openverse(*subject),
            )
        }));
        let selected = sources
            .iter()
            .find(|(_, source)| *source == chrome.settings.wallpaper_source)
            .map(|(label, _)| label.clone())
            .unwrap_or_else(|| chrome.settings.wallpaper_source.label());
        egui::ComboBox::from_id_salt("zervo_wallpaper_source")
            .selected_text(selected)
            .width(260.0)
            .show_ui(ui, |ui| {
                for (label, source) in sources {
                    if ui
                        .selectable_label(chrome.settings.wallpaper_source == source, label)
                        .clicked()
                    {
                        chrome.settings.wallpaper_source = source;
                        actions.push(UiAction::ShuffleWallpaper);
                    }
                }
            });

        ui.add_space(10.0);
        ui.label(
            RichText::new(tr("settings-change-it"))
                .size(12.0)
                .color(palette.text_muted),
        );
        let cadences = Cadence::ALL;
        let labels: Vec<&str> = cadences.iter().map(|cadence| cadence.label()).collect();
        let current = cadences
            .iter()
            .position(|cadence| *cadence == chrome.settings.wallpaper_cadence)
            .unwrap_or(0);
        if let Some(index) = widgets::segmented(ui, current, &labels, palette) {
            chrome.settings.wallpaper_cadence = cadences[index];
            actions.push(UiAction::PersistSettings);
        }

        ui.add_space(10.0);
        ui.label(
            RichText::new(tr("settings-veil"))
                .size(12.0)
                .color(palette.text_muted),
        );
        if widgets::slider(ui, &mut chrome.settings.wallpaper_dim, 0.15..=0.9, palette).settled {
            actions.push(UiAction::PersistSettings);
        }
        ui.label(
            RichText::new(format!(
                "{:.0}% — how far the picture is dimmed so the cards stay readable on it.",
                chrome.settings.wallpaper_dim * 100.0
            ))
            .size(11.5)
            .color(palette.text_muted),
        );

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button(tr("common-another-picture")).clicked() {
                actions.push(UiAction::ShuffleWallpaper);
            }
            if ui.button(tr("common-choose-file")).clicked() {
                actions.push(UiAction::PickWallpaper);
            }
        });
        ui.add_space(4.0);
        let credit = chrome.wallpaper.credit;
        let note = if let Some(why) = chrome.wallpaper.error {
            format!("The last attempt failed: {why}")
        } else if chrome.wallpaper.loading {
            tr("settings-fetching-one").to_owned()
        } else if chrome.wallpaper.texture.is_some() {
            format!("Showing {} — from {}.", credit.line(), credit.source)
        } else {
            tr("settings-nothing-fetched-yet").to_owned()
        };
        ui.label(RichText::new(note).size(11.5).color(palette.text_muted));
    });

    settings_section(ui, palette, tr("settings-trackpad"), |ui| {
        if widgets::toggle(
            ui,
            &mut chrome.settings.gestures.enabled,
            tr("settings-two-finger-swipes"),
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.label(
            RichText::new(tr("settings-a-quick-straight-flick-a-slow-or"))
                .size(11.5)
                .color(palette.text_muted),
        );
        if chrome.settings.gestures.enabled {
            ui.add_space(10.0);
            let mut changed = false;
            changed |= gesture_row(
                ui,
                tr("settings-swipe-right"),
                tr("settings-anywhere-in-the-window"),
                &mut chrome.settings.gestures.right,
                palette,
            );
            changed |= gesture_row(
                ui,
                tr("settings-swipe-left"),
                "",
                &mut chrome.settings.gestures.left,
                palette,
            );
            changed |= gesture_row(
                ui,
                tr("settings-swipe-down"),
                "",
                &mut chrome.settings.gestures.down,
                palette,
            );
            changed |= gesture_row(
                ui,
                tr("settings-swipe-up"),
                "",
                &mut chrome.settings.gestures.up,
                palette,
            );
            if changed {
                actions.push(UiAction::SettingsChanged);
            }
        }
    });

    settings_section(ui, palette, tr("settings-arrangement"), |ui| {
        ui.label(
            RichText::new(tr("settings-reset-sidebar-widgets-and-new-tab"))
                .size(11.5)
                .color(palette.text_muted),
        );
        ui.add_space(8.0);
        if ui.button(tr("settings-reset-to-defaults")).clicked() {
            actions.push(UiAction::ResetLayout);
        }
    });
}

/// Saved logins.
///
/// Deliberately plain about what this can and cannot do: Servo gives the
/// embedder no way to see a submitted form or write into a page, so there is no
/// autofill to offer and pretending otherwise would be worse than saying so.
pub(crate) fn settings_passwords(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;

    settings_section(ui, &palette, tr("settings-saved-logins"), |ui| {
        ui.label(
            RichText::new(tr("settings-passwords-are-kept-in-your-system-keychain"))
                .size(12.0)
                .color(palette.text_muted),
        );
        ui.add_space(10.0);
        // What the engine cannot do, said where the feature is rather than in
        // the README where nobody using the browser will read it. Somebody who
        // signs in, is never offered a save, and is not told why concludes the
        // password manager is broken; somebody who was told concludes the
        // engine is young. Same fact, opposite conclusion.
        crate::ui::limitation(
            ui,
            &palette,
            Icon::Info,
            tr("settings-the-engine-gives-zervo-no-hook-for"),
        );
        ui.add_space(12.0);

        if chrome.vault.is_empty() {
            ui.label(
                RichText::new(tr("settings-nothing-saved-yet"))
                    .size(13.0)
                    .color(palette.text_muted),
            );
        }
        let logins: Vec<(String, String)> = chrome
            .vault
            .logins()
            .iter()
            .map(|login| (login.site.clone(), login.username.clone()))
            .collect();
        for (site, username) in logins {
            let (row, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
            if response.hovered() {
                ui.painter()
                    .rect_filled(row, palette.corner(Tier::Control), palette.surface_hover);
            }
            ui.painter().text(
                pos2(row.min.x + 6.0, row.center().y),
                Align2::LEFT_CENTER,
                &site,
                FontId::proportional(13.0),
                palette.text,
            );
            ui.painter().text(
                pos2(row.min.x + 6.0 + 170.0, row.center().y),
                Align2::LEFT_CENTER,
                &username,
                FontId::proportional(12.5),
                palette.text_muted,
            );
            let remove =
                Rect::from_center_size(pos2(row.max.x - 14.0, row.center().y), vec2(18.0, 18.0));
            icons::draw_icon(
                ui.painter(),
                remove.shrink(4.0),
                Icon::Trash,
                palette.text_muted,
            );
            if ui
                .interact(
                    remove,
                    Id::new("zervo_pw_remove").with((&site, &username)),
                    Sense::click(),
                )
                .on_hover_text(tr("settings-forget-this-login"))
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                actions.push(UiAction::RemovePassword(site.clone(), username.clone()));
            }
        }
    });

    ui.add_space(14.0);
    settings_section(ui, &palette, tr("settings-add-a-login"), |ui| {
        let field = |ui: &mut Ui, label: &str, value: &mut String, secret: bool| {
            ui.horizontal(|ui| {
                ui.allocate_ui(vec2(90.0, 26.0), |ui| {
                    ui.label(RichText::new(label).size(12.5).color(palette.text_muted));
                });
                ui.add(
                    TextEdit::singleline(value)
                        .password(secret)
                        .font(FontId::proportional(13.0))
                        .desired_width(ui.available_width().min(260.0)),
                );
            });
        };
        let draft = &mut chrome.browser.password_draft;
        field(ui, tr("settings-site"), &mut draft.0, false);
        field(ui, tr("settings-username"), &mut draft.1, false);
        field(ui, tr("settings-password"), &mut draft.2, true);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(tr("settings-save-login")).clicked() {
                actions.push(UiAction::SavePassword);
            }
            ui.add_space(6.0);
            if ui.button(tr("settings-import")).clicked() {
                actions.push(UiAction::ImportPasswords);
            }
            if ui
                .button(tr("settings-export"))
                .on_hover_text(tr("settings-writes-every-password-to-a-plain-unencrypted"))
                .clicked()
            {
                actions.push(UiAction::ExportPasswords);
            }
        });
        if !chrome.browser.password_notice.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(chrome.browser.password_notice.clone())
                    .size(12.0)
                    .color(palette.text_muted),
            );
        }
    });
}

pub(crate) fn settings_about(ui: &mut Ui, palette: &Palette) {
    settings_section(ui, palette, "Zervo", |ui| {
        ui.label(
            RichText::new(crate::l10n::message(
                "settings-version",
                [("version", env!("CARGO_PKG_VERSION").into())],
            ))
            .size(13.0)
            .color(palette.text),
        );
        ui.label(
            RichText::new(tr(
                "settings-rendering-engine-servo-compositing-through-webrender",
            ))
            .size(12.0)
            .color(palette.text_muted),
        );
        // Not a guess about the platform: the driver's own GL_RENDERER and
        // GL_VERSION, read at startup from the context the page is drawn in.
        let adapter = crate::gpu::adapter();
        if !adapter.renderer.is_empty() {
            ui.label(
                RichText::new(format!("Graphics: {}", adapter.renderer))
                    .size(12.0)
                    .color(palette.text_muted),
            );
            let driver = if adapter.vendor.is_empty() {
                adapter.version.clone()
            } else {
                format!("{} — {}", adapter.version, adapter.vendor)
            };
            ui.label(
                RichText::new(format!("Driver: {driver}"))
                    .size(12.0)
                    .color(palette.text_muted),
            );
        }
        ui.label(
            RichText::new(format!("WebGPU: {}", crate::gpu::webgpu_backend_name()))
                .size(12.0)
                .color(palette.text_muted),
        );
        ui.label(
            RichText::new(tr("settings-chrome-egui-on-winit-painted-against-servo"))
                .size(12.0)
                .color(palette.text_muted),
        );
        ui.label(
            RichText::new(format!(
                "Material: {} — corner radii, fills, edges and shadows all come from it.",
                palette.material.name
            ))
            .size(12.0)
            .color(palette.text_muted),
        );
    });
}

pub(crate) fn settings_section(
    ui: &mut Ui,
    palette: &Palette,
    title: &str,
    add_contents: impl FnOnce(&mut Ui),
) {
    ui.label(
        RichText::new(title.to_uppercase())
            .size(10.5)
            .strong()
            .color(palette.text_muted),
    );
    ui.add_space(4.0);
    Frame::NONE.inner_margin(Margin::same(14)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        // Reserve a slot, lay out the contents, then backfill the glass
        // card sized to what was actually laid out (max_rect here spans
        // the whole remaining scroll viewport, not the section).
        let placeholder = ui.painter().add(Shape::Noop);
        // Each section its own id scope. A `Frame` does not open one, so every
        // section on the page derived its widget ids from the same parent —
        // and the segmented control keys its segments on (index, label), so
        // two sections offering a choice that happens to share a word at the
        // same position were one widget with two positions. egui paints "First
        // use of widget ID …" across the page when that happens, which is a
        // debug banner the reader should never be shown.
        ui.push_id(title, add_contents);
        let card_rect = ui.min_rect().expand(14.0);
        ui.painter().set(
            placeholder,
            Shape::Vec(glass::shapes(
                card_rect,
                palette,
                Glass::tier(Tier::Card).strength(0.8),
            )),
        );
    });
    ui.add_space(14.0);
}
