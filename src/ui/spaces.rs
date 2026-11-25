use crate::state::{FolderId, Tab, TabFolder};

use super::*;

#[derive(Clone, Copy)]
struct WorkspaceSwitch {
    started: std::time::Instant,
    forward: bool,
}

pub(crate) fn animate_workspace_switch(ctx: &egui::Context, forward: bool) {
    let motion = WorkspaceSwitch {
        started: std::time::Instant::now(),
        forward,
    };
    ctx.data_mut(|data| data.insert_temp(Id::unique("workspace_switch"), motion));
}

fn workspace_motion(ui: &Ui, palette: &Palette) -> (f32, f32) {
    let id = Id::unique("workspace_switch");
    let ctx = ui.ctx();
    let Some(motion) = ctx.data(|data| data.get_temp::<WorkspaceSwitch>(id)) else {
        return (0.0, 1.0);
    };
    let duration = palette.material.animation * 1.35;
    let elapsed = motion.started.elapsed().as_secs_f32();
    if elapsed >= duration {
        ctx.data_mut(|data| data.remove::<WorkspaceSwitch>(id));
        return (0.0, 1.0);
    }
    ui.request_repaint();
    let t = glass::ease_out(elapsed / duration);
    let direction = if motion.forward { 1.0 } else { -1.0 };
    (direction * 28.0 * (1.0 - t), 0.55 + 0.45 * t)
}

fn folder_intro_id(id: FolderId) -> Id {
    Id::unique(("folder_intro", id))
}

pub(crate) fn animate_folder_creation(ctx: &egui::Context, id: FolderId) {
    ctx.animate_bool_with_time(folder_intro_id(id), false, 0.0);
}

fn header(ui: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    let active = chrome.browser.active_workspace;
    let browser = &mut chrome.browser;
    let palette = &chrome.palette;
    let (_, row) = ui.allocate_space(vec2(ui.available_width(), 34.0));
    let editing = browser
        .workspace_edit
        .as_mut()
        .filter(|(index, _)| *index == active);
    let material = editing.as_ref().map(|_| {
        let surface = crate::overlay_glass::Surface::begin(ui);
        paint_tab_selection(ui, row, palette, 1.0);
        surface
    });
    let dot = pos2(row.left() + 11.0, row.center().y);
    ui.painter().circle_filled(
        dot,
        4.0,
        theme::workspace_color_from(active, chrome.settings.space_colour),
    );
    let mut name_rect = row;
    name_rect.min.x += 22.0;
    let editor_id = ui.scope_id().with(("workspace_name", active));
    if let (Some((_, name)), Some(material)) = (editing, material) {
        if edit_name(
            ui,
            row,
            (name_rect.left(), 14.0),
            editor_id,
            name,
            palette,
            |name| {
                actions.push(UiAction::RenameWorkspace(active, name));
            },
        ) {
            browser.workspace_edit = None;
        }
        material.finish(ui, row, crate::overlay_glass::Role::Tab);
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
    if browser.workspaces.len() > 1 {
        if button(
            &mut tools,
            Icon::Forward,
            tr("gesture-next-workspace"),
            palette,
        )
        .clicked()
        {
            actions.push(UiAction::CycleWorkspace { forward: true });
        }
        if button(
            &mut tools,
            Icon::Back,
            tr("gesture-previous-workspace"),
            palette,
        )
        .clicked()
        {
            actions.push(UiAction::CycleWorkspace { forward: false });
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
    icons::labelled_button(ui, icon, 13.0, palette, true, label)
}

fn edit_name(
    ui: &mut Ui,
    row: Rect,
    (left, font_size): (f32, f32),
    id: Id,
    name: &mut String,
    palette: &Palette,
    keep: impl FnOnce(String),
) -> bool {
    let field = row.with_min_x(left).with_max_x(row.right() - 53.0);
    let editor = ui.put(
        field,
        TextEdit::singleline(name)
            .id(id)
            .frame(Frame::NONE)
            .margin(Margin::ZERO)
            .align(Align2::LEFT_CENTER)
            .min_size(field.size())
            .desired_width(field.width())
            .hint_text(tr("common-rename"))
            .font(FontId::proportional(font_size))
            .text_color(row_ink(palette, 1.0).0),
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
    let intro = 1.0
        - glass::ease_out(ui.ctx().animate_bool_with_time(
            folder_intro_id(folder.id),
            true,
            palette.material.animation * 1.9,
        ));
    let editing = editing.filter(|(id, _)| *id == folder.id);
    let material = editing.as_ref().map(|_| {
        let surface = crate::overlay_glass::Surface::begin(ui);
        paint_tab_selection(ui, row, palette, 1.0);
        surface
    });
    let response = ui.interact(
        row,
        id,
        if editing.is_some() {
            Sense::hover()
        } else {
            Sense::click()
        },
    );
    let ink = if editing.is_some() {
        row_ink(palette, 1.0).0
    } else {
        if dragged_over || response.hovered() || selected_inside {
            ui.painter()
                .rect_filled(row, palette.corner(Tier::Row), palette.surface_hover);
        }
        palette.text
    };
    if intro > 0.0 {
        ui.painter().rect_filled(
            row,
            palette.corner(Tier::Row),
            palette.accent.gamma_multiply(0.18 * intro),
        );
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
        Rect::from_center_size(
            pos2(row.left() + 18.0 - 5.0 * intro, row.center().y),
            vec2(17.0, 17.0),
        ),
        if folder.collapsed {
            Icon::Folder
        } else {
            Icon::FolderOpen
        },
        if palette.dark { ink } else { palette.accent },
    );
    let mut text_rect = row;
    text_rect.min.x += 33.0;
    if let (Some((_, draft)), Some(material)) = (editing, material) {
        let done = edit_name(
            ui,
            row,
            (text_rect.left(), 13.0),
            id.with("name"),
            draft,
            palette,
            |name| {
                actions.push(UiAction::RenameFolder(folder.id, name));
            },
        );
        material.finish(ui, row, crate::overlay_glass::Role::Tab);
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
    crate::overlay_glass::row_label(ui, text_rect, &folder.name, FontId::proportional(13.0), ink);
    icons::draw_icon(
        ui.painter(),
        Rect::from_center_size(pos2(row.right() - 16.0, row.center().y), vec2(12.0, 12.0)),
        if folder.collapsed {
            Icon::CaretRight
        } else {
            Icon::CaretDown
        },
        ink,
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

fn pinned_tile(
    ui: &mut Ui,
    style: TabRowStyle<'_>,
    palette: &Palette,
    favicon: Option<&TextureHandle>,
    actions: &mut Vec<UiAction>,
) -> TabRowOut {
    let (rect, response) = ui.allocate_exact_size(vec2(42.0, 42.0), Sense::click_and_drag());
    let material = (style.native_overlay && rect.intersect(ui.clip_rect()).is_positive())
        .then(|| crate::overlay_glass::Surface::begin(ui));
    let (ink, _) = row_ink(palette, if style.selected { 1.0 } else { 0.0 });
    if style.selected {
        paint_tab_selection(ui, rect, palette, 1.0);
    } else if response.hovered() || material.is_none() {
        let fill = if response.hovered() {
            palette.surface_hover
        } else {
            palette.surface
        };
        ui.painter()
            .rect_filled(rect, palette.corner(Tier::Row), fill);
    }
    if response.has_focus() {
        icons::paint_focus(ui, palette, rect, Tier::Row);
    }
    response.clone().on_hover_cursor(CursorIcon::PointingHand);
    if let Some(favicon) = favicon {
        ui.painter().image(
            favicon.id(),
            Rect::from_center_size(rect.center(), vec2(20.0, 20.0)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            initial(style.title, style.url),
            FontId::proportional(15.0),
            ink,
        );
    }
    if let Some(material) = material {
        material.finish(ui, rect, crate::overlay_glass::Role::Tab);
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Tab, true, style.selected, style.title)
    });
    tab_context_menu(&response, &style, actions);
    let drop_at = style
        .dragging
        .filter(|tab| *tab != style.tab_id)
        .and_then(|_| {
            ui.input(|input| input.pointer.latest_pos())
                .filter(|pos| rect.intersect(ui.clip_rect()).contains(*pos))
                .map(|pos| {
                    if pos.x < rect.center().x {
                        DropAt::Before
                    } else {
                        DropAt::After
                    }
                })
        });
    if let Some(edge) = drop_at {
        let x = if matches!(edge, DropAt::Before) {
            rect.left()
        } else {
            rect.right()
        };
        ui.painter().vline(
            x,
            rect.y_range().shrink(5.0),
            Stroke::new(2.0, palette.accent),
        );
    }
    let out = TabRowOut {
        clicked: response.clicked(),
        close_clicked: false,
        drag_started: response.drag_started(),
        drop_at,
    };
    if style.dragging.is_none() {
        crate::overlay_glass::tooltip(response, palette, style.title);
    }
    out
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
                let icon_rect = Rect::from_center_size(rect.center(), vec2(18.0, 18.0));
                if let Some(favicon) = open
                    .filter(|_| chrome.settings.show_tab_favicons)
                    .and_then(|tab| chrome.favicons.get(&tab.id))
                {
                    ui.painter().image(
                        favicon.id(),
                        icon_rect,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else if !chrome.settings.show_tab_favicons
                    || !paint_site_icon(ui, chrome.saved_favicons, &site.url, icon_rect)
                {
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
        if let Some(folder) = tab.folder.filter(|_| !tab.pinned) {
            folder_tabs.entry(folder).or_default().push((index, tab));
        }
    }

    let draw_tab = |ui: &mut Ui, index, tab: &Tab, actions: &mut Vec<UiAction>| {
        let style = TabRowStyle {
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
        };
        let favicon = chrome
            .favicons
            .get(&tab.id)
            .filter(|_| chrome.settings.show_tab_favicons);
        let out = if tab.pinned {
            pinned_tile(ui, style, &palette, favicon, actions)
        } else {
            tab_row(ui, style, &palette, favicon, actions)
        };
        if out.drag_started {
            ctx.data_mut(|data| data.insert_temp(drag_id, tab.id));
        }
        if out.close_clicked {
            actions.push(UiAction::CloseTab(tab.id));
        } else if out.clicked {
            actions.push(UiAction::SelectTab(tab.id));
        }
        out.drop_at.map(|drop| match drop {
            DropAt::Before => TabDrop::Insert(index, tab.folder.filter(|_| !tab.pinned)),
            DropAt::After => TabDrop::Insert(index + 1, tab.folder.filter(|_| !tab.pinned)),
            DropAt::Onto => TabDrop::GroupWith(tab.id),
        })
    };

    // The address, Space controls and widget shelf stay anchored while the
    // incoming list slides in. Move its hit targets with its ink.
    let (shift, opacity) = workspace_motion(ui, &palette);
    let viewport = ui.available_rect_before_wrap();
    let mut list = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("workspace_list")
            .max_rect(viewport.translate(vec2(shift, 0.0))),
    );
    list.shrink_clip_rect(viewport);
    list.multiply_opacity(opacity);
    egui::ScrollArea::vertical()
        .id_salt(("space_tabs", active))
        .auto_shrink([false, false])
        .show(&mut list, |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            let mut target = None;
            let mut pins = space
                .tabs
                .iter()
                .enumerate()
                .filter(|(_, tab)| tab.pinned)
                .peekable();
            if chrome.settings.show_pinned_section && pins.peek().is_some() {
                heading(ui, tr("sidebar-pinned-tabs"), &palette);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    for (index, tab) in pins {
                        target = draw_tab(ui, index, tab, actions).or(target.take());
                    }
                });
                ui.add_space(8.0);
            }
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
                if tabs.is_empty() {
                    continue;
                }
                let collapse_id = ui.scope_id().with(("folder_contents", folder.id));
                let mut collapse = egui::collapsing_header::CollapsingState::load_with_default_open(
                    &ctx,
                    collapse_id,
                    !folder.collapsed,
                );
                collapse.set_open(!folder.collapsed);
                if palette.material.animation <= 0.0 {
                    ctx.animate_bool_with_time(collapse_id, !folder.collapsed, 0.0);
                }
                collapse.show_body_unindented(ui, |ui| {
                    for &(index, tab) in tabs {
                        if let Some(drop) = draw_tab(ui, index, tab, actions) {
                            target = Some(drop);
                        }
                    }
                });
            }
            if !space.folders.is_empty() {
                ui.add_space(5.0);
                ui.separator();
                ui.add_space(5.0);
            }
            for section in chrome.settings.sidebar_sections {
                use crate::settings::SidebarSection;
                if section == SidebarSection::Pinned || !section.visible(chrome.settings) {
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
                    .filter(|(_, tab)| tab.folder.is_none() && !tab.pinned);
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
                ui.painter()
                    .rect_filled(row, palette.corner(Tier::Row), palette.surface_hover);
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
