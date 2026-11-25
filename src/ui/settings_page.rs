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
use crate::settings::{MAX_ACCENT_STRENGTH, NewTabBackground, NewTabPage, SearchEngine};
use crate::theme::{self, AccentColor, Palette, ThemeMode, Tier};
use crate::widgets;

use super::*;

const SPECIMEN_HEIGHT: f32 = 190.0;

pub(crate) fn draw_settings_page(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    content_rect: Rect,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let ctx = root.ctx().clone();
    paint_builtin_page(root, content_rect, &palette);
    let page = builtin_page_rect(content_rect, chrome.window_controls_room());
    let wide = page.width() >= 680.0;
    let mut area = root.new_child(egui::UiBuilder::new().max_rect(page));
    area.set_clip_rect(page.intersect(root.clip_rect()));

    use crate::state::SettingsSection;
    let selection = &mut chrome.browser.settings_section;
    let query = &mut chrome.browser.settings_query;
    let mut pane = page;
    if wide {
        let nav = Rect::from_min_size(page.min, vec2(174.0, page.height()));
        let mut nav_area = area.new_child(egui::UiBuilder::new().max_rect(nav));
        nav_area.label(
            RichText::new(tr("common-settings"))
                .size(21.0)
                .strong()
                .color(palette.text),
        );
        nav_area.add_space(12.0);
        for section in SettingsSection::ALL {
            if settings_nav(
                &mut nav_area,
                section,
                *selection == section && query.is_empty(),
                true,
                &palette,
            ) {
                *selection = section;
                query.clear();
            }
        }
        pane.min.x = nav.right() + 24.0;
    } else {
        area.label(
            RichText::new(tr("common-settings"))
                .size(20.0)
                .strong()
                .color(palette.text),
        );
        area.add_space(5.0);
        area.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(3.0, 4.0);
            for section in SettingsSection::ALL {
                if settings_nav(
                    ui,
                    section,
                    *selection == section && query.is_empty(),
                    false,
                    &palette,
                ) {
                    *selection = section;
                    query.clear();
                }
            }
        });
        pane.min.y = area.min_rect().bottom() + 10.0;
    }

    let column = centre_builtin_column(pane);
    let mut body = area.new_child(
        egui::UiBuilder::new()
            .max_rect(column)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    body.set_clip_rect(body.clip_rect().intersect(column));
    builtin_search(
        &mut body,
        &palette,
        "settings_search",
        query,
        tr("settings-find"),
    );
    let section = *selection;
    let words: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    let searching = !words.is_empty();
    body.add_space(15.0);
    body.label(
        RichText::new(if searching {
            tr("settings-search-results")
        } else {
            section.label()
        })
        .size(BUILTIN_HEADING)
        .strong()
        .color(palette.text),
    );
    body.add_space(12.0);

    egui::ScrollArea::vertical()
        .id_salt((
            "settings_cards",
            if searching { None } else { Some(section) },
        ))
        .auto_shrink([false, false])
        .show(&mut body, |ui| {
            if searching {
                let id = settings_search_id();
                let mut matches = 0_usize;
                let words: std::sync::Arc<[String]> = words.into();
                for section in SettingsSection::ALL {
                    let filter = CardSearch {
                        words: words.clone(),
                        category: section.label().to_lowercase().into(),
                        matches,
                    };
                    ctx.data_mut(|data| data.insert_temp(id, filter));
                    draw_settings_cards(ui, section, chrome, &palette, actions);
                    if let Some(filter) = ctx.data_mut(|data| data.remove_temp::<CardSearch>(id)) {
                        matches = filter.matches;
                    }
                }
                if matches == 0 {
                    ui.label(RichText::new(tr("settings-no-results")).color(palette.text_muted));
                }
            } else {
                if !cfg!(target_os = "macos") && section == SettingsSection::Appearance {
                    let (rect, _) = ui.allocate_exact_size(
                        vec2(ui.available_width(), SPECIMEN_HEIGHT),
                        Sense::hover(),
                    );
                    appearance_specimen(ui, &palette, rect);
                    ui.add_space(14.0);
                }
                draw_settings_cards(ui, section, chrome, &palette, actions);
            }
        });
}

#[derive(Clone, Default)]
struct CardSearch {
    words: std::sync::Arc<[String]>,
    category: std::sync::Arc<str>,
    matches: usize,
}

fn settings_search_id() -> Id {
    Id::unique("settings_card_search")
}

fn draw_settings_cards(
    ui: &mut Ui,
    section: crate::state::SettingsSection,
    chrome: &mut ChromeContext,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    use crate::state::SettingsSection;
    ui.push_id(section, |ui| match section {
        SettingsSection::Appearance => settings_appearance(ui, chrome, palette, actions),
        SettingsSection::General => settings_general(ui, chrome, palette, actions),
        SettingsSection::Layout => settings_layout(ui, chrome, palette, actions),
        SettingsSection::Passwords => settings_passwords(ui, chrome, actions),
        SettingsSection::About => settings_about(ui, palette, chrome.updates, actions),
    });
}

fn settings_nav(
    ui: &mut Ui,
    section: crate::state::SettingsSection,
    selected: bool,
    wide: bool,
    palette: &Palette,
) -> bool {
    use crate::state::SettingsSection;
    let icon = match section {
        SettingsSection::Appearance => Icon::Palette,
        SettingsSection::General => Icon::Gear,
        SettingsSection::Layout => Icon::Sidebar,
        SettingsSection::Passwords => Icon::Lock,
        SettingsSection::About => Icon::Info,
    };
    let font = FontId::proportional(13.5);
    let ink = if selected {
        palette.text
    } else {
        palette.text_muted
    };
    let label = ui
        .painter()
        .layout_no_wrap(section.label().to_owned(), font.clone(), ink);
    let width = if wide {
        ui.available_width()
    } else {
        label.size().x + 45.0
    };
    let (row, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    if selected || response.hovered() {
        let fill = if selected {
            palette.active
        } else {
            palette.surface_hover
        };
        ui.painter()
            .rect_filled(row, palette.corner(Tier::Row), fill);
    }
    if selected {
        ui.painter().rect_filled(
            Rect::from_center_size(pos2(row.left() + 3.0, row.center().y), vec2(2.5, 14.0)),
            1,
            palette.accent,
        );
    }
    icons::draw_icon(
        ui.painter(),
        Rect::from_center_size(pos2(row.left() + 16.0, row.center().y), vec2(15.0, 15.0)),
        icon,
        ink,
    );
    crate::overlay_glass::row_label(
        ui,
        Rect::from_min_max(
            pos2(row.left() + 31.0, row.top()),
            pos2(row.right() - 8.0, row.bottom()),
        ),
        section.label(),
        font,
        ink,
    );
    if response.has_focus() {
        icons::paint_focus(ui, palette, row, Tier::Row);
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Tab, true, selected, section.label())
    });
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

pub(super) fn builtin_search(
    ui: &mut Ui,
    palette: &Palette,
    id: &str,
    query: &mut String,
    label: &str,
) {
    let (field, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    let surface = crate::overlay_glass::Surface::begin(ui);
    let role = crate::overlay_glass::Role::Field;
    let fill = builtin_card(palette).fill;
    ui.painter().rect_filled(
        field,
        role.radius(),
        fill.gamma_multiply(if surface.is_native() { 0.60 } else { 1.0 }),
    );
    icons::draw_icon(
        ui.painter(),
        Rect::from_center_size(
            pos2(field.left() + 18.0, field.center().y),
            vec2(15.0, 15.0),
        ),
        Icon::Search,
        palette.text_muted,
    );
    let area = field
        .with_min_x(field.left() + 34.0)
        .with_max_x(field.right() - 34.0);
    let mut entry = ui.new_child(egui::UiBuilder::new().id_salt(id).max_rect(area));
    entry.style_mut().visuals.weak_text_color = Some(palette.text_muted);
    let search = entry.put(
        area,
        TextEdit::singleline(query)
            .font(FontId::proportional(13.5))
            .text_color(palette.text)
            .hint_text(label)
            .frame(Frame::NONE)
            .margin(Margin::ZERO)
            .min_size(area.size())
            .align(Align2::LEFT_CENTER)
            .desired_width(area.width()),
    );
    search.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, label));
    let focused = search.has_focus();
    if focused && entry.input(|input| input.key_pressed(Key::Escape)) {
        query.clear();
    }
    if focused || !surface.is_native() {
        let stroke = if focused {
            ui.visuals().selection.stroke
        } else {
            Stroke::new(1.0, palette.border)
        };
        ui.painter()
            .rect_stroke(field, role.radius(), stroke, StrokeKind::Inside);
    }
    if !query.is_empty() {
        let rect = Rect::from_center_size(
            pos2(field.right() - 17.0, field.center().y),
            vec2(24.0, 24.0),
        );
        let clear = ui.interact(rect, entry.scope_id().with("clear"), Sense::click());
        icons::paint_icon_button(ui, &clear, Icon::Close, 12.0, palette);
        if crate::overlay_glass::tooltip(clear, palette, tr("settings-clear-search")).clicked() {
            query.clear();
            search.request_focus();
        }
    }
    surface.finish(ui, field, role);
}

/// "Save as a preset…", and the field it opens.
///
/// The five presets are constants and stay constants — they are the ones the
/// study argues for. This is the other half of the answer to "a theme is a
/// Rust constant": not only is every value settable, an arrangement somebody
/// tuned can be put somewhere and come back.
#[cfg(not(target_os = "macos"))]
fn save_as_preset(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    let open_id = Id::unique("zervo_preset_naming");
    let draft_id = Id::unique("zervo_preset_name");
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
    let field = crate::overlay_glass::text_field(
        ui,
        TextEdit::singleline(&mut name)
            .font(FontId::proportional(12.5))
            .hint_text(RichText::new(tr("settings-name-it")).color(palette.text_muted))
            .desired_width(150.0),
    )
    .response;
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
#[cfg(not(target_os = "macos"))]
fn preset_button(ui: &mut Ui, palette: &Palette, label: &str, selected: bool) -> egui::Response {
    widgets::label_button(ui, palette, label, 30.0, 26.0, selected)
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
#[cfg(not(target_os = "macos"))]
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
    #[cfg(not(target_os = "macos"))]
    let (opened_as, mut chose_preset) = (chrome.settings.appearance, false);

    #[cfg(not(target_os = "macos"))]
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

    #[cfg(target_os = "macos")]
    settings_section(ui, palette, tr("common-liquid-glass"), |ui| {
        note(ui, palette, tr("settings-liquid-glass-native"));
        ui.add_space(6.0);
        if ui.button(tr("settings-open-macos-appearance")).clicked() {
            actions.push(UiAction::OpenSystemAppearance);
        }
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
            0.0..=MAX_ACCENT_STRENGTH,
            |value| format!("{value:.3}"),
        )
        .settled
        {
            actions.push(UiAction::SettingsChanged);
        }
        note(ui, palette, tr("settings-accent-strength-help"));
    });

    if chrome.settings.layout.is_sidebar() {
        use theme::Seam;

        let frame_options: &[Seam] = if cfg!(target_os = "macos") {
            &[Seam::Card, Seam::EdgeToEdge]
        } else {
            &Seam::ALL
        };
        settings_section(ui, palette, tr("settings-page-frame"), |ui| {
            if choose(
                ui,
                palette,
                &mut chrome.settings.appearance.seam,
                frame_options,
                Seam::label,
            ) {
                actions.push(UiAction::SettingsChanged);
            }
            let seam = chrome.settings.appearance.seam;
            let detail = match seam {
                Seam::Card => tr("settings-page-frame-card"),
                Seam::EdgeToEdge => tr("settings-page-frame-edge-to-edge"),
                _ => seam.note(),
            };
            note(ui, palette, detail);

            if !seam.closes_gap() {
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
            }
        });
    }

    #[cfg(not(target_os = "macos"))]
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
        let material_note = match chrome.settings.appearance.translucency {
            theme::Translucency::LiquidGlass => {
                tr("settings-zervo-s-frosted-material-is-used-here")
            },
            translucency => translucency.note(),
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
            tr("settings-sidebar-specular-sweep"),
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        note(ui, palette, tr("settings-sidebar-specular-sweep-help"));

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
        if cfg!(target_os = "macos") {
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

    #[cfg(not(target_os = "macos"))]
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

    #[cfg(not(target_os = "macos"))]
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

    ui.horizontal_wrapped(|ui| {
        let custom_selected = matches!(chrome.settings.accent, AccentColor::Custom(..));
        let rgb = chrome.settings.accent.rgb(palette.dark);
        let response = swatch(
            ui,
            egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
            custom_selected,
            true,
        )
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(tr("settings-mix-your-own"));
        let popup = egui::Popup::from_toggle_button_response(&response)
            .gap(6.0)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        crate::overlay_glass::popup(popup, ui.ctx(), |ui| {
            ui.spacing_mut().slider_width = 232.0;
            let mut colour = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            if egui::color_picker::color_picker_color32(
                ui,
                &mut colour,
                egui::color_picker::Alpha::Opaque,
            ) {
                chrome.settings.accent = AccentColor::Custom(colour.r(), colour.g(), colour.b());
                actions.push(UiAction::SettingsChanged);
            }
            if ui.button(tr("common-done")).clicked() {
                ui.close();
            }
        });

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
    settings_section(ui, palette, tr("common-window"), |ui| {
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
        let response = crate::overlay_glass::text_field(
            ui,
            TextEdit::singleline(&mut chrome.settings.homepage)
                .font(FontId::proportional(13.0))
                .desired_width(f32::INFINITY),
        )
        .response;
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
            .show_glass(ui, |ui| {
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
                .show_glass(ui, |ui| {
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
                &mut chrome.settings.show_tab_favicons,
                tr("layout-show-site-icons"),
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
    settings_section(
        ui,
        palette,
        tr("settings-what-a-hidden-sidebar-leaves-at-the"),
        |ui| {
            if choose(
                ui,
                palette,
                &mut chrome.settings.appearance.spine,
                &theme::Spine::ALL,
                theme::Spine::label,
            ) {
                actions.push(UiAction::SettingsChanged);
            }
            note(ui, palette, chrome.settings.appearance.spine.note());
        },
    );
    settings_section(ui, palette, tr("settings-new-tab-page"), |ui| {
        ui.label(tr("newtab-backdrop"));
        if choose(
            ui,
            palette,
            &mut chrome.settings.new_tab_background,
            &NewTabBackground::ALL,
            NewTabBackground::label,
        ) {
            actions.push(UiAction::PersistSettings);
            ui.ctx().request_repaint();
        }
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
            ui.label(
                RichText::new(tr("settings-sidebar-space-swipes"))
                    .size(11.5)
                    .color(palette.text_muted),
            );
            ui.add_space(10.0);
            let mut changed = false;
            changed |= gesture_row(
                ui,
                tr("settings-swipe-right"),
                tr("settings-outside-sidebar"),
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
                    Id::unique("zervo_pw_remove").with((&site, &username)),
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
                crate::overlay_glass::text_field(
                    ui,
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

fn settings_about(
    ui: &mut Ui,
    palette: &Palette,
    updates: &crate::updates::Snapshot,
    actions: &mut Vec<UiAction>,
) {
    settings_section(ui, palette, tr("settings-build-and-engine"), |ui| {
        let icon = application_icon(ui, palette.dark);
        ui.horizontal(|ui| {
            if let Some(icon) = icon.as_ref() {
                ui.image((icon.id(), vec2(50.0, 50.0)));
            }
            ui.vertical(|ui| {
                ui.label(RichText::new(zervo_core::APP_NAME).size(20.0).strong());
                ui.label(
                    RichText::new(crate::l10n::message(
                        "settings-version",
                        [("version", crate::updates::installed_version().into())],
                    ))
                    .color(palette.text_muted),
                );
            });
        });
        ui.add_space(8.0);
        ui.separator();
        about_detail(
            ui,
            palette,
            tr("settings-rendering-engine"),
            concat!("Servo ", env!("ZERVO_SERVO_VERSION"), " · WebRender"),
        );
        ui.separator();
        let toolkit = concat!(
            "egui ",
            env!("ZERVO_EGUI_VERSION"),
            " · winit ",
            env!("ZERVO_WINIT_VERSION")
        );
        let toolkit = if cfg!(target_os = "macos") {
            format!("{toolkit} · AppKit")
        } else {
            toolkit.to_owned()
        };
        about_detail(ui, palette, tr("settings-ui-toolkit"), &toolkit);
    });

    settings_section(ui, palette, tr("updates-title"), |ui| {
        note(ui, palette, updates.label());
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(updates.can_check, egui::Button::new(tr("updates-check")))
                .clicked()
            {
                actions.push(UiAction::CheckForUpdates);
            }
            if ui.button(tr("updates-release-notes")).clicked() {
                actions.push(UiAction::OpenUpdates);
            }
        });
        if updates.enabled() {
            update_preferences(ui, palette, updates, actions);
        }
    });

    settings_section(ui, palette, tr("settings-graphics"), |ui| {
        let adapter = crate::gpu::adapter();
        if !adapter.renderer.is_empty() {
            about_detail(
                ui,
                palette,
                tr("settings-graphics-adapter"),
                &adapter.renderer,
            );
            ui.separator();
            let driver = if adapter.vendor.is_empty() {
                adapter.version.clone()
            } else {
                format!("{} · {}", adapter.version, adapter.vendor)
            };
            about_detail(ui, palette, tr("settings-graphics-driver"), &driver);
            ui.separator();
        }
        about_detail(ui, palette, "WebGPU", crate::gpu::webgpu_backend_name());
    });
}

fn update_preferences(
    ui: &mut Ui,
    palette: &Palette,
    updates: &crate::updates::Snapshot,
    actions: &mut Vec<UiAction>,
) {
    use crate::updates::{CHECK_INTERVALS, Preference, check_interval_label};

    ui.add_space(10.0);
    let mut checks = updates.automatic_checks;
    if widgets::toggle(ui, &mut checks, tr("updates-automatic-checks"), palette) {
        actions.push(UiAction::ConfigureUpdates(Preference::AutomaticChecks(
            checks,
        )));
    }
    ui.add_enabled_ui(checks, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("updates-check-frequency"));
            let mut interval = updates.check_interval;
            egui::ComboBox::from_id_salt("update_check_interval")
                .selected_text(tr(check_interval_label(interval)))
                .show_glass(ui, |ui| {
                    for (seconds, label) in CHECK_INTERVALS {
                        if ui
                            .selectable_value(&mut interval, seconds, tr(label))
                            .changed()
                        {
                            actions.push(UiAction::ConfigureUpdates(Preference::CheckInterval(
                                seconds,
                            )));
                        }
                    }
                });
        });
    });
    ui.add_space(8.0);
    ui.add_enabled_ui(checks, |ui| {
        let mut automatic = updates.automatic_install;
        if widgets::toggle(ui, &mut automatic, tr("updates-automatic"), palette) {
            actions.push(UiAction::ConfigureUpdates(Preference::AutomaticInstall(
                automatic,
            )));
        }
    });
    note(ui, palette, tr("updates-automatic-hint"));
    ui.add_space(8.0);
    let mut notifications = updates.system_notifications;
    if widgets::toggle(
        ui,
        &mut notifications,
        tr("updates-system-notifications"),
        palette,
    ) {
        actions.push(UiAction::ConfigureUpdates(Preference::SystemNotifications(
            notifications,
        )));
    }
    if updates.notifications_blocked {
        note(ui, palette, tr("updates-notifications-blocked"));
    }
}

fn about_detail(ui: &mut Ui, palette: &Palette, label: &str, value: &str) {
    ui.add_space(3.0);
    ui.label(RichText::new(label).strong().size(12.5).color(palette.text));
    ui.label(RichText::new(value).size(12.5).color(palette.text_muted));
    ui.add_space(3.0);
}

pub(crate) fn settings_section(
    ui: &mut Ui,
    palette: &Palette,
    title: &str,
    add_contents: impl FnOnce(&mut Ui),
) {
    let visible = ui.ctx().data_mut(|data| {
        let Some(mut filter) = data.get_temp::<CardSearch>(settings_search_id()) else {
            return true;
        };
        let title = title.to_lowercase();
        let visible = filter
            .words
            .iter()
            .all(|word| title.contains(word) || filter.category.contains(word));
        if visible {
            filter.matches += 1;
            data.insert_temp(settings_search_id(), filter);
        }
        visible
    });
    if !visible {
        return;
    }

    Frame::NONE.inner_margin(Margin::same(18)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).size(16.0).strong().color(palette.text));
        ui.add_space(7.0);
        ui.push_id(title, add_contents);
    });
    ui.add_space(13.0);
}
