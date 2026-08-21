//! `zervo://settings`.
//!
//! Every section of it — appearance, general, layout, passwords, about — and
//! the navigation column down the left. This is where the great majority of
//! `chrome.settings` is read and written; almost nowhere else touches it.

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

pub(crate) fn draw_settings_page(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    content_rect: Rect,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let painter = root.ctx().layer_painter(egui::LayerId::background());
    let card_radius = CornerRadius::same(theme::CONTENT_RADIUS as u8);
    painter.rect_filled(content_rect, card_radius, palette.surface);

    // ── Left navigation column, Chrome-style: categories on the left, the
    // selected category's panel on the right.
    let nav_width = 188.0_f32.min(content_rect.width() * 0.34);
    let nav_rect = Rect::from_min_size(content_rect.min, vec2(nav_width, content_rect.height()));
    painter.rect_filled(
        nav_rect,
        CornerRadius {
            nw: theme::CONTENT_RADIUS as u8,
            sw: theme::CONTENT_RADIUS as u8,
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
            RichText::new("Settings")
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
                ui.id().with(("settings_nav", section.label())),
                response.hovered() && !selected,
                0.12,
            ));
            let select_t = glass::ease_out(ui.ctx().animate_bool_with_time(
                ui.id().with(("settings_nav_sel", section.label())),
                selected,
                0.18,
            ));
            if hover_t > 0.0 {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(8),
                    palette.surface_hover.gamma_multiply(hover_t),
                );
            }
            if select_t > 0.0 {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(8),
                    palette.active.gamma_multiply(select_t),
                );
            }
            ui.painter().text(
                pos2(rect.min.x + 12.0, rect.center().y),
                Align2::LEFT_CENTER,
                section.label(),
                FontId::proportional(13.5),
                if selected {
                    palette.text
                } else {
                    palette.text_muted
                },
            );
            if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                chrome.browser.settings_section = section;
            }
        }
    }

    // ── Content pane: scrolls independently, scrollbar at the card's edge.
    let pane_rect = Rect::from_min_max(pos2(nav_rect.max.x, content_rect.min.y), content_rect.max)
        .shrink2(vec2(6.0, 10.0));
    let mut pane_ui = root.new_child(
        egui::UiBuilder::new()
            .max_rect(pane_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let ui = &mut pane_ui;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let column = (ui.available_width() - 40.0).clamp(240.0, 520.0);
            let margin = ((ui.available_width() - column) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_width(column);
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new(chrome.browser.settings_section.label())
                            .size(21.0)
                            .strong()
                            .color(palette.text),
                    );
                    ui.add_space(12.0);
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

pub(crate) fn settings_appearance(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    settings_section(ui, palette, "Theme", |ui| {
        let labels: Vec<&str> = ThemeMode::ALL
            .iter()
            .map(crate::theme::ThemeMode::label)
            .collect();
        let current = ThemeMode::ALL
            .iter()
            .position(|mode| *mode == chrome.settings.theme)
            .unwrap_or(0);
        if let Some(index) = widgets::segmented(ui, current, &labels, palette) {
            chrome.settings.theme = ThemeMode::ALL[index];
            actions.push(UiAction::SettingsChanged);
        }
        ui.add_space(2.0);
        ui.label(
            RichText::new("Auto follows the system appearance and day/night cycle.")
                .size(11.5)
                .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, "Accent colour", |ui| {
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
                ui.painter().circle_stroke(
                    centre,
                    10.0 + 2.0 * t,
                    Stroke::new(1.5_f32, palette.bg),
                );
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
                .on_hover_text("Mix your own")
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
                                chrome.settings.accent =
                                    AccentColor::Custom(rgb[0], rgb[1], rgb[2]);
                                actions.push(UiAction::SettingsChanged);
                            }
                            if ui.button("Done").clicked() {
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
    });

    settings_section(ui, palette, "App icon", |ui| {
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
            RichText::new(
                "Transparent lets the desktop show through the icon's backdrop. \
                 Changes apply to the Dock immediately.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, "Window", |ui| {
        ui.label(
            RichText::new("Top glow strip")
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
                "Off — flat chrome, no light across the top.".to_owned()
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
            "Outline around content",
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.label(
            RichText::new("Accent-tinted edge framing the web page.")
                .size(11.5)
                .color(palette.text_muted),
        );
        ui.add_space(10.0);

        if widgets::toggle(
            ui,
            &mut chrome.settings.content_shadow,
            "Shadow around content",
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.label(
            RichText::new("Depth under the card. On a window this size it mostly traces the edge.")
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
            "Halo around content",
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.label(
            RichText::new("A glow spreading out from the card's edge.")
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

    settings_section(ui, palette, "Transparency", |ui| {
        ui.label(
            RichText::new("Material")
                .size(12.0)
                .color(palette.text_muted),
        );
        let levels = crate::theme::Translucency::ALL;
        let labels: Vec<&str> = levels.iter().map(|level| level.label()).collect();
        let current = levels
            .iter()
            .position(|level| *level == chrome.settings.translucency)
            .unwrap_or(0);
        if let Some(index) = widgets::segmented(ui, current, &labels, palette) {
            chrome.settings.translucency = levels[index];
            actions.push(UiAction::SettingsChanged);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(chrome.settings.translucency.note())
                .size(11.5)
                .color(palette.text_muted),
        );
        ui.add_space(2.0);
        ui.label(
            RichText::new(
                "Everything the material draws — the window's own chrome, the cards, \
                 the menus, the shelf, the new tab page.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );

        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Frosted asks the system for the backdrop behind the window and tints \
                 it. Solid asks for none and paints over it.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );
    });
}

pub(crate) fn settings_general(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    settings_section(ui, palette, "Startup", |ui| {
        ui.label(
            RichText::new("Homepage")
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

    settings_section(ui, palette, "Search", |ui| {
        ui.label(
            RichText::new("Search engine")
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

    settings_section(ui, palette, "Downloads", |ui| {
        if widgets::toggle(
            ui,
            &mut chrome.settings.downloads_auto,
            "Save files without asking where",
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "Saved to {}",
                crate::downloads::downloads_dir().display()
            ))
            .size(11.5)
            .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, "Compatibility", |ui| {
        if widgets::toggle(
            ui,
            &mut chrome.settings.user_agent_compat,
            "Present as plain Firefox",
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Servo's own user agent already claims Firefox, but keeps a Servo token \
                 and omits Gecko — and enough sites match on exactly those to turn you \
                 away. Takes effect on the next launch.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, "New tabs", |ui| {
        ui.label(
            RichText::new("Open with")
                .size(12.0)
                .color(palette.text_muted),
        );
        let pages = [NewTabPage::ZervoPage, NewTabPage::Homepage];
        let current = pages
            .iter()
            .position(|page| *page == chrome.settings.new_tab_page)
            .unwrap_or(0);
        if let Some(index) = widgets::segmented(ui, current, &["Zervo page", "Homepage"], palette) {
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
    settings_section(ui, palette, "Toolbar", |ui| {
        for (value, label) in [
            (
                &mut chrome.settings.show_forward_button,
                "Show forward button",
            ),
            (
                &mut chrome.settings.show_reload_button,
                "Show reload button",
            ),
        ] {
            if widgets::toggle(ui, value, label, palette) {
                actions.push(UiAction::SettingsChanged);
            }
        }
    });

    settings_section(ui, palette, "Sidebar", |ui| {
        for (value, label) in [
            (
                &mut chrome.settings.show_essentials,
                "Show pinned essentials grid",
            ),
            (
                &mut chrome.settings.show_tab_counts,
                "Show workspace tab counts",
            ),
            (
                &mut chrome.settings.always_show_tab_close,
                "Always show tab close buttons",
            ),
            (&mut chrome.settings.compact_sidebar, "Compact rows"),
            (
                &mut chrome.settings.sidebar_autohide,
                "Reveal a hidden sidebar on hover",
            ),
        ] {
            if widgets::toggle(ui, value, label, palette) {
                actions.push(UiAction::SettingsChanged);
            }
        }
    });
    settings_section(ui, palette, "New tab page", |ui| {
        ui.label(
            RichText::new(
                "The cards are arranged on the page itself — press Customise there to \
                 move, resize and remove them.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );
        ui.add_space(10.0);
        ui.label(
            RichText::new("Custom greeting")
                .size(12.0)
                .color(palette.text_muted),
        );
        let response = ui.add(
            TextEdit::singleline(&mut chrome.settings.newtab_message)
                .font(FontId::proportional(13.0))
                .hint_text("Leave empty for the time of day")
                .desired_width(f32::INFINITY),
        );
        if response.lost_focus() {
            actions.push(UiAction::PersistSettings);
        }
    });

    settings_section(ui, palette, "World clocks", |ui| {
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
                        .on_hover_text("Take this city off")
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
                RichText::new("No cities — the card says so rather than showing nothing.")
                    .size(11.5)
                    .color(palette.text_muted),
            );
        }
        ui.add_space(6.0);
        egui::ComboBox::from_id_salt("zervo_world_clock_add")
            .selected_text("Add a city…")
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
            RichText::new(
                "Each clock reads the zone from the compiled-in IANA table, so daylight \
                 saving is right without anyone maintaining it.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, "Wallpaper", |ui| {
        use crate::wallpaper::{Cadence, Source, Subject};
        let photo = chrome.settings.new_tab_background == NewTabBackground::Photo;
        let mut wants_photo = photo;
        if widgets::toggle(ui, &mut wants_photo, "Show a photograph", palette) {
            chrome.settings.new_tab_background = if wants_photo {
                NewTabBackground::Photo
            } else {
                NewTabBackground::Aurora
            };
            actions.push(UiAction::PersistSettings);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Pictures come from Wikimedia Commons and Openverse, which publish under \
                 licences that allow this. Neither needs an account. The credit line under \
                 the page is part of the licence, so it is always drawn.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );

        ui.add_space(10.0);
        ui.label(RichText::new("Source").size(12.0).color(palette.text_muted));
        let mut sources: Vec<(String, Source)> =
            vec![("Commons picture of the day".to_owned(), Source::Commons)];
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
            RichText::new("Change it")
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
        ui.label(RichText::new("Veil").size(12.0).color(palette.text_muted));
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
            if ui.button("Another picture").clicked() {
                actions.push(UiAction::ShuffleWallpaper);
            }
            if ui.button("Choose a file…").clicked() {
                actions.push(UiAction::PickWallpaper);
            }
        });
        ui.add_space(4.0);
        let credit = chrome.wallpaper.credit;
        let note = if let Some(why) = chrome.wallpaper.error {
            format!("The last attempt failed: {why}")
        } else if chrome.wallpaper.loading {
            "Fetching one…".to_owned()
        } else if chrome.wallpaper.texture.is_some() {
            format!("Showing {} — from {}.", credit.line(), credit.source)
        } else {
            "Nothing fetched yet.".to_owned()
        };
        ui.label(RichText::new(note).size(11.5).color(palette.text_muted));
    });

    settings_section(ui, palette, "Trackpad", |ui| {
        if widgets::toggle(
            ui,
            &mut chrome.settings.gestures.enabled,
            "Two-finger swipes",
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.label(
            RichText::new(
                "A quick, straight flick. A slow or wandering one is a scroll and is \
                 left alone.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );
        if chrome.settings.gestures.enabled {
            ui.add_space(10.0);
            let mut changed = false;
            changed |= gesture_row(
                ui,
                "Swipe right",
                "Anywhere in the window.",
                &mut chrome.settings.gestures.right,
                palette,
            );
            changed |= gesture_row(
                ui,
                "Swipe left",
                "",
                &mut chrome.settings.gestures.left,
                palette,
            );
            changed |= gesture_row(
                ui,
                "Swipe down",
                "Over the bar above the page — everywhere else this scrolls.",
                &mut chrome.settings.gestures.down,
                palette,
            );
            changed |= gesture_row(
                ui,
                "Swipe up",
                "",
                &mut chrome.settings.gestures.up,
                palette,
            );
            if changed {
                actions.push(UiAction::SettingsChanged);
            }
        }
    });

    settings_section(ui, palette, "Arrangement", |ui| {
        ui.label(
            RichText::new(
                "The navigation bar, its widgets, and the widths of the sidebar and \
                 address bar are all arranged by dragging them rather than set here.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );
        ui.add_space(8.0);
        if ui
            .button("Reset to defaults")
            .on_hover_text("Puts every bar button, widget and width back")
            .clicked()
        {
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

    settings_section(ui, &palette, "Saved logins", |ui| {
        ui.label(
            RichText::new(
                "Passwords are kept in your system keychain, never in Zervo's own files. \
                 Zervo cannot fill them into web forms — the engine provides no way to \
                 do that — but it does use them when a site asks for HTTP authentication.",
            )
            .size(12.0)
            .color(palette.text_muted),
        );
        ui.add_space(10.0);

        if chrome.vault.is_empty() {
            ui.label(
                RichText::new("Nothing saved yet.")
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
                    .rect_filled(row, CornerRadius::same(7), palette.surface_hover);
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
                .on_hover_text("Forget this login")
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                actions.push(UiAction::RemovePassword(site.clone(), username.clone()));
            }
        }
    });

    ui.add_space(14.0);
    settings_section(ui, &palette, "Add a login", |ui| {
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
        field(ui, "Site", &mut draft.0, false);
        field(ui, "Username", &mut draft.1, false);
        field(ui, "Password", &mut draft.2, true);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Save login").clicked() {
                actions.push(UiAction::SavePassword);
            }
            ui.add_space(6.0);
            if ui.button("Import…").clicked() {
                actions.push(UiAction::ImportPasswords);
            }
            if ui
                .button("Export…")
                .on_hover_text("Writes every password to a plain, unencrypted file")
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
            RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                .size(13.0)
                .color(palette.text),
        );
        ui.label(
            RichText::new("Rendering engine: Servo, compositing through WebRender.")
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
            RichText::new("Chrome: egui on winit, painted against Servo's GL context.")
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
        add_contents(ui);
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
