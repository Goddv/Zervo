//! Content for the dashboard's optional widgets and writing card.

use super::{Ink, fit, weekday};
use crate::glass;
use crate::icons::{self, Icon};
use crate::l10n::text as tr;
use crate::state::TabId;
use crate::theme::{self, Palette, Tier};
use crate::ui::{ChromeContext, UiAction};
use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Frame, Id, Key, Rect, RichText, Sense,
    Stroke, StrokeKind, TextEdit, Ui, pos2, vec2,
};

pub(super) fn draw_world_clocks(area: &mut Ui, chrome: &ChromeContext, body: Rect) {
    let palette = chrome.palette;
    let ink = Ink::new(&palette, false);
    let zones = &chrome.settings.newtab_world_clocks;
    if zones.is_empty() {
        empty(
            area,
            &palette,
            body,
            tr("newtab-add-a-city-in-settings-layout"),
        );
        return;
    }
    let now = jiff::Timestamp::now();
    // As many across as fit at a readable width, the rest wrap onto a row of
    // their own.
    let per_row = ((body.width() / 96.0).floor() as usize).clamp(1, zones.len());
    let rows = zones.len().div_ceil(per_row);
    let cell = vec2(body.width() / per_row as f32, body.height() / rows as f32);
    for (index, zone) in zones.iter().enumerate() {
        let slot = Rect::from_min_size(
            pos2(
                body.min.x + (index % per_row) as f32 * cell.x,
                body.min.y + (index / per_row) as f32 * cell.y,
            ),
            cell,
        );
        let Ok(tz) = jiff::tz::TimeZone::get(&zone.name) else {
            area.painter().text(
                slot.center(),
                Align2::CENTER_CENTER,
                "—",
                FontId::proportional(13.0),
                palette.text_muted,
            );
            continue;
        };
        let there = now.to_zoned_datetime(tz);
        let clock = (slot.height() * 0.34).clamp(16.0, 26.0);
        area.painter().text(
            pos2(slot.min.x + 2.0, slot.min.y + 9.0),
            Align2::LEFT_TOP,
            fit(
                area.painter(),
                zone.display_label(),
                &FontId::proportional(11.0),
                cell.x - 8.0,
            ),
            FontId::proportional(11.0),
            palette.text_muted,
        );
        area.painter().text(
            pos2(slot.min.x + 2.0, slot.min.y + 24.0),
            Align2::LEFT_TOP,
            there.strftime("%H:%M").to_string(),
            FontId::proportional(clock),
            ink.text,
        );
        if slot.height() > 58.0 {
            area.painter().text(
                pos2(slot.min.x + 2.0, slot.min.y + 26.0 + clock),
                Align2::LEFT_TOP,
                crate::l10n::message(
                    "clock-zone-day",
                    [
                        ("zone", there.strftime("%Z").to_string().into()),
                        ("weekday", weekday(&there, true).into()),
                    ],
                ),
                FontId::proportional(10.0),
                palette.text_muted,
            );
        }
    }
    area.ctx()
        .request_repaint_after(std::time::Duration::from_secs(20));
}

pub(super) fn draw_quick_links(
    area: &mut Ui,
    chrome: &mut ChromeContext,
    body: Rect,
    id: Id,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    struct Link {
        tab_id: TabId,
        workspace: usize,
        title: String,
    }
    let links: Vec<Link> = chrome
        .browser
        .workspaces
        .iter()
        .enumerate()
        .flat_map(|(workspace, space)| {
            space
                .tabs
                .iter()
                .filter(|tab| tab.pinned)
                .map(move |tab| Link {
                    tab_id: tab.id,
                    workspace,
                    title: crate::ui::display_name(&tab.title, &tab.url).to_owned(),
                })
        })
        .take(24)
        .collect();
    if links.is_empty() {
        empty(area, &palette, body, tr("newtab-pin-a-tab-and-it-turns-up"));
        return;
    }

    let tile = vec2(74.0, 70.0);
    let per_row = ((body.width() / tile.x).floor() as usize).max(1);
    for (index, link) in links.iter().enumerate() {
        let column = index % per_row;
        let row = index / per_row;
        let slot = Rect::from_min_size(
            pos2(
                body.min.x + column as f32 * tile.x,
                body.min.y + row as f32 * tile.y,
            ),
            tile,
        )
        .shrink(3.0);
        if slot.max.y > body.max.y + 6.0 {
            break; // The card is full; the rest are in the sidebar.
        }
        let response = area.interact(slot, id.with(("link", link.tab_id)), Sense::click());
        let hover = glass::ease_out(area.ctx().animate_bool_with_time(
            response.id.with("hover"),
            response.hovered(),
            0.12,
        ));
        if hover > 0.0 {
            area.painter().rect_filled(
                slot,
                CornerRadius::same(palette.radius(Tier::Card)),
                palette.surface_hover.gamma_multiply(hover),
            );
        }
        let icon = pos2(slot.center().x, slot.min.y + 22.0);
        match chrome.favicons.get(&link.tab_id) {
            Some(texture) => {
                area.painter().image(
                    texture.id(),
                    Rect::from_center_size(icon, vec2(22.0, 22.0)),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            },
            None => icons::draw_icon(
                area.painter(),
                Rect::from_center_size(icon, vec2(19.0, 19.0)),
                Icon::Globe,
                palette.text_muted,
            ),
        }
        area.painter().text(
            pos2(slot.center().x, slot.max.y - 12.0),
            Align2::CENTER_CENTER,
            fit(
                area.painter(),
                &link.title,
                &FontId::proportional(10.5),
                slot.width() - 4.0,
            ),
            FontId::proportional(10.5),
            theme::mix(palette.text_muted, palette.text, hover),
        );
        if response
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(&link.title)
            .clicked()
        {
            actions.push(UiAction::SelectWorkspace(link.workspace));
            actions.push(UiAction::SelectTab(link.tab_id));
        }
    }
}

pub(super) fn draw_top_sites(
    area: &mut Ui,
    chrome: &mut ChromeContext,
    body: Rect,
    id: Id,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let rows = ((body.height() / 30.0).floor() as usize).max(1);
    let sites: Vec<(String, String, usize)> = chrome
        .library
        .top_sites(rows)
        .iter()
        .map(|site| (site.host.clone(), site.url.clone(), site.visits))
        .collect();
    if sites.is_empty() {
        empty(
            area,
            &palette,
            body,
            tr("newtab-nowhere-yet-this-fills-as-you-browse"),
        );
        return;
    }
    for (index, (host, url, visits)) in sites.iter().enumerate() {
        let row = Rect::from_min_size(
            pos2(body.min.x, body.min.y + index as f32 * 30.0),
            vec2(body.width(), 28.0),
        );
        if row.max.y > body.max.y + 4.0 {
            break;
        }
        let response = list_row(area, &palette, row, id, index, host, host);
        area.painter().text(
            pos2(row.max.x - 4.0, row.center().y),
            Align2::RIGHT_CENTER,
            visits.to_string(),
            FontId::proportional(11.0),
            palette.text_muted,
        );
        if response.clicked() {
            actions.push(UiAction::Navigate(url.clone()));
        }
    }
}

pub(super) fn draw_downloads(
    area: &mut Ui,
    chrome: &mut ChromeContext,
    body: Rect,
    id: Id,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let rows = ((body.height() / 34.0).floor() as usize).max(1);
    let items: Vec<(u64, String, Option<f32>, bool)> = chrome
        .downloads
        .items
        .iter()
        .rev()
        .take(rows)
        .map(|item| {
            (
                item.id,
                item.filename.clone(),
                item.fraction(),
                item.state == crate::downloads::DownloadState::Running,
            )
        })
        .collect();
    if items.is_empty() {
        empty(area, &palette, body, tr("common-no-downloads"));
        return;
    }
    for (index, (download_id, name, fraction, running)) in items.iter().enumerate() {
        let row = Rect::from_min_size(
            pos2(body.min.x, body.min.y + index as f32 * 34.0),
            vec2(body.width(), 32.0),
        );
        if row.max.y > body.max.y + 4.0 {
            break;
        }
        let response = area.interact(row, id.with(("download", download_id)), Sense::click());
        let hover = glass::ease_out(area.ctx().animate_bool_with_time(
            response.id.with("hover"),
            response.hovered(),
            0.12,
        ));
        if hover > 0.0 {
            area.painter().rect_filled(
                row,
                CornerRadius::same(palette.radius(Tier::Row)),
                palette.surface_hover.gamma_multiply(hover),
            );
        }
        icons::draw_icon(
            area.painter(),
            Rect::from_center_size(pos2(row.min.x + 14.0, row.center().y), vec2(14.0, 14.0)),
            if *running {
                Icon::Download
            } else {
                Icon::Check
            },
            if *running {
                palette.accent
            } else {
                palette.text_muted
            },
        );
        area.painter().text(
            pos2(row.min.x + 28.0, row.min.y + 11.0),
            Align2::LEFT_CENTER,
            fit(
                area.painter(),
                name,
                &FontId::proportional(12.5),
                row.width() - 36.0,
            ),
            FontId::proportional(12.5),
            theme::mix(palette.text_muted, palette.text, hover.max(0.4)),
        );
        if let Some(fraction) = fraction
            && *running
        {
            let track = Rect::from_min_size(
                pos2(row.min.x + 28.0, row.max.y - 8.0),
                vec2(row.width() - 36.0, 3.0),
            );
            area.painter().rect_filled(
                track,
                CornerRadius::same(palette.radius(Tier::Hairline)),
                palette.border,
            );
            area.painter().rect_filled(
                Rect::from_min_size(track.min, vec2(track.width() * fraction, track.height())),
                CornerRadius::same(palette.radius(Tier::Hairline)),
                palette.accent,
            );
        }
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            actions.push(UiAction::OpenDownloads);
        }
    }
}

pub(super) fn draw_now_playing(
    area: &mut Ui,
    chrome: &mut ChromeContext,
    body: Rect,
    id: Id,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let media = chrome.media;
    if media.is_idle() {
        empty(area, &palette, body, tr("newtab-nothing-playing"));
        return;
    }
    let controls = 108.0_f32.min(body.width() * 0.5);
    let text_width = (body.width() - controls - 8.0).max(20.0);
    area.painter().text(
        pos2(body.min.x, body.min.y + 9.0),
        Align2::LEFT_CENTER,
        fit(
            area.painter(),
            &media.title,
            &FontId::proportional(13.0),
            text_width,
        ),
        FontId::proportional(13.0),
        palette.text,
    );
    area.painter().text(
        pos2(body.min.x, body.min.y + 25.0),
        Align2::LEFT_CENTER,
        fit(
            area.painter(),
            &media.artist,
            &FontId::proportional(11.5),
            text_width,
        ),
        FontId::proportional(11.5),
        palette.text_muted,
    );
    if media.duration > 0.0 {
        let track =
            Rect::from_min_size(pos2(body.min.x, body.max.y - 5.0), vec2(body.width(), 3.0));
        area.painter().rect_filled(
            track,
            CornerRadius::same(palette.radius(Tier::Hairline)),
            palette.border,
        );
        let played = (media.position / media.duration).clamp(0.0, 1.0) as f32;
        area.painter().rect_filled(
            Rect::from_min_size(track.min, vec2(track.width() * played, track.height())),
            CornerRadius::same(palette.radius(Tier::Hairline)),
            palette.accent,
        );
    }

    use servo::MediaSessionActionType;
    let buttons = [
        (Icon::SkipBack, MediaSessionActionType::PreviousTrack),
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
        (Icon::SkipForward, MediaSessionActionType::NextTrack),
    ];
    for (index, (icon, action)) in buttons.into_iter().enumerate() {
        let centre = pos2(
            body.max.x - controls + controls * (0.18 + 0.32 * index as f32),
            body.min.y + 17.0,
        );
        let hit = Rect::from_center_size(centre, vec2(28.0, 28.0));
        let response = area.interact(hit, id.with(("transport", index)), Sense::click());
        icons::paint_icon_button(area, &response, icon, 15.0, &palette);
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            actions.push(UiAction::MediaAction(action));
        }
    }
}

pub(super) fn draw_notes(area: &mut Ui, chrome: &mut ChromeContext, body: Rect, id: Id) {
    let palette = chrome.palette;
    let mut inner = area.new_child(egui::UiBuilder::new().max_rect(body).id_salt(id));
    let editor = TextEdit::multiline(chrome.library.note_mut())
        .frame(Frame::NONE)
        .font(FontId::proportional(12.5))
        .text_color(palette.text)
        .hint_text(
            RichText::new(tr("newtab-a-place-to-put-something-down")).color(palette.text_muted),
        )
        .desired_width(body.width())
        .desired_rows(((body.height() / 16.0) as usize).max(1));
    if inner.add(editor).changed() {
        chrome.library.changed();
    }
}

pub(super) fn draw_tasks(area: &mut Ui, chrome: &mut ChromeContext, body: Rect, id: Id) {
    let palette = chrome.palette;
    const ROW: f32 = 26.0;
    let field = 28.0;
    let list = Rect::from_min_max(
        body.min,
        pos2(body.max.x, (body.max.y - field).max(body.min.y)),
    );
    let capacity = ((list.height() / ROW).floor() as usize).max(1);

    let tasks: Vec<(String, bool)> = chrome
        .library
        .tasks
        .iter()
        .take(capacity)
        .map(|task| (task.text.clone(), task.done))
        .collect();
    if tasks.is_empty() {
        area.painter().text(
            pos2(list.min.x, list.min.y + 10.0),
            Align2::LEFT_CENTER,
            tr("newtab-nothing-to-do-enviable"),
            FontId::proportional(11.5),
            palette.text_muted,
        );
    }

    let mut toggled = None;
    let mut removed = None;
    for (index, (text, done)) in tasks.iter().enumerate() {
        let row = Rect::from_min_size(
            pos2(list.min.x, list.min.y + index as f32 * ROW),
            vec2(list.width(), ROW - 2.0),
        );
        let response = area.interact(row, id.with(("task", index)), Sense::click());
        let hover = glass::ease_out(area.ctx().animate_bool_with_time(
            response.id.with("hover"),
            response.hovered(),
            0.12,
        ));
        if hover > 0.0 {
            area.painter().rect_filled(
                row,
                CornerRadius::same(palette.radius(Tier::Control)),
                palette.surface_hover.gamma_multiply(hover),
            );
        }
        let box_rect =
            Rect::from_center_size(pos2(row.min.x + 9.0, row.center().y), vec2(14.0, 14.0));
        if *done {
            area.painter().rect_filled(
                box_rect,
                CornerRadius::same(palette.radius(Tier::Control)),
                palette.accent,
            );
            icons::draw_icon(
                area.painter(),
                box_rect.shrink(2.5),
                Icon::Check,
                Color32::WHITE,
            );
        } else {
            area.painter().rect_stroke(
                box_rect,
                CornerRadius::same(palette.radius(Tier::Control)),
                Stroke::new(1.0_f32, palette.border),
                StrokeKind::Inside,
            );
        }
        area.painter().text(
            pos2(row.min.x + 24.0, row.center().y),
            Align2::LEFT_CENTER,
            fit(
                area.painter(),
                text,
                &FontId::proportional(12.5),
                row.width() - 46.0,
            ),
            FontId::proportional(12.5),
            if *done {
                palette.text_muted
            } else {
                palette.text
            },
        );
        // A done line still says so; a struck-through one says it and stops
        // asking to be read.
        if *done {
            let width = fit(
                area.painter(),
                text,
                &FontId::proportional(12.5),
                row.width() - 46.0,
            );
            let extent = area
                .painter()
                .layout_no_wrap(width, FontId::proportional(12.5), Color32::WHITE)
                .rect
                .width();
            area.painter().line_segment(
                [
                    pos2(row.min.x + 24.0, row.center().y),
                    pos2(row.min.x + 24.0 + extent, row.center().y),
                ],
                Stroke::new(1.0_f32, palette.text_muted.gamma_multiply(0.8)),
            );
        }
        if hover > 0.0 {
            let bin =
                Rect::from_center_size(pos2(row.max.x - 10.0, row.center().y), vec2(16.0, 16.0));
            icons::draw_icon(
                area.painter(),
                bin.shrink(3.0),
                Icon::Close,
                palette.text_muted,
            );
            if area
                .interact(bin, id.with(("task_remove", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                removed = Some(index);
            }
        }
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() && removed.is_none() {
            toggled = Some(index);
        }
    }

    let entry = Rect::from_min_size(
        pos2(body.min.x, body.max.y - field + 4.0),
        vec2(body.width(), field - 6.0),
    );
    let mut inner = area.new_child(
        egui::UiBuilder::new()
            .max_rect(entry)
            .id_salt(id)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let editor = TextEdit::singleline(&mut chrome.browser.newtab_task_draft)
        .frame(Frame::NONE)
        .font(FontId::proportional(12.5))
        .text_color(palette.text)
        .align(egui::Align2::LEFT_CENTER)
        .hint_text(RichText::new(tr("newtab-add-something")).color(palette.text_muted))
        .desired_width(entry.width());
    let response = inner.add(editor);
    area.painter().line_segment(
        [
            pos2(entry.min.x, entry.min.y - 2.0),
            pos2(entry.max.x, entry.min.y - 2.0),
        ],
        Stroke::new(1.0_f32, palette.border),
    );
    if response.lost_focus() && inner.input(|input| input.key_pressed(Key::Enter)) {
        let draft = std::mem::take(&mut chrome.browser.newtab_task_draft);
        chrome.library.add_task(&draft);
        response.request_focus();
    }

    if let Some(index) = removed {
        chrome.library.remove_task(index);
    } else if let Some(index) = toggled {
        chrome.library.toggle_task(index);
    }
}

pub(super) fn draw_workspaces(
    area: &mut Ui,
    chrome: &mut ChromeContext,
    body: Rect,
    id: Id,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let active = chrome.browser.active_workspace;
    let spaces: Vec<(String, usize)> = chrome
        .browser
        .workspaces
        .iter()
        .map(|space| (space.name.clone(), space.tabs.len()))
        .collect();
    for (index, (name, count)) in spaces.iter().enumerate() {
        let row = Rect::from_min_size(
            pos2(body.min.x, body.min.y + index as f32 * 28.0),
            vec2(body.width(), 26.0),
        );
        if row.max.y > body.max.y + 4.0 {
            break;
        }
        let response = area.interact(row, id.with(("workspace", index)), Sense::click());
        let ink = crate::widgets::button_ink(area, &response, row, &palette, index == active);
        area.painter().circle_filled(
            pos2(row.min.x + 10.0, row.center().y),
            4.0,
            theme::workspace_color_from(index, chrome.settings.space_colour),
        );
        area.painter().text(
            pos2(row.min.x + 22.0, row.center().y),
            Align2::LEFT_CENTER,
            fit(
                area.painter(),
                name,
                &FontId::proportional(12.5),
                row.width() - 52.0,
            ),
            FontId::proportional(12.5),
            ink,
        );
        area.painter().text(
            pos2(row.max.x - 8.0, row.center().y),
            Align2::RIGHT_CENTER,
            count.to_string(),
            FontId::proportional(11.0),
            ink,
        );
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            actions.push(UiAction::SelectWorkspace(index));
        }
    }
}

fn list_row(
    area: &mut Ui,
    palette: &Palette,
    row: Rect,
    id: Id,
    index: usize,
    title: &str,
    url: &str,
) -> egui::Response {
    let response = area.interact(row, id.with(("row", index)), Sense::click());
    let ink = crate::widgets::button_ink(area, &response, row, palette, false);
    let badge = Rect::from_center_size(pos2(row.min.x + 14.0, row.center().y), vec2(20.0, 20.0));
    area.painter().rect_filled(
        badge,
        CornerRadius::same(palette.radius(Tier::Control)),
        palette.surface_hover.gamma_multiply(0.9),
    );
    area.painter().text(
        badge.center(),
        Align2::CENTER_CENTER,
        crate::ui::initial(title, url),
        FontId::proportional(11.0),
        palette.text_muted,
    );
    area.painter().text(
        pos2(row.min.x + 30.0, row.center().y),
        Align2::LEFT_CENTER,
        fit(
            area.painter(),
            title,
            &FontId::proportional(12.5),
            row.width() - 66.0,
        ),
        FontId::proportional(12.5),
        ink,
    );
    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(url)
}

/// What a card says when it has nothing to show. Every card has one: a blank
/// rectangle looks broken, and "there is nothing here yet" does not.
fn empty(area: &mut Ui, palette: &Palette, body: Rect, message: &str) {
    area.painter().text(
        pos2(body.min.x, body.min.y + 10.0),
        Align2::LEFT_CENTER,
        fit(
            area.painter(),
            message,
            &FontId::proportional(11.5),
            body.width(),
        ),
        FontId::proportional(11.5),
        palette.text_muted,
    );
}
