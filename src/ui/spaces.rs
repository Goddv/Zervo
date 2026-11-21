use super::*;

pub(super) fn switcher(ui: &mut Ui, chrome: &ChromeContext, actions: &mut Vec<UiAction>) {
    let browser = &chrome.browser;
    let active = browser.active_workspace;
    let Some(space) = browser.workspaces.get(active) else {
        return;
    };
    let color = theme::workspace_color_from(active, chrome.settings.space_colour);
    ui.horizontal(|ui| {
        let button_width = (ui.available_width() - 60.0).max(1.0);
        ui.painter().circle_filled(
            pos2(ui.cursor().left() + 6.0, ui.cursor().top() + 16.0),
            4.0,
            color,
        );
        ui.add_space(16.0);
        let selector = egui::ComboBox::from_id_salt("zervo_space_switcher")
            .selected_text(&space.name)
            .truncate()
            .width(button_width)
            .show_glass(ui, |ui| {
                ui.label(RichText::new(tr("sidebar-spaces")).strong().size(12.0));
                for (index, entry) in browser.workspaces.iter().enumerate() {
                    if ui.selectable_label(index == active, &entry.name).clicked() {
                        actions.push(UiAction::SelectWorkspace(index));
                        ui.close();
                    }
                }
                ui.separator();
                if browser.can_remove_workspace(active)
                    && ui.button(tr("sidebar-delete-space")).clicked()
                {
                    actions.push(UiAction::RemoveWorkspace(active));
                    ui.close();
                }
                if ui.button(tr("chrome-new-workspace")).clicked() {
                    actions.push(UiAction::NewWorkspace);
                    ui.close();
                }
            });
        selector.response.on_hover_text(tr("sidebar-spaces"));
        if icons::icon_button(ui, Icon::Plus, 16.0, &chrome.palette, true)
            .on_hover_text(tr("chrome-new-workspace"))
            .clicked()
        {
            actions.push(UiAction::NewWorkspace);
        }
    });
}

fn heading(ui: &mut Ui, label: &str, palette: &Palette) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new(label)
                .size(11.5)
                .strong()
                .color(palette.text_muted),
        );
    });
}

fn favourites(ui: &mut Ui, chrome: &ChromeContext, actions: &mut Vec<UiAction>) {
    let sites = &chrome.library.favourites;
    if sites.is_empty() {
        return;
    }
    let palette = &chrome.palette;
    heading(ui, tr("common-favourites"), palette);
    let gap = 6.0;
    let columns = ((ui.available_width() + gap) / 49.0)
        .floor()
        .clamp(3.0, 6.0) as usize;
    let width = ((ui.available_width() - gap * (columns - 1) as f32) / columns as f32).floor();
    // Keep the sidebar quick to scan. The star menu retains the full library.
    for row in sites
        .iter()
        .take(columns * 2)
        .collect::<Vec<_>>()
        .chunks(columns)
    {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for site in row {
                let selected = chrome
                    .browser
                    .active_tab()
                    .is_some_and(|tab| tab.url == site.url);
                let (rect, response) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click());
                let fill = if selected || response.hovered() {
                    palette.surface_hover
                } else {
                    palette.surface
                };
                ui.painter().rect_filled(rect, 8, fill);
                ui.painter().rect_stroke(
                    rect,
                    8,
                    Stroke::new(1.0_f32, palette.border),
                    StrokeKind::Inside,
                );
                if response.has_focus() {
                    icons::paint_focus(ui, palette, rect, Tier::Control);
                }
                let open = chrome
                    .browser
                    .workspaces
                    .iter()
                    .flat_map(|space| &space.tabs)
                    .find(|tab| tab.url == site.url);
                if let Some(favicon) = open.and_then(|tab| chrome.favicons.get(&tab.id)) {
                    ui.painter().image(
                        favicon.id(),
                        Rect::from_center_size(rect.center(), vec2(18.0, 18.0)),
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else {
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        initial(&site.title, &site.url),
                        FontId::proportional(15.0),
                        palette.text,
                    );
                }
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        true,
                        selected,
                        &site.title,
                    )
                });
                crate::overlay_glass::context_menu(&response, |ui| {
                    if ui.button(tr("chrome-remove-from-favourites")).clicked() {
                        actions.push(UiAction::RemoveFavourite(site.url.clone()));
                        ui.close();
                    }
                });
                if response
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text(&site.title)
                    .clicked()
                {
                    actions.push(open.map_or_else(
                        || UiAction::OpenAddress(site.url.clone()),
                        |tab| UiAction::SelectTab(tab.id),
                    ));
                }
            }
        });
    }
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(6.0);
}

pub(super) fn body(ui: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    let active = chrome.browser.active_workspace;
    let Some(space) = chrome.browser.workspaces.get(active) else {
        return;
    };
    let palette = chrome.palette;
    let count = space.tabs.len();
    let mut rename = None;
    workspace_header(
        ui,
        active,
        chrome.settings.space_colour,
        &space.name,
        count,
        chrome.settings.show_tab_counts,
        true,
        chrome
            .browser
            .workspace_edit
            .as_ref()
            .filter(|(index, _)| *index == active)
            .map(|(_, name)| name.as_str()),
        &palette,
        &mut rename,
    );
    if let Some((index, edit)) = rename {
        match edit {
            WorkspaceName::Typing(name) => chrome.browser.workspace_edit = Some((index, name)),
            WorkspaceName::Keep(name) => {
                chrome.browser.workspace_edit = None;
                actions.push(UiAction::RenameWorkspace(index, name));
            },
            WorkspaceName::Discard => chrome.browser.workspace_edit = None,
        }
    }

    ui.add_space(7.0);
    let ctx = ui.ctx().clone();
    let drag_id = Id::new("zervo_tab_drag");
    let dragging = ctx.data(|data| data.get_temp::<TabId>(drag_id));
    let pointer = ctx.input(|input| input.pointer.latest_pos());
    egui::ScrollArea::vertical()
        .id_salt(("space_tabs", active))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            let mut target = None;
            let mut released = false;
            let mut ghost = None;
            for section in chrome.settings.sidebar_sections {
                use crate::settings::SidebarSection;
                if !section.visible(chrome.settings) {
                    continue;
                }
                if section == SidebarSection::Favourites {
                    favourites(ui, chrome, actions);
                    continue;
                }
                let tabs = space
                    .tabs
                    .iter()
                    .enumerate()
                    .filter(|(_, tab)| match section {
                        SidebarSection::Pinned => tab.pinned,
                        SidebarSection::Tabs => !tab.pinned || !chrome.settings.show_pinned_section,
                        SidebarSection::Favourites => false,
                    });
                if tabs.clone().next().is_some() {
                    heading(ui, section.label(), &palette);
                }
                for (index, tab) in tabs {
                    if dragging == Some(tab.id) {
                        ghost = Some(tab);
                    }
                    let out = tab_row(
                        ui,
                        TabRowStyle {
                            tab_id: tab.id,
                            title: &tab.title,
                            url: &tab.url,
                            selected: chrome.browser.active_tab == Some(tab.id),
                            loading: tab.loading,
                            pinnable: tab.kind == TabKind::Web,
                            pinned: tab.pinned,
                            workspace: active,
                            spaces: &chrome.browser.workspaces,
                            glyph: internal_glyph(tab.kind),
                            always_show_close: chrome.settings.always_show_tab_close,
                            compact: chrome.settings.compact_sidebar,
                            dragging,
                        },
                        &palette,
                        chrome.favicons.get(&tab.id),
                        actions,
                    );
                    if out.drag_started {
                        ctx.data_mut(|data| data.insert_temp(drag_id, tab.id));
                    }
                    released |= out.drag_stopped;
                    if let Some(drop) = out.drop_at {
                        target = Some(match drop {
                            DropAt::Before => (index, None),
                            DropAt::After => (index + 1, None),
                            DropAt::Onto => (index, Some(tab.id)),
                        });
                    }
                    if out.close_clicked {
                        actions.push(UiAction::CloseTab(tab.id));
                    } else if out.clicked {
                        actions.push(UiAction::SelectTab(tab.id));
                    }
                }
                ui.add_space(8.0);
            }
            let (row, new) =
                ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
            if new.hovered() {
                ui.painter().rect_filled(row, 8, palette.surface_hover);
            }
            if new.has_focus() {
                icons::paint_focus(ui, &palette, row, Tier::Row);
            }
            icons::draw_icon(
                ui.painter(),
                Rect::from_center_size(pos2(row.left() + 19.0, row.center().y), vec2(14.0, 14.0)),
                Icon::Plus,
                palette.text_muted,
            );
            let key = if cfg!(target_os = "macos") {
                "⌘T"
            } else {
                "Ctrl+T"
            };
            ui.painter().text(
                pos2(row.right() - 9.0, row.center().y),
                Align2::RIGHT_CENTER,
                key,
                FontId::proportional(11.0),
                palette.text_muted,
            );
            let mut title = egui::text::LayoutJob::simple_singleline(
                tr("common-new-tab").to_owned(),
                FontId::proportional(13.0),
                palette.text,
            );
            title.wrap = egui::text::TextWrapping::truncate_at_width((row.width() - 93.0).max(0.0));
            let title = ui.painter().layout_job(title);
            ui.painter().galley(
                pos2(row.left() + 34.0, row.center().y - title.size().y / 2.0),
                title,
                palette.text,
            );
            new.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    tr("sidebar-shortcut-new-tab"),
                )
            });
            if new.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                actions.push(UiAction::OpenCommand(CommandMode::NewTab));
            }
            if dragging.is_some() && pointer.is_some_and(|pos| row.contains(pos)) {
                target = Some((space.tabs.len(), None));
            }
            if let (Some(tab), Some(pos)) = (ghost, pointer) {
                let viewport = ui.clip_rect();
                if viewport.x_range().contains(pos.x) {
                    if pos.y < viewport.top() + 24.0 {
                        ui.scroll_with_delta(vec2(0.0, 6.0));
                    } else if pos.y > viewport.bottom() - 24.0 {
                        ui.scroll_with_delta(vec2(0.0, -6.0));
                    }
                }
                paint_tab_ghost(
                    ui.painter(),
                    Rect::from_min_size(
                        pos2(viewport.left() + 4.0, pos.y - 15.0),
                        vec2(viewport.width() - 8.0, 30.0),
                    ),
                    &tab.title,
                    chrome.favicons.get(&tab.id),
                    &palette,
                );
                ctx.request_repaint();
            }
            if dragging.is_some() && (released || !ctx.input(|input| input.pointer.any_down())) {
                ctx.data_mut(|data| data.remove::<TabId>(drag_id));
            }
            if released && let (Some(tab), Some((index, onto))) = (dragging, target) {
                if let Some(onto) = onto.filter(|onto| *onto != tab) {
                    actions.push(UiAction::GroupTabs { onto, dragged: tab });
                } else {
                    actions.push(UiAction::MoveTab {
                        tab,
                        workspace: active,
                        index,
                    });
                }
            }
        });
}
