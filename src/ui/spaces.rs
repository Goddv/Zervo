use crate::state::{FolderId, Tab, TabFolder};

use super::*;

fn header(ui: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    let active = chrome.browser.active_workspace;
    let browser = &mut chrome.browser;
    let palette = &chrome.palette;
    let (_, row) = ui.allocate_space(vec2(ui.available_width(), 34.0));
    let dot = pos2(row.left() + 11.0, row.center().y);
    ui.painter().circle_filled(
        dot,
        4.0,
        theme::workspace_color_from(active, chrome.settings.space_colour),
    );
    let mut name_rect = row;
    name_rect.min.x += 22.0;
    let editor_id = ui.scope_id().with(("workspace_name", active));
    if let Some((_, name)) = browser
        .workspace_edit
        .as_mut()
        .filter(|(index, _)| *index == active)
    {
        if edit_name(ui, name_rect, editor_id, name, palette, |name| {
            actions.push(UiAction::RenameWorkspace(active, name));
        }) {
            browser.workspace_edit = None;
        }
        return;
    }

    let space = &browser.workspaces[active];
    let mut tools = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(row)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    tools.spacing_mut().item_spacing.x = 1.0;
    let add = button(&mut tools, Icon::Plus, tr("sidebar-create"), palette);
    crate::overlay_glass::popup(egui::Popup::menu(&add), ui.ctx(), |ui| {
        if ui.button(tr("sidebar-new-folder")).clicked() {
            actions.push(UiAction::NewFolder(None));
            ui.close();
        }
        if ui.button(tr("chrome-new-workspace")).clicked() {
            actions.push(UiAction::NewWorkspace);
            ui.close();
        }
    });
    if let (Some(next), Some(previous)) = (
        browser.adjacent_workspace(true),
        browser.adjacent_workspace(false),
    ) {
        if button(
            &mut tools,
            Icon::Forward,
            tr("gesture-next-workspace"),
            palette,
        )
        .clicked()
        {
            actions.push(UiAction::SelectWorkspace(next));
        }
        if button(
            &mut tools,
            Icon::Back,
            tr("gesture-previous-workspace"),
            palette,
        )
        .clicked()
        {
            actions.push(UiAction::SelectWorkspace(previous));
        }
    }
    name_rect.max.x = tools.min_rect().left() - 4.0;
    let name = &space.name;
    let response = ui.interact(name_rect, editor_id.with("heading"), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    let mut label_rect = name_rect;
    if chrome.settings.show_tab_counts {
        let count = ui.painter().text(
            pos2(label_rect.right() - 2.0, label_rect.center().y),
            Align2::RIGHT_CENTER,
            space.tabs.len().to_string(),
            FontId::proportional(11.0),
            palette.text_muted,
        );
        label_rect.max.x = count.left() - 6.0;
    }
    crate::overlay_glass::row_label(
        ui,
        label_rect,
        name,
        FontId::proportional(14.0),
        palette.text,
    );
    if response.has_focus() {
        icons::paint_focus(ui, palette, name_rect, Tier::Control);
    }
    let mut rename = response.double_clicked()
        || (response.has_focus()
            && ui.input(|input| input.key_pressed(Key::Enter) || input.key_pressed(Key::Space)));
    crate::overlay_glass::context_menu(&response, |ui| {
        if ui.button(tr("common-rename")).clicked() {
            rename = true;
            ui.close();
        }
        if browser.can_remove_workspace(active) && ui.button(tr("sidebar-delete-space")).clicked() {
            actions.push(UiAction::RemoveWorkspace(active));
            ui.close();
        }
    });
    if rename {
        browser.folder_edit = None;
        browser.workspace_edit = Some((active, name.clone()));
    }
    response.on_hover_text(name);
}

fn button(ui: &mut Ui, icon: Icon, label: &str, palette: &Palette) -> egui::Response {
    let response = icons::icon_button(ui, icon, 13.0, palette, true);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    response.on_hover_text(label)
}

fn edit_name(
    ui: &mut Ui,
    row: Rect,
    id: Id,
    name: &mut String,
    palette: &Palette,
    keep: impl FnOnce(String),
) -> bool {
    let mut field = row.shrink2(vec2(0.0, 2.0));
    field.max.x -= 45.0;
    let editor = ui.put(
        field,
        TextEdit::singleline(name)
            .id(id)
            .frame(Frame::NONE)
            .desired_width(field.width())
            .hint_text(tr("common-rename"))
            .font(FontId::proportional(13.0))
            .text_color(palette.text),
    );
    if !editor.has_focus() && !editor.lost_focus() {
        editor.request_focus();
        if let Some(mut state) = TextEdit::load_state(ui.ctx(), id) {
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(name.chars().count()),
                )));
            state.store(ui.ctx(), id);
        }
    }

    let controls_area = Rect::from_min_max(pos2(field.right(), row.top()), row.max);
    let mut controls = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(controls_area)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    controls.spacing_mut().item_spacing.x = 0.0;
    let cancel = button(&mut controls, Icon::Close, tr("common-cancel"), palette).clicked();
    let save = button(&mut controls, Icon::Check, tr("common-save"), palette).clicked();
    if cancel || ui.input(|input| input.key_pressed(Key::Escape)) {
        return true;
    }
    if save || ui.input(|input| input.key_pressed(Key::Enter)) {
        keep(std::mem::take(name));
        return true;
    }
    false
}

enum FolderEdit {
    Unchanged,
    Rename,
    Done,
}

fn folder_row(
    ui: &mut Ui,
    folder: &TabFolder,
    editing: Option<&mut (FolderId, String)>,
    selected_inside: bool,
    drag_at: Option<egui::Pos2>,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) -> (bool, FolderEdit) {
    let height = 32.0;
    let (_, row) = ui.allocate_space(vec2(ui.available_width(), height));
    let id = ui.scope_id().with(("tab_folder", folder.id));
    let dragged_over = drag_at.is_some_and(|point| row.intersect(ui.clip_rect()).contains(point));
    let editing = editing.filter(|(id, _)| *id == folder.id);
    let response = ui.interact(
        row,
        id,
        if editing.is_some() {
            Sense::hover()
        } else {
            Sense::click()
        },
    );
    if dragged_over || response.hovered() || selected_inside {
        ui.painter()
            .rect_filled(row, palette.corner(Tier::Row), palette.surface_hover);
    }
    if dragged_over {
        ui.painter().rect_stroke(
            row,
            palette.corner(Tier::Row),
            Stroke::new(1.0_f32, palette.accent),
            StrokeKind::Inside,
        );
    }
    icons::draw_icon(
        ui.painter(),
        Rect::from_center_size(pos2(row.left() + 18.0, row.center().y), vec2(17.0, 17.0)),
        if folder.collapsed {
            Icon::Folder
        } else {
            Icon::FolderOpen
        },
        palette.accent,
    );
    let mut text_rect = row;
    text_rect.min.x += 33.0;
    if let Some((_, draft)) = editing {
        let done = edit_name(ui, text_rect, id.with("name"), draft, palette, |name| {
            actions.push(UiAction::RenameFolder(folder.id, name));
        });
        return (
            dragged_over,
            if done {
                FolderEdit::Done
            } else {
                FolderEdit::Unchanged
            },
        );
    }

    text_rect.max.x -= 27.0;
    crate::overlay_glass::row_label(
        ui,
        text_rect,
        &folder.name,
        FontId::proportional(13.0),
        palette.text,
    );
    icons::draw_icon(
        ui.painter(),
        Rect::from_center_size(pos2(row.right() - 16.0, row.center().y), vec2(12.0, 12.0)),
        if folder.collapsed {
            Icon::CaretRight
        } else {
            Icon::CaretDown
        },
        palette.text_muted,
    );
    if response.has_focus() {
        icons::paint_focus(ui, palette, row, Tier::Row);
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            crate::l10n::message(
                if folder.collapsed {
                    "sidebar-expand-folder"
                } else {
                    "sidebar-collapse-folder"
                },
                [("name", folder.name.as_str().into())],
            ),
        )
    });
    let mut rename = false;
    crate::overlay_glass::context_menu(&response, |ui| {
        if ui.button(tr("common-rename")).clicked() {
            rename = true;
            ui.close();
        }
        if ui.button(tr("sidebar-remove-folder")).clicked() {
            actions.push(UiAction::RemoveFolder(folder.id));
            ui.close();
        }
    });
    if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        actions.push(UiAction::ToggleFolder(folder.id));
    }
    (
        dragged_over,
        if rename {
            FolderEdit::Rename
        } else {
            FolderEdit::Unchanged
        },
    )
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
                        egui::Role::ListBoxOption,
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

pub(super) fn body(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    native_overlay: bool,
) {
    let active = chrome.browser.active_workspace;
    let palette = chrome.palette;
    header(ui, chrome, actions);
    ui.add_space(7.0);
    let space = &chrome.browser.workspaces[active];
    let mut folder_edit = chrome.browser.folder_edit.take();
    let ctx = ui.ctx().clone();
    let drag_id = Id::unique("zervo_tab_drag");
    let dragging = ctx.data(|data| data.get_temp::<TabId>(drag_id));
    let pointer = ctx.input(|input| input.pointer.latest_pos());
    let drag_at = dragging.and(pointer);
    let mut folder_tabs = HashMap::<_, Vec<_>>::new();
    for (index, tab) in space.tabs.iter().enumerate() {
        if let Some(folder) = tab.folder {
            folder_tabs.entry(folder).or_default().push((index, tab));
        }
    }

    let draw_tab = |ui: &mut Ui, index, tab: &Tab, actions: &mut Vec<UiAction>| {
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
                folder: tab.folder,
                spaces: &chrome.browser.workspaces,
                glyph: internal_glyph(tab.kind),
                always_show_close: chrome.settings.always_show_tab_close,
                compact: chrome.settings.compact_sidebar,
                dragging,
                native_overlay,
            },
            &palette,
            chrome.favicons.get(&tab.id),
            actions,
        );
        if out.drag_started {
            ctx.data_mut(|data| data.insert_temp(drag_id, tab.id));
        }
        if out.close_clicked {
            actions.push(UiAction::CloseTab(tab.id));
        } else if out.clicked {
            actions.push(UiAction::SelectTab(tab.id));
        }
        out.drop_at.map(|drop| match drop {
            DropAt::Before => TabDrop::Insert(index, tab.folder),
            DropAt::After => TabDrop::Insert(index + 1, tab.folder),
            DropAt::Onto => TabDrop::GroupWith(tab.id),
        })
    };

    egui::ScrollArea::vertical()
        .id_salt(("space_tabs", active))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            let mut target = None;
            for folder in &space.folders {
                let tabs = folder_tabs.get(&folder.id).map_or(&[][..], Vec::as_slice);
                let selected_inside = folder.collapsed
                    && tabs
                        .iter()
                        .any(|(_, tab)| chrome.browser.active_tab == Some(tab.id));
                let (over, edit) = folder_row(
                    ui,
                    folder,
                    folder_edit.as_mut(),
                    selected_inside,
                    drag_at,
                    &palette,
                    actions,
                );
                match edit {
                    FolderEdit::Unchanged => {},
                    FolderEdit::Rename => folder_edit = Some((folder.id, folder.name.clone())),
                    FolderEdit::Done => folder_edit = None,
                }
                if over {
                    target = Some(TabDrop::Insert(space.tabs.len(), Some(folder.id)));
                }
                if !folder.collapsed {
                    for &(index, tab) in tabs {
                        target = draw_tab(ui, index, tab, actions).or(target);
                    }
                }
            }
            if !space.folders.is_empty() {
                ui.add_space(5.0);
                ui.separator();
                ui.add_space(5.0);
            }
            for section in chrome.settings.sidebar_sections {
                use crate::settings::SidebarSection;
                if !section.visible(chrome.settings) {
                    continue;
                }
                if section == SidebarSection::Favourites {
                    favourites(ui, chrome, actions);
                    continue;
                }
                let tabs = space.tabs.iter().enumerate().filter(|(_, tab)| {
                    tab.folder.is_none()
                        && match section {
                            SidebarSection::Pinned => tab.pinned,
                            SidebarSection::Tabs => {
                                !tab.pinned || !chrome.settings.show_pinned_section
                            },
                            SidebarSection::Favourites => false,
                        }
                });
                if tabs.clone().next().is_some() {
                    heading(ui, section.label(), &palette);
                }
                for (index, tab) in tabs {
                    target = draw_tab(ui, index, tab, actions).or(target);
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
                egui::WidgetInfo::labeled(egui::Role::Button, true, tr("sidebar-shortcut-new-tab"))
            });
            if new.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                actions.push(UiAction::OpenCommand(CommandMode::NewTab));
            }
            if drag_at.is_some_and(|pos| row.intersect(ui.clip_rect()).contains(pos)) {
                target = Some(TabDrop::Insert(space.tabs.len(), None));
            }
            if let (Some(tab), Some(pos)) = (
                dragging.and_then(|id| space.tabs.iter().find(|tab| tab.id == id)),
                pointer,
            ) {
                let viewport = ui.clip_rect();
                if viewport.contains(pos) {
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
            if let Some(tab) = dragging
                && !ctx.input(|input| input.pointer.primary_down())
            {
                ctx.data_mut(|data| data.remove::<TabId>(drag_id));
                if ctx.input(|input| input.pointer.primary_released()) {
                    match target {
                        Some(TabDrop::GroupWith(onto)) => {
                            actions.push(UiAction::GroupTabs { onto, dragged: tab });
                        },
                        Some(TabDrop::Insert(index, folder)) => actions.push(UiAction::MoveTab {
                            tab,
                            workspace: active,
                            index,
                            folder,
                        }),
                        None => {},
                    }
                }
            }
        });
    chrome.browser.folder_edit = folder_edit;
}

enum TabDrop {
    Insert(usize, Option<FolderId>),
    GroupWith(TabId),
}
