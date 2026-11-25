//! Responsive search-and-widget dashboard for a newly opened tab.

use egui::{
    Align2, Color32, CursorIcon, FontId, Id, Key, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    UiBuilder, pos2, vec2,
};

use crate::dashboard::{self, Row};
use crate::glass::{self, Glass};
use crate::icons::{self, Icon};
use crate::l10n::{message, text as tr};
use crate::overlay_glass::{Role, Surface};
use crate::settings::SearchEngine;
use crate::theme::{self, Palette, Tier};
use crate::ui::{ChromeContext, UiAction};

use super::{Widget, calendar_date, fit, widgets};

const GAP: f32 = 14.0;
const SEARCH_HEIGHT: f32 = 54.0;
const SHORTCUTS_HEIGHT: f32 = 144.0;
const ACTIVITY_HEIGHT: f32 = 204.0;
const WIDE: f32 = 640.0;

pub(super) fn draw(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    rect: Rect,
    actions: &mut Vec<UiAction>,
) {
    if !rect.is_positive() {
        return;
    }
    let width = rect.width().min(984.0);
    let viewport = Rect::from_center_size(rect.center(), vec2(width, rect.height()));
    let mut view = root.new_child(
        UiBuilder::new()
            .max_rect(viewport)
            .id_salt("newtab_home")
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    view.set_clip_rect(root.clip_rect().intersect(viewport));

    let editing = super::editing(chrome);
    let mut layout = chrome.settings.visible_newtab_widgets().to_vec();
    let columns = if width < WIDE { 1 } else { 2 };
    let mut edit = None;
    egui::ScrollArea::vertical()
        .id_salt("newtab_dashboard")
        .max_height(viewport.height())
        .auto_shrink([false, false])
        .show(&mut view, |ui| {
            let width = ui.available_width();
            let height = layout
                .chunks(columns)
                .map(|row| row_height(row) + GAP)
                .sum::<f32>();
            let inset = ((viewport.height() - SEARCH_HEIGHT - height - GAP) * 0.34).max(0.0);
            ui.add_space(inset);
            let (search_box, _) =
                ui.allocate_exact_size(vec2(width, SEARCH_HEIGHT), Sense::hover());
            search(ui, chrome, search_box, actions);
            ui.add_space(GAP);
            if editing {
                edit = layout_controls(ui, &chrome.palette, &layout);
                ui.add_space(GAP);
            }

            for (row_index, row) in layout.chunks(columns).enumerate() {
                let toolbar_height = if editing { 32.0 } else { 0.0 };
                let height = row_height(row) + toolbar_height;
                let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
                let bounds = match *row {
                    [left, right] => {
                        let (a, b) = split(rect, width_fraction(left, right));
                        [a, b]
                    },
                    _ => [rect, Rect::NOTHING],
                };
                for (column, (&widget, rect)) in row.iter().zip(bounds).enumerate() {
                    if editing {
                        let index = row_index * columns + column;
                        let toolbar = rect.with_max_y(rect.top() + 27.0);
                        edit = widget_controls(
                            ui,
                            &chrome.palette,
                            toolbar,
                            widget,
                            index,
                            layout.len(),
                        )
                        .or(edit);
                    }
                    draw_widget(
                        ui,
                        chrome,
                        widget,
                        rect.with_min_y(rect.top() + toolbar_height),
                        actions,
                    );
                }
                ui.add_space(GAP);
            }
        });

    if let Some(edit) = edit {
        match edit {
            Edit::Add(widget) => layout.push(widget),
            Edit::Remove(index) => {
                layout.remove(index);
            },
            Edit::Move(from, to) => layout.swap(from, to),
            Edit::Reset => layout = Widget::DEFAULT.to_vec(),
        }
        chrome.settings.newtab_widgets = Some(layout);
        actions.push(UiAction::PersistSettings);
    }
}

#[derive(Clone, Copy)]
enum Edit {
    Add(Widget),
    Remove(usize),
    Move(usize, usize),
    Reset,
}

fn row_height(widgets: &[Widget]) -> f32 {
    if widgets.iter().any(|widget| {
        !matches!(
            widget,
            Widget::Clock | Widget::QuickLinks | Widget::NowPlaying
        )
    }) {
        ACTIVITY_HEIGHT
    } else {
        SHORTCUTS_HEIGHT
    }
}

fn width_fraction(left: Widget, right: Widget) -> f32 {
    match (left, right) {
        (Widget::Clock, _) => 0.33,
        (_, Widget::Clock) => 0.67,
        (Widget::Recent, _) => 0.60,
        (_, Widget::Recent) => 0.40,
        _ => 0.50,
    }
}

fn layout_controls(ui: &mut Ui, palette: &Palette, layout: &[Widget]) -> Option<Edit> {
    let available: Vec<_> = Widget::ALL
        .into_iter()
        .filter(|widget| !layout.contains(widget))
        .map(|widget| Row::Item(widget.label().to_owned(), widget, false))
        .collect();
    let (add, reset) = ui
        .horizontal(|ui| {
            let add = (!available.is_empty()).then(|| {
                crate::widgets::label_button(ui, palette, tr("newtab-add-card"), 28.0, 22.0, false)
            });
            let reset =
                crate::widgets::label_button(ui, palette, tr("newtab-reset"), 28.0, 22.0, false)
                    .clicked();
            (add, reset)
        })
        .inner;
    if reset {
        return Some(Edit::Reset);
    }
    popup(ui, palette, "newtab_available_widgets", add?, &available).map(Edit::Add)
}

fn widget_controls(
    ui: &Ui,
    palette: &Palette,
    rect: Rect,
    widget: Widget,
    index: usize,
    count: usize,
) -> Option<Edit> {
    let name = widget.label();
    label(
        ui,
        rect.left_center() + vec2(5.0, 0.0),
        name,
        rect.width() - 87.0,
        12.0,
        palette.text,
    );
    let controls = [
        (
            Icon::Close,
            tr("newtab-take-this-card-off").to_owned(),
            Some(Edit::Remove(index)),
        ),
        (
            Icon::Forward,
            message("layout-move-down", [("name", name.into())]),
            (index + 1 < count).then_some(Edit::Move(index, index + 1)),
        ),
        (
            Icon::Back,
            message("layout-move-up", [("name", name.into())]),
            index.checked_sub(1).map(|before| Edit::Move(index, before)),
        ),
    ];
    let mut result = None;
    for (i, (glyph, label, edit)) in controls.into_iter().enumerate() {
        if let Some(edit) = edit {
            let x = rect.right() - 13.0 - i as f32 * 25.0;
            let slot = Rect::from_center_size(pos2(x, rect.center().y), vec2(24.0, 24.0));
            let response = button(
                ui,
                Id::unique(("newtab_widget_control", widget, i)),
                slot,
                &label,
                palette,
            );
            icon(ui, x, rect.center().y, 13.0, glyph, palette.text);
            if crate::overlay_glass::tooltip(response, palette, &label).clicked() {
                result = Some(edit);
            }
        }
    }
    result
}

fn draw_widget(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    widget: Widget,
    rect: Rect,
    actions: &mut Vec<UiAction>,
) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let glyph = match widget {
        Widget::Clock => return clock_card(ui, chrome, rect),
        Widget::QuickLinks => return shortcuts(ui, chrome, rect, actions),
        Widget::Recent => return recent(ui, chrome, rect, actions),
        Widget::Writing => return writing(ui, chrome, rect),
        Widget::WorldClocks => Icon::World,
        Widget::PinnedTabs => Icon::Pin,
        Widget::MostVisited => Icon::TrendUp,
        Widget::Downloads => Icon::Download,
        Widget::NowPlaying => Icon::Music,
        Widget::Spaces => Icon::Stack,
    };
    let palette = chrome.palette;
    let surface = material(ui, &palette, rect, Role::Widget);
    let inner = rect.shrink(16.0);
    heading(ui, inner, glyph, widget.label(), &palette);
    let body = inner.with_min_y(inner.top() + 23.0);
    let id = Id::unique(("newtab_widget", widget));
    match widget {
        Widget::WorldClocks => widgets::draw_world_clocks(ui, chrome, body),
        Widget::PinnedTabs => widgets::draw_quick_links(ui, chrome, body, id, actions),
        Widget::MostVisited => widgets::draw_top_sites(ui, chrome, body, id, actions),
        Widget::Downloads => widgets::draw_downloads(ui, chrome, body, id, actions),
        Widget::NowPlaying => widgets::draw_now_playing(ui, chrome, body, id, actions),
        Widget::Spaces => widgets::draw_workspaces(ui, chrome, body, id, actions),
        _ => unreachable!(),
    }
    surface.finish(ui, rect, Role::Widget);
}

pub(super) fn compact(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    rect: Rect,
    actions: &mut Vec<UiAction>,
) {
    if rect.width() >= 116.0 && rect.height() >= 48.0 {
        let field =
            Rect::from_center_size(rect.center(), vec2((rect.width() - 22.0).min(596.0), 44.0));
        search(root, chrome, field, actions);
    }
}

fn split(row: Rect, fraction: f32) -> (Rect, Rect) {
    let x = row.left() + (row.width() - GAP) * fraction;
    (row.with_max_x(x), row.with_min_x(x + GAP))
}

fn material(ui: &Ui, palette: &Palette, rect: Rect, role: Role) -> Surface {
    let surface = Surface::begin(ui);
    if !surface.is_native() {
        glass::paint(
            ui.painter(),
            rect,
            palette,
            Glass::tier(Tier::Card)
                .radius_exact(role.radius())
                .strength(0.9)
                .border(palette.border)
                .opaque(palette.bg),
        );
    }
    surface
}

fn icon(ui: &Ui, x: f32, y: f32, size: f32, kind: Icon, tint: Color32) {
    icons::draw_icon(
        ui.painter(),
        Rect::from_center_size(pos2(x, y), vec2(size, size)),
        kind,
        tint,
    );
}

fn label(ui: &Ui, point: egui::Pos2, text: &str, width: f32, size: f32, color: Color32) {
    let font = FontId::proportional(size);
    ui.painter().text(
        point,
        Align2::LEFT_CENTER,
        fit(ui.painter(), text, &font, width.max(1.0)),
        font,
        color,
    );
}

fn heading(ui: &Ui, rect: Rect, glyph: Icon, name: &str, palette: &Palette) {
    icon(
        ui,
        rect.left() + 6.0,
        rect.top() + 7.0,
        13.0,
        glyph,
        palette.text_muted,
    );
    label(
        ui,
        pos2(rect.left() + 20.0, rect.top() + 7.0),
        name,
        rect.width() - 20.0,
        12.5,
        palette.text,
    );
}

fn button(ui: &Ui, id: Id, rect: Rect, name: &str, palette: &Palette) -> Response {
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter()
            .rect_filled(rect, palette.corner(Tier::Row), palette.surface_hover);
    }
    if response.has_focus() {
        icons::paint_focus(ui, palette, rect, Tier::Row);
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn search(ui: &mut Ui, chrome: &mut ChromeContext, rect: Rect, actions: &mut Vec<UiAction>) {
    if !ui.is_rect_visible(rect)
        || chrome
            .browser
            .command_bar
            .as_ref()
            .is_some_and(crate::ui::CommandBar::from_home)
    {
        return;
    }
    let palette = chrome.palette;
    let engine = chrome.settings.search_engine;
    let surface = material(ui, &palette, rect, Role::Field);
    ui.painter().rect_stroke(
        rect,
        Role::Field.radius(),
        Stroke::new(1.0, palette.border.gamma_multiply(0.7)),
        StrokeKind::Inside,
    );
    let show_provider = rect.width() >= 356.0;
    let input = rect
        .shrink(5.0)
        .with_max_x(rect.right() - if show_provider { 134.0 } else { 5.0 });
    let hint = message("newtab-search-compact", [("engine", engine.label().into())]);
    let response = button(ui, Id::unique("newtab_search"), input, &hint, &palette);
    icon(
        ui,
        input.left() + 17.0,
        input.center().y,
        17.0,
        Icon::Search,
        palette.text_muted,
    );
    let shortcut = if cfg!(target_os = "macos") {
        "⌘L"
    } else {
        "Ctrl+L"
    };
    let hotkey_width = if input.width() > 264.0 { 56.0 } else { 0.0 };
    label(
        ui,
        pos2(input.left() + 35.0, input.center().y),
        &hint,
        input.width() - 45.0 - hotkey_width,
        15.0,
        palette.text,
    );
    if hotkey_width > 0.0 {
        ui.painter().text(
            input.right_center() - vec2(11.0, 0.0),
            Align2::RIGHT_CENTER,
            shortcut,
            FontId::proportional(12.0),
            palette.text_muted,
        );
    }
    if response.clicked() {
        actions.push(UiAction::OpenCommandFrom(rect));
    }

    let provider = show_provider.then(|| {
        let bounds = Rect::from_min_max(
            pos2(input.right() + 5.0, rect.top() + 7.0),
            rect.right_bottom() - vec2(7.0, 7.0),
        );
        let response = ui.interact(bounds, Id::unique("newtab_search_provider"), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, true, tr("settings-search-engine"))
        });
        let tint = crate::widgets::button_ink(ui, &response, bounds, &palette, false);
        icon(
            ui,
            bounds.left() + 14.0,
            bounds.center().y,
            13.0,
            Icon::Globe,
            tint,
        );
        label(
            ui,
            pos2(bounds.left() + 26.0, bounds.center().y),
            engine.label(),
            bounds.width() - 45.0,
            12.0,
            tint,
        );
        icon(
            ui,
            bounds.right() - 12.0,
            bounds.center().y,
            10.0,
            Icon::CaretDown,
            tint,
        );
        response
    });
    surface.finish(ui, rect, Role::Field);

    if let Some(trigger) = provider {
        let rows = SearchEngine::ALL
            .map(|option| Row::Item(option.label().to_owned(), option, option == engine));
        if let Some(option) = popup(ui, &palette, "newtab_providers", trigger, &rows)
            && option != engine
        {
            chrome.settings.search_engine = option;
            actions.push(UiAction::SettingsChanged);
        }
    }
}

fn popup<T: Copy>(
    ui: &mut Ui,
    palette: &Palette,
    name: &str,
    trigger: Response,
    rows: &[Row<T>],
) -> Option<T> {
    let id = Id::unique(name).with("open");
    let mut open = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(id))
        .unwrap_or(false);
    if trigger.clicked() {
        open = !open;
        ui.ctx().data_mut(|data| data.insert_temp(id, open));
    }
    if !open {
        return None;
    }

    let selected = dashboard::menu(ui, palette, name, trigger.rect, 198.0, rows);
    if selected.is_some()
        || ui.input(|input| input.key_pressed(Key::Escape))
        || (ui.input(|input| input.pointer.any_pressed())
            && !trigger.hovered()
            && !dashboard::over_menu(ui.ctx()))
    {
        ui.ctx().data_mut(|data| data.insert_temp(id, false));
    }
    selected
}

fn clock_card(ui: &Ui, chrome: &ChromeContext, rect: Rect) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let palette = &chrome.palette;
    let surface = material(ui, palette, rect, Role::Widget);
    let body = rect.shrink(16.0);
    let now = jiff::ZonedDateTime::now();
    let subtitle = chrome.settings.newtab_message.trim();
    let date = calendar_date(&now, false);
    heading(ui, body, Icon::Clock, &date, palette);
    label(
        ui,
        pos2(body.left(), rect.center().y + 3.0),
        &now.strftime("%H:%M").to_string(),
        body.width(),
        43.0,
        palette.text,
    );

    if !subtitle.is_empty() {
        label(
            ui,
            pos2(body.left(), body.bottom() - 4.0),
            subtitle,
            body.width(),
            12.5,
            palette.text_muted,
        );
    } else if let Some(space) = chrome
        .browser
        .workspaces
        .get(chrome.browser.active_workspace)
    {
        let tabs = message(
            "newtab-open-tab-count",
            [("count", space.tabs.len().into())],
        );
        label(
            ui,
            pos2(body.left(), body.bottom() - 4.0),
            &tabs,
            body.width(),
            12.5,
            palette.text_muted,
        );
    }

    surface.finish(ui, rect, Role::Widget);
    let next_minute = 60 - now.second() as u64;
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs(next_minute));
}

struct Link {
    title: String,
    url: String,
}

fn shortcuts(ui: &mut Ui, chrome: &mut ChromeContext, rect: Rect, actions: &mut Vec<UiAction>) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let palette = chrome.palette;
    let surface = material(ui, &palette, rect, Role::Widget);
    let body = rect.shrink(16.0);
    heading(ui, body, Icon::Apps, tr("newtab-quick-links"), &palette);
    let columns = ((body.width() / 78.0) as usize).clamp(2, 6);
    let width = body.width() / columns as f32;

    let mut links: Vec<Link> = chrome
        .library
        .favourites
        .iter()
        .take(columns - 1)
        .map(|entry| Link {
            title: crate::ui::display_name(&entry.title, &entry.url).to_owned(),
            url: entry.url.clone(),
        })
        .collect();
    if links.is_empty() {
        links.extend(
            chrome
                .library
                .top_sites(columns - 1)
                .iter()
                .map(|site| Link {
                    title: site.host.clone(),
                    url: site.url.clone(),
                }),
        );
    }

    let slot = |index: usize| {
        Rect::from_min_size(
            pos2(body.left() + index as f32 * width, body.top() + 28.0),
            vec2(width - 4.0, body.height() - 28.0),
        )
    };
    for (i, link) in links.iter().enumerate() {
        let rect = slot(i);
        let id = Id::unique("newtab_shortcut").with(&link.url);
        if button(ui, id, rect, &link.title, &palette)
            .on_hover_text(&link.title)
            .clicked()
        {
            actions.push(UiAction::Navigate(link.url.clone()));
        }
        let badge =
            Rect::from_center_size(pos2(rect.center().x, rect.top() + 26.0), vec2(32.0, 32.0));
        site_mark(ui, chrome, badge, &link.title, &link.url);
        centered_title(ui, rect, &link.title, &palette);
    }

    let plus = slot(links.len());
    let pin_label = tr("newtab-pin-a-site");
    let pin = button(
        ui,
        Id::unique("newtab_pin_shortcut"),
        plus,
        pin_label,
        &palette,
    );
    icon(
        ui,
        plus.center().x,
        plus.top() + 26.0,
        20.0,
        Icon::Plus,
        palette.text,
    );
    centered_title(ui, plus, pin_label, &palette);
    surface.finish(ui, rect, Role::Widget);

    let menu = "newtab_shortcut_pin_menu";
    let open = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(Id::unique(menu).with("open")))
        .unwrap_or(false);
    if !pin.clicked() && !open {
        return;
    }
    let choices: Vec<_> = chrome
        .library
        .top_sites(8)
        .iter()
        .map(|site| Link {
            title: site.host.clone(),
            url: site.url.clone(),
        })
        .collect();
    let rows = if choices.is_empty() {
        vec![Row::Heading(tr("newtab-nothing-visited-yet"))]
    } else {
        choices
            .iter()
            .enumerate()
            .map(|(i, item)| {
                Row::Item(
                    item.title.clone(),
                    i,
                    chrome.library.is_favourite(&item.url),
                )
            })
            .collect()
    };
    if let Some(index) = popup(ui, &palette, menu, pin, &rows)
        && let Some(item) = choices.get(index)
    {
        actions.push(UiAction::PinSite(item.url.clone(), item.title.clone()));
    }
}

fn centered_title(ui: &Ui, rect: Rect, title: &str, palette: &Palette) {
    let font = FontId::proportional(11.5);
    ui.painter().text(
        rect.center_bottom() - vec2(0.0, 10.0),
        Align2::CENTER_CENTER,
        fit(ui.painter(), title, &font, rect.width() - 4.0),
        font,
        palette.text,
    );
}

fn site_mark(ui: &Ui, chrome: &ChromeContext, badge: Rect, title: &str, url: &str) {
    let palette = &chrome.palette;
    let texture = chrome
        .browser
        .workspaces
        .iter()
        .flat_map(|space| space.tabs.iter())
        .filter(|tab| tab.url == url)
        .find_map(|tab| chrome.favicons.get(&tab.id));
    if let Some(texture) = texture {
        ui.painter().image(
            texture.id(),
            badge.shrink(3.0),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        ui.painter().rect_filled(
            badge,
            palette.corner(Tier::Row),
            theme::mix(palette.surface, palette.accent, 0.17),
        );
        ui.painter().text(
            badge.center(),
            Align2::CENTER_CENTER,
            crate::ui::initial(title, url),
            FontId::proportional(13.0),
            palette.text,
        );
    }
}

fn recent(ui: &mut Ui, chrome: &ChromeContext, rect: Rect, actions: &mut Vec<UiAction>) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let palette = &chrome.palette;
    let surface = material(ui, palette, rect, Role::Widget);
    let body = rect.shrink(16.0);
    heading(ui, body, Icon::Recent, tr("newtab-recent"), palette);

    let visits = chrome.library.recent(3);
    let now = jiff::Timestamp::now().as_second();
    for (index, visit) in visits.iter().enumerate() {
        let row = Rect::from_min_size(
            pos2(body.left() - 4.0, body.top() + 24.0 + index as f32 * 49.0),
            vec2(body.width() + 8.0, 46.0),
        );
        let title = crate::ui::display_name(&visit.title, &visit.url);
        if button(
            ui,
            Id::unique("newtab_recent").with(&visit.url),
            row,
            title,
            palette,
        )
        .on_hover_text(title)
        .clicked()
        {
            actions.push(UiAction::Navigate(visit.url.clone()));
        }
        let badge =
            Rect::from_center_size(pos2(row.left() + 22.0, row.center().y), vec2(29.0, 29.0));
        site_mark(ui, chrome, badge, title, &visit.url);
        label(
            ui,
            pos2(row.left() + 46.0, row.center().y - 8.0),
            title,
            row.width() - 57.0,
            13.0,
            palette.text,
        );
        let when = ago(now.saturating_sub(visit.at));
        let location = format!("{} · {when}", crate::ui::host_of(&visit.url));
        label(
            ui,
            pos2(row.left() + 46.0, row.center().y + 9.0),
            &location,
            row.width() - 57.0,
            11.5,
            palette.text_muted,
        );
    }
    if visits.is_empty() {
        label(
            ui,
            pos2(body.left(), body.top() + 40.0),
            tr("newtab-nothing-visited-yet"),
            body.width(),
            13.0,
            palette.text_muted,
        );
    }
    surface.finish(ui, rect, Role::Widget);
}

fn writing(ui: &mut Ui, chrome: &mut ChromeContext, rect: Rect) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let palette = chrome.palette;
    let surface = material(ui, &palette, rect, Role::Widget);
    let body = rect.shrink(16.0);
    let mut controls = ui.new_child(
        UiBuilder::new()
            .max_rect(body.with_max_y(body.top() + 28.0))
            .id_salt("newtab_writing_choices")
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let mode = Id::unique("newtab_writing_show_tasks");
    let mut tasks = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(mode))
        .unwrap_or(!chrome.library.tasks.is_empty());
    if crate::widgets::label_button(
        &mut controls,
        &palette,
        tr("newtab-note"),
        26.0,
        19.0,
        !tasks,
    )
    .clicked()
    {
        tasks = false;
    }
    if crate::widgets::label_button(
        &mut controls,
        &palette,
        tr("newtab-to-do"),
        26.0,
        19.0,
        tasks,
    )
    .clicked()
    {
        tasks = true;
    }
    ui.ctx().data_mut(|data| data.insert_temp(mode, tasks));

    let text = body.with_min_y(body.top() + 38.0);
    if tasks {
        widgets::draw_tasks(ui, chrome, text, mode.with("tasks"));
    } else {
        widgets::draw_notes(ui, chrome, text, mode.with("notes"));
    }
    surface.finish(ui, rect, Role::Widget);
}

fn ago(seconds: i64) -> String {
    let minutes = seconds.max(0) / 60;
    match minutes {
        0..=1 => tr("newtab-age-now").to_owned(),
        2..=59 => message("newtab-age-minutes", [("count", minutes.into())]),
        60..=1439 => message("newtab-age-hours", [("count", (minutes / 60).into())]),
        1440..=2879 => tr("newtab-age-yesterday").to_owned(),
        _ => message("newtab-age-days", [("count", (minutes / 1440).into())]),
    }
}
