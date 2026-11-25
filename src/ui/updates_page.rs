use egui::text::LayoutJob;
use egui::{Align, Color32, FontId, Layout, Rect, RichText, ScrollArea, TextFormat, Ui, UiBuilder};

use crate::l10n::{message, text as tr};
use crate::theme::{self, Palette};
use crate::ui::{ChromeContext, UiAction, application_icon};
use crate::updates::Status;

use super::frame::{builtin_card, builtin_page_rect, centre_builtin_column, paint_builtin_page};

const GITHUB_RELEASES: &str = "https://github.com/Goddv/Zervo/releases";
const HISTORY: &str = include_str!("../../CHANGELOG.md");

pub(super) fn draw(
    root: &mut Ui,
    chrome: &ChromeContext,
    content: Rect,
    actions: &mut Vec<UiAction>,
) {
    let palette = &chrome.palette;
    paint_builtin_page(root, content, palette);
    let bounds = centre_builtin_column(builtin_page_rect(content, chrome.window_controls_room()));
    let mut pane = root.new_child(
        UiBuilder::new()
            .max_rect(bounds)
            .layout(Layout::top_down(Align::Min)),
    );
    pane.shrink_clip_rect(bounds);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(&mut pane, |ui| {
            ui.label(
                RichText::new(tr("updates-title"))
                    .size(26.0)
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(14.0);
            let updates = chrome.updates;
            let tint = if palette.dark { 0.12 } else { 0.07 };
            let base = if palette.dark {
                palette.bg
            } else {
                Color32::WHITE
            };
            builtin_card(palette)
                .fill(theme::mix(base, palette.accent, tint))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if let Some(icon) = application_icon(ui, palette.dark) {
                            ui.image((icon.id(), egui::vec2(48.0, 48.0)));
                        }
                        ui.add_space(5.0);
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(tr("updates-installed"))
                                    .size(12.0)
                                    .color(palette.text_muted),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "{} {}",
                                    zervo_core::APP_NAME,
                                    crate::updates::installed_version()
                                ))
                                .size(20.0)
                                .strong()
                                .color(palette.text),
                            );
                        });
                    });
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(updates.label())
                            .size(13.0)
                            .color(palette.text),
                    );
                    if matches!(
                        updates.status,
                        Status::Unavailable
                            | Status::Manual
                            | Status::NoCompatible
                            | Status::Failed
                    ) && ui.link(tr("updates-github")).clicked()
                    {
                        actions.push(UiAction::OpenAddress(GITHUB_RELEASES.into()));
                    }
                    if matches!(
                        updates.status,
                        Status::Available | Status::Ready | Status::Manual
                    ) {
                        if updates.reminder_scheduled {
                            ui.label(
                                RichText::new(tr("updates-reminder-scheduled"))
                                    .size(12.0)
                                    .color(palette.text_muted),
                            );
                        } else if ui.link(tr("updates-remind-tomorrow")).clicked() {
                            actions.push(UiAction::ConfigureUpdates(
                                crate::updates::Preference::RemindTomorrow,
                            ));
                        }
                    }
                });
            if !updates.version.is_empty() {
                ui.add_space(16.0);
                ui.label(
                    RichText::new(message(
                        "updates-next-version",
                        [("version", updates.version.as_str().into())],
                    ))
                    .size(18.0)
                    .strong()
                    .color(palette.text),
                );
                if updates.bytes > 0 {
                    ui.label(
                        RichText::new(message(
                            "updates-download-size",
                            [("size", crate::downloads::format_bytes(updates.bytes).into())],
                        ))
                        .size(12.0)
                        .color(palette.text_muted),
                    );
                }
                if !updates.notes.is_empty() {
                    release_body(ui, palette, &updates.notes);
                }
            }
            ui.add_space(22.0);
            ui.label(
                RichText::new(tr("updates-history"))
                    .size(18.0)
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(8.0);
            // Keep older releases collapsed and render their text only on demand.
            for (index, release) in HISTORY.split("\n## ").skip(1).enumerate() {
                let Some((heading, body)) = release.split_once('\n') else {
                    continue;
                };
                egui::CollapsingHeader::new(RichText::new(heading).color(palette.text).strong())
                    .id_salt(index)
                    .default_open(index == 0)
                    .show(ui, |ui| release_body(ui, palette, body));
            }
            ui.add_space(16.0);
        });
}

fn release_body(ui: &mut Ui, palette: &Palette, body: &str) {
    // Render text only. This also keeps signed notes out of the browser engine.
    let mut paragraph = String::new();
    let flush = |ui: &mut Ui, paragraph: &mut String| {
        if !paragraph.is_empty() {
            let mut text = LayoutJob::default();
            for (index, part) in paragraph.split('`').enumerate() {
                let font_id = if index % 2 == 1 {
                    FontId::monospace(12.5)
                } else {
                    FontId::proportional(13.0)
                };
                text.append(part, 0.0, TextFormat::simple(font_id, palette.text));
            }
            ui.label(text);
            paragraph.clear();
        }
    };
    for line in body.lines().map(str::trim) {
        if let Some(heading) = line.strip_prefix("### ") {
            flush(ui, &mut paragraph);
            ui.add_space(5.0);
            ui.label(
                RichText::new(heading)
                    .size(13.0)
                    .strong()
                    .color(palette.text),
            );
        } else if line.is_empty() || line.starts_with("- ") || line.starts_with("* ") {
            flush(ui, &mut paragraph);
            if let Some(bullet) = line.get(2..) {
                paragraph.push_str("• ");
                paragraph.push_str(bullet);
            }
        } else {
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(line);
        }
    }
    flush(ui, &mut paragraph);
}
