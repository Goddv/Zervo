use super::*;
use crate::settings::{Layout, SidebarSection};

pub(super) fn draw(
    ui: &mut Ui,
    settings: &mut Settings,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    settings_section(ui, palette, tr("layout-customize"), |ui| {
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            for layout in Layout::ALL {
                if ui
                    .selectable_label(settings.layout == layout, layout.label())
                    .clicked()
                {
                    settings.layout = layout;
                    changed = true;
                }
            }
        });
        ui.add_space(14.0);
        ui.label(RichText::new(tr("layout-sidebar-sections")).strong());
        ui.small(tr("layout-reorder-sections"));
        ui.add_space(6.0);
        let mut reorder = None;
        for (index, section) in settings.sidebar_sections.into_iter().enumerate() {
            let (row, dropped) = ui.dnd_drop_zone::<SidebarSection, _>(
                Frame::NONE
                    .inner_margin(Margin::symmetric(6, 3))
                    .corner_radius(6),
                |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        let id = ui.id().with(("reorder_section", section));
                        ui.dnd_drag_source(id, section, |ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(24.0, 26.0), Sense::hover());
                            icons::draw_icon(
                                ui.painter(),
                                rect.shrink(5.0),
                                Icon::DragHandle,
                                palette.text_muted,
                            );
                        });
                        let mut shown = section.visible(settings);
                        if ui.checkbox(&mut shown, section.label()).changed() {
                            section.set_visible(settings, shown);
                            changed = true;
                        }
                        if move_button(ui, true, index > 0, section.label()).clicked() {
                            reorder = Some((index, index.saturating_sub(1)));
                        }
                        if move_button(ui, false, index < 2, section.label()).clicked() {
                            reorder = Some((index, index + 1));
                        }
                    });
                },
            );
            if let Some(source) = dropped
                && let Some(from) = settings
                    .sidebar_sections
                    .iter()
                    .position(|other| other == source.as_ref())
            {
                reorder = Some((from, index));
            }
            if row.response.contains_pointer()
                && egui::DragAndDrop::has_payload_of_type::<SidebarSection>(ui.ctx())
            {
                ui.painter().rect_stroke(
                    row.response.rect,
                    6,
                    Stroke::new(1.5_f32, palette.accent),
                    StrokeKind::Inside,
                );
            }
        }
        if let Some((from, to)) = reorder {
            let sections = &mut settings.sidebar_sections;
            if from < to {
                sections[from..=to].rotate_left(1);
            } else {
                sections[to..=from].rotate_right(1);
            }
            changed = true;
        }
        ui.add_space(8.0);
        changed |= ui
            .checkbox(&mut settings.show_sidebar_tools, tr("layout-sidebar-tools"))
            .changed();
        changed |= ui
            .checkbox(
                &mut settings.show_sidebar_widgets,
                tr("layout-widget-shelf"),
            )
            .changed();
        if ui.button(tr("layout-reset-sidebar")).clicked() {
            settings.reset_sidebar_sections();
            changed = true;
        }
        if changed {
            actions.push(UiAction::PersistSettings);
        }
    });

    settings_section(ui, palette, tr("layout-top-bar-tools"), |ui| {
        let left = &settings.navbar_left;
        let right = &settings.navbar_right;
        tool_group(
            ui,
            actions,
            NavSide::Left,
            left,
            right.len(),
            tr("layout-before-address"),
        );
        ui.add_space(8.0);
        tool_group(
            ui,
            actions,
            NavSide::Right,
            right,
            left.len(),
            tr("layout-after-address"),
        );
        ui.add_space(8.0);
        egui::ComboBox::from_id_salt("add_top_bar_tool")
            .selected_text(tr("layout-add-tool"))
            .show_ui(ui, |ui| {
                let mut found = false;
                for item in NavItem::ALL {
                    if left.contains(&item) || right.contains(&item) {
                        continue;
                    }
                    found = true;
                    if ui.button(item.label()).clicked() {
                        actions.push(UiAction::AddNavItem(item));
                        ui.close();
                    }
                }
                if !found {
                    ui.weak(tr("layout-all-tools-visible"));
                }
            });
        if ui.button(tr("layout-reset-top-bar")).clicked() {
            settings.navbar_left = NavItem::default_left();
            settings.navbar_right = NavItem::default_right();
            actions.push(UiAction::PersistSettings);
        }
    });
}

fn move_button(ui: &mut Ui, earlier: bool, enabled: bool, name: &str) -> egui::Response {
    let label = crate::l10n::message(
        if earlier {
            "layout-move-up"
        } else {
            "layout-move-down"
        },
        [("name", name.into())],
    );
    let response = ui.add_enabled(
        enabled,
        egui::Button::new(if earlier { "↑" } else { "↓" }).min_size(vec2(28.0, 27.0)),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &label));
    response.on_hover_text(label)
}

fn tool_group(
    ui: &mut Ui,
    actions: &mut Vec<UiAction>,
    side: NavSide,
    items: &[NavItem],
    other_count: usize,
    title: &str,
) {
    ui.label(RichText::new(title).strong());
    if items.is_empty() {
        ui.weak(tr("layout-empty-tools"));
    }
    let other = match side {
        NavSide::Left => NavSide::Right,
        NavSide::Right => NavSide::Left,
    };
    for (index, item) in items.iter().copied().enumerate() {
        ui.push_id(item, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(item.label());
                if move_button(ui, true, index > 0, item.label()).clicked() {
                    actions.push(UiAction::MoveNavItem {
                        item,
                        side,
                        index: index - 1,
                    });
                }
                if move_button(ui, false, index + 1 < items.len(), item.label()).clicked() {
                    actions.push(UiAction::MoveNavItem {
                        item,
                        side,
                        index: index + 1,
                    });
                }
                ui.menu_button(tr("layout-tool-options"), |ui| {
                    let placement = if side == NavSide::Left {
                        tr("layout-after-address")
                    } else {
                        tr("layout-before-address")
                    };
                    if ui.button(placement).clicked() {
                        actions.push(UiAction::MoveNavItem {
                            item,
                            side: other,
                            index: other_count,
                        });
                        ui.close();
                    }
                    if ui.button(tr("layout-hide-tool")).clicked() {
                        actions.push(UiAction::RemoveNavItem(item));
                        ui.close();
                    }
                });
            });
        });
    }
}
