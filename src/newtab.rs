//! Native New Tab dashboard. The browser owns search, and the profile owns
//! the cards' data so the page needs no network requests of its own.

use egui::{CursorIcon, FontId, Id, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2};

use crate::glass::{self, Glass};
use crate::icons::{self, Icon};
use crate::l10n::{message, text as tr};
use crate::overlay_glass::{self, Role, Surface};
use crate::settings::{NewTabBackground, SearchEngine};
use crate::theme::{self, Palette, Tier, Translucency};
use crate::ui::{ChromeContext, UiAction};

const GAP: f32 = 16.0;
const CARD_HEIGHT: f32 = 178.0;

pub fn draw(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    page: Rect,
    content: Rect,
    actions: &mut Vec<UiAction>,
) {
    let background = chrome.settings.new_tab_background;
    let material = match background.material() {
        _ if chrome.native_glass.reduce_transparency => Translucency::Solid,
        Translucency::LiquidGlass if !chrome.native_glass.window_backdrop => Translucency::Frosted,
        material => material,
    };
    let palette = chrome.palette.with_translucency(material);
    backdrop(root, chrome, page, &palette, background);
    if content.width() < 140.0 || content.height() < 80.0 {
        return;
    }

    let inset = (content.width() * 0.045).clamp(18.0, 48.0);
    let viewport = content.shrink2(vec2(inset, 12.0));
    let width = viewport.width().min(1040.0);
    let area = Rect::from_center_size(viewport.center(), vec2(width, viewport.height()));
    let mut dashboard = root.new_child(UiBuilder::new().id_salt("newtab_dashboard").max_rect(area));
    dashboard.set_clip_rect(root.clip_rect().intersect(viewport));
    egui::ScrollArea::vertical()
        .id_salt("newtab_scroll")
        .max_height(area.height())
        .auto_shrink([false, false])
        .show(&mut dashboard, |ui| {
            let width = ui.available_width();
            ui.add_space(chrome.window_controls_room());
            header(ui, &palette, width, actions);
            ui.add_space((area.height() * 0.10).clamp(20.0, 76.0));
            search(ui, chrome, &palette, width, actions);
            ui.add_space(22.0);
            cards(ui, chrome, &palette, width, actions);
            ui.add_space(10.0);
        });
}

fn backdrop(
    ui: &Ui,
    chrome: &ChromeContext,
    page: Rect,
    palette: &Palette,
    background: NewTabBackground,
) {
    let ground = match (palette.translucency, chrome.native_glass.vibrancy_available) {
        (Translucency::LiquidGlass, true) => theme::page_veil(palette, 0.48),
        (Translucency::Frosted, true) => theme::page_veil(palette, 0.68),
        _ => theme::page_base(palette),
    };
    let painter = ui
        .ctx()
        .layer_painter(egui::LayerId::background())
        .with_clip_rect(page);
    painter.rect_filled(page, theme::content_corners(palette), ground);
    if palette.translucency.is_translucent() {
        let strength = if palette.dark { 0.28 } else { 0.16 };
        let spread = page.width().min(page.height());
        for (x, y, index, size) in [
            (0.18, 0.22, 4, 0.82),
            (0.81, 0.48, 0, 0.77),
            (0.56, 0.9, 1, 0.67),
        ] {
            let color = theme::mix(palette.accent, theme::workspace_color(index), 0.58);
            let center = pos2(
                page.left() + page.width() * x,
                page.top() + page.height() * y,
            );
            glow(
                &painter,
                center,
                spread * size,
                color.gamma_multiply(strength),
            );
        }
    }
    if background == NewTabBackground::Grid {
        let tint = palette
            .accent
            .gamma_multiply(if palette.dark { 0.21 } else { 0.15 });
        let mut y = page.top() + 29.0;
        while y < page.bottom() {
            let mut x = page.left() + 29.0;
            while x < page.right() {
                painter.circle_filled(pos2(x, y), 0.8, tint);
                x += 29.0;
            }
            y += 29.0;
        }
    }
    if let Some(capture) = &chrome.capture {
        crate::backdrop::capture_into(&painter, page, capture);
    }
}

fn glow(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: egui::Color32) {
    const SEGMENTS: u32 = 48;
    let mut mesh = egui::epaint::Mesh::default();
    mesh.colored_vertex(center, color);
    for i in 0..SEGMENTS {
        let angle = std::f32::consts::TAU * i as f32 / SEGMENTS as f32;
        mesh.colored_vertex(
            center + vec2(angle.cos(), angle.sin()) * radius,
            egui::Color32::TRANSPARENT,
        );
        mesh.add_triangle(0, i + 1, (i + 1) % SEGMENTS + 1);
    }
    painter.add(egui::Shape::mesh(mesh));
}

fn header(ui: &mut Ui, palette: &Palette, width: f32, actions: &mut Vec<UiAction>) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 40.0), Sense::hover());
    let badge = Rect::from_center_size(rect.left_center() + vec2(19.0, 0.0), vec2(38.0, 38.0));
    crate::ui::paint_application_icon(ui, badge, palette.dark);
    text(
        ui,
        rect.shrink2(vec2(46.0, 0.0)),
        zervo_core::APP_NAME,
        20.0,
        palette.text,
    );
    let settings = Rect::from_center_size(rect.right_center() - vec2(19.0, 0.0), vec2(38.0, 38.0));
    if button(
        ui,
        settings,
        "newtab_settings",
        tr("common-settings"),
        palette,
    )
    .on_hover_text(tr("common-settings"))
    .clicked()
    {
        actions.push(UiAction::CustomizeChrome);
    }
    draw_icon(ui, settings.center(), Icon::Sliders, palette.text);
}

fn search(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    width: f32,
    actions: &mut Vec<UiAction>,
) {
    let engine = chrome.settings.search_engine;
    let capsule_width = width.min(660.0);
    if width >= 330.0 {
        let (engines, _) = ui.allocate_exact_size(vec2(width, 34.0), Sense::hover());
        let labels = SearchEngine::ALL.map(|engine| engine.label());
        let bounds = Rect::from_center_size(engines.center(), vec2(330.0, 32.0));
        let mut selector =
            ui.new_child(UiBuilder::new().id_salt("newtab_engines").max_rect(bounds));
        let current = SearchEngine::ALL
            .iter()
            .position(|value| *value == engine)
            .unwrap_or(0);
        if let Some(selected) = crate::widgets::segmented(&mut selector, current, &labels, palette)
        {
            chrome.settings.search_engine = SearchEngine::ALL[selected];
            actions.push(UiAction::PersistSettings);
            ui.ctx().request_repaint();
        }
        ui.add_space(10.0);
    }
    let (slot, _) = ui.allocate_exact_size(vec2(width, 54.0), Sense::hover());
    let bounds = Rect::from_center_size(slot.center(), vec2(capsule_width, 54.0));
    if chrome
        .browser
        .command_bar
        .as_ref()
        .is_some_and(crate::ui::CommandBar::from_home)
        || !ui.is_rect_visible(bounds)
    {
        return;
    }
    let surface = glass_surface(ui, bounds, palette, Role::SearchPill);
    let hint = message(
        "newtab-search-compact",
        [("engine", chrome.settings.search_engine.label().into())],
    );
    let response = button(ui, bounds, "newtab_search", &hint, palette);
    let submit = Rect::from_center_size(bounds.right_center() - vec2(24.0, 0.0), vec2(32.0, 32.0));
    ui.painter()
        .circle_filled(submit.center(), 16.0, palette.accent.gamma_multiply(0.24));
    draw_icon(ui, submit.center(), Icon::Forward, palette.text);
    draw_icon(
        ui,
        bounds.left_center() + vec2(25.0, 0.0),
        Icon::Search,
        palette.text_muted,
    );
    text(
        ui,
        bounds
            .with_min_x(bounds.left() + 46.0)
            .with_max_x(submit.left() - 9.0),
        &hint,
        15.0,
        palette.text,
    );
    surface.finish(ui, bounds, Role::SearchPill);
    if response.clicked() {
        actions.push(UiAction::OpenCommandFrom(bounds));
    }
}

fn cards(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    width: f32,
    actions: &mut Vec<UiAction>,
) {
    let columns = if width >= 770.0 {
        3
    } else if width >= 490.0 {
        2
    } else {
        1
    };
    let mut index = 0;
    while index < 5 {
        let count = columns.min(5 - index);
        let (row, _) = ui.allocate_exact_size(vec2(width, CARD_HEIGHT), Sense::hover());
        let cell = (width - GAP * (count - 1) as f32) / count as f32;
        for col in 0..count {
            let rect = Rect::from_min_size(
                row.min + vec2((cell + GAP) * col as f32, 0.0),
                vec2(cell, CARD_HEIGHT),
            );
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let surface = glass_surface(ui, rect, palette, Role::DashboardCard);
            let mut card = ui.new_child(
                UiBuilder::new()
                    .id_salt(("newtab_card", index + col))
                    .max_rect(rect),
            );
            card.set_clip_rect(ui.clip_rect().intersect(rect));
            let inner = rect.shrink(15.0);
            match index + col {
                0 => clock(&card, palette, inner),
                1 => recent(&card, chrome, palette, inner, actions),
                2 => pinned(&card, chrome, palette, inner, actions),
                3 => checklist(&mut card, chrome, palette, inner),
                4 => pulse(&card, chrome, palette, inner),
                _ => unreachable!(),
            }
            surface.finish(ui, rect, Role::DashboardCard);
        }
        index += count;
        if index < 5 {
            ui.add_space(GAP);
        }
    }
}

fn clock(ui: &Ui, palette: &Palette, rect: Rect) {
    let now = jiff::ZonedDateTime::now();
    heading(ui, rect, palette, Icon::Clock, tr("newtab-clock"));
    text(
        ui,
        band(rect, 35.0, 67.0),
        &now.strftime("%H:%M").to_string(),
        55.0,
        palette.text,
    );
    text(
        ui,
        band(rect, 115.0, 24.0),
        &crate::dashboard::calendar_date(&now, false),
        13.0,
        palette.text_muted,
    );
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs((60 - now.second()) as u64));
}

fn recent(
    ui: &Ui,
    chrome: &ChromeContext,
    palette: &Palette,
    rect: Rect,
    actions: &mut Vec<UiAction>,
) {
    heading(ui, rect, palette, Icon::Recent, tr("newtab-recent"));
    let mut shown = std::collections::HashSet::with_capacity(3);
    for (index, visit) in chrome
        .library
        .history
        .iter()
        .rev()
        .filter(|visit| shown.insert(visit.url.as_str()))
        .take(3)
        .enumerate()
    {
        let row = band(rect, 35.0 + index as f32 * 37.0, 35.0);
        let title = crate::ui::display_name(&visit.title, &visit.url);
        if button(ui, row, ("newtab_visit", index), title, palette)
            .on_hover_text(title)
            .clicked()
        {
            actions.push(UiAction::Navigate(visit.url.clone()));
        }
        draw_icon(
            ui,
            row.left_center() + vec2(13.0, 0.0),
            Icon::Globe,
            palette.text_muted,
        );
        let labels = row
            .with_min_x(row.left() + 30.0)
            .with_max_x(row.right() - 4.0);
        text(ui, band(labels, 1.0, 18.0), title, 12.5, palette.text);
        text(
            ui,
            band(labels, 18.0, 14.0),
            &crate::ui::host_of(&visit.url),
            10.5,
            palette.text_muted,
        );
    }
    if shown.is_empty() {
        text(
            ui,
            band(rect, 37.0, 24.0),
            tr("newtab-nothing-visited-yet"),
            13.0,
            palette.text_muted,
        );
    }
}

fn pinned(
    ui: &Ui,
    chrome: &ChromeContext,
    palette: &Palette,
    rect: Rect,
    actions: &mut Vec<UiAction>,
) {
    heading(ui, rect, palette, Icon::Pin, tr("sidebar-pinned-tabs"));
    let tabs = chrome
        .browser
        .workspaces
        .iter()
        .flat_map(|space| &space.tabs)
        .filter(|tab| tab.pinned);
    let mut count = 0;
    let width = (rect.width() - 16.0) / 3.0;
    for tab in tabs.take(6) {
        let x = count % 3;
        let y = count / 3;
        let tile = Rect::from_min_size(
            rect.min + vec2((width + 8.0) * x as f32, 37.0 + 55.0 * y as f32),
            vec2(width, 49.0),
        );
        ui.painter()
            .rect_filled(tile, 11, palette.surface_hover.gamma_multiply(0.45));
        let title = crate::ui::display_name(&tab.title, &tab.url);
        if button(ui, tile, ("newtab_pinned", tab.id), title, palette)
            .on_hover_text(title)
            .clicked()
        {
            actions.push(UiAction::SelectTab(tab.id));
        }
        let mark = tile.center_top() + vec2(0.0, 15.0);
        if let Some(icon) = chrome.favicons.get(&tab.id) {
            ui.painter().image(
                icon.id(),
                Rect::from_center_size(mark, vec2(16.0, 16.0)),
                Rect::from_min_max(egui::Pos2::ZERO, pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        } else {
            draw_icon(ui, mark, Icon::Globe, palette.text_muted);
        }
        text(
            ui,
            band(tile.shrink2(vec2(5.0, 0.0)), 25.0, 16.0),
            title,
            10.5,
            palette.text,
        );
        count += 1;
    }
    if count == 0 {
        text(
            ui,
            band(rect, 37.0, 24.0),
            tr("newtab-pinned-empty"),
            13.0,
            palette.text_muted,
        );
    }
}

fn checklist(ui: &mut Ui, chrome: &mut ChromeContext, palette: &Palette, rect: Rect) {
    heading(ui, rect, palette, Icon::Todo, tr("newtab-checklist"));
    let list = band(rect, 30.0, 82.0);
    let mut body = ui.new_child(UiBuilder::new().id_salt("newtab_task_list").max_rect(list));
    body.set_clip_rect(ui.clip_rect().intersect(list));
    if chrome.library.tasks.is_empty() {
        text(
            ui,
            band(list, 0.0, 27.0),
            tr("newtab-checklist-empty"),
            13.0,
            palette.text_muted,
        );
    }
    let mut edit = None;
    egui::ScrollArea::vertical()
        .max_height(list.height())
        .auto_shrink([false, false])
        .show(&mut body, |ui| {
            for (index, task) in chrome.library.tasks.iter().enumerate() {
                let (row, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
                let remove = row.with_min_x(row.right() - 25.0);
                let check = row.with_max_x(remove.left() - 2.0);
                let response =
                    ui.interact(check, Id::unique(("newtab_task", index)), Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::selected(egui::Role::CheckBox, true, task.done, &task.text)
                });
                if response.has_focus() {
                    icons::paint_focus(ui, palette, check, Tier::Control);
                }
                if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    edit = Some((index, false));
                }
                draw_icon(
                    ui,
                    check.left_center() + vec2(10.0, 0.0),
                    if task.done {
                        Icon::CheckSquare
                    } else {
                        Icon::Square
                    },
                    if task.done {
                        palette.accent
                    } else {
                        palette.text_muted
                    },
                );
                text(
                    ui,
                    check.with_min_x(check.left() + 25.0),
                    &task.text,
                    12.5,
                    if task.done {
                        palette.text_muted
                    } else {
                        palette.text
                    },
                );
                if button(
                    ui,
                    remove,
                    ("newtab_remove_task", index),
                    tr("newtab-delete-task"),
                    palette,
                )
                .on_hover_text(tr("newtab-delete-task"))
                .clicked()
                {
                    edit = Some((index, true));
                }
                draw_icon(ui, remove.center(), Icon::Close, palette.text_muted);
            }
        });
    if let Some((index, remove)) = edit {
        chrome.library.edit_task(index, remove);
        ui.ctx().request_repaint();
    }

    let input = band(rect, 117.0, 32.0);
    let id = Id::unique("newtab_task_draft");
    let mut draft = ui
        .ctx()
        .data(|data| data.get_temp::<String>(id))
        .unwrap_or_default();
    let add = input.with_min_x(input.right() - 35.0);
    let editor = input.with_max_x(add.left() - 5.0);
    let response = ui.put(
        editor,
        egui::TextEdit::singleline(&mut draft)
            .hint_text(tr("newtab-add-something"))
            .align(egui::Align2::LEFT_CENTER)
            .margin(egui::Margin::symmetric(8, 5)),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::TextInput, true, tr("newtab-add-task"))
    });
    let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    let clicked = button(ui, add, "newtab_add_task", tr("newtab-add-task"), palette).clicked();
    draw_icon(ui, add.center(), Icon::Plus, palette.text);
    if (enter || clicked) && chrome.library.add_task(&draft) {
        draft.clear();
        response.request_focus();
        ui.ctx().request_repaint();
    }
    ui.ctx().data_mut(|data| data.insert_temp(id, draft));
}

fn pulse(ui: &Ui, chrome: &ChromeContext, palette: &Palette, rect: Rect) {
    heading(
        ui,
        rect,
        palette,
        Icon::TrendUp,
        tr("newtab-workspace-pulse"),
    );
    let spaces = &chrome.browser.workspaces;
    if let Some(current) = spaces.get(chrome.browser.active_workspace) {
        text(
            ui,
            band(rect, 27.0, 24.0),
            &current.name,
            14.0,
            palette.text_muted,
        );
    }
    let stats = [
        (spaces.len(), tr("newtab-workspaces")),
        (
            spaces.iter().map(|space| space.tabs.len()).sum(),
            tr("newtab-stat-tabs"),
        ),
        (
            spaces
                .iter()
                .flat_map(|space| &space.tabs)
                .filter(|tab| tab.pinned)
                .count(),
            tr("newtab-stat-pinned"),
        ),
    ];
    let width = (rect.width() - 14.0) / 3.0;
    for (index, (value, label)) in stats.into_iter().enumerate() {
        let box_rect = Rect::from_min_size(
            rect.min + vec2((width + 7.0) * index as f32, 61.0),
            vec2(width, 77.0),
        );
        ui.painter()
            .rect_filled(box_rect, 12, palette.surface_hover.gamma_multiply(0.44));
        let body = box_rect.shrink2(vec2(8.0, 0.0));
        text(
            ui,
            band(body, 8.0, 37.0),
            &value.to_string(),
            28.0,
            palette.text,
        );
        text(ui, band(body, 48.0, 21.0), label, 11.0, palette.text_muted);
    }
}

fn glass_surface(ui: &Ui, rect: Rect, palette: &Palette, role: Role) -> Surface {
    let native = Surface::begin_if(ui, palette.translucency == Translucency::LiquidGlass);
    if !native.is_native() {
        glass::paint(
            ui.painter(),
            rect,
            palette,
            Glass::tier(Tier::Card)
                .radius_exact(role.radius())
                .border(palette.border)
                .opaque(palette.surface),
        );
    }
    native
}

fn heading(ui: &Ui, rect: Rect, palette: &Palette, glyph: Icon, label: &str) {
    draw_icon(
        ui,
        band(rect, 0.0, 20.0).left_center() + vec2(8.0, 0.0),
        glyph,
        palette.accent,
    );
    text(
        ui,
        band(rect, 0.0, 20.0).with_min_x(rect.left() + 23.0),
        label,
        13.0,
        palette.text,
    );
}

fn button(
    ui: &Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    palette: &Palette,
) -> egui::Response {
    let response = ui.interact(rect, Id::unique(id), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let radius = (rect.height() * 0.5).min(27.0) as u8;
    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter()
            .rect_filled(rect, radius, palette.surface_hover.gamma_multiply(0.62));
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, palette.accent),
            StrokeKind::Inside,
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn band(rect: Rect, y: f32, height: f32) -> Rect {
    Rect::from_min_size(rect.min + vec2(0.0, y), vec2(rect.width(), height))
}

fn draw_icon(ui: &Ui, center: egui::Pos2, icon: Icon, ink: egui::Color32) {
    icons::draw_icon(
        ui.painter(),
        Rect::from_center_size(center, vec2(16.0, 16.0)),
        icon,
        ink,
    );
}

fn text(ui: &Ui, rect: Rect, label: &str, size: f32, ink: egui::Color32) {
    overlay_glass::row_label(ui, rect, label, FontId::proportional(size), ink);
}
