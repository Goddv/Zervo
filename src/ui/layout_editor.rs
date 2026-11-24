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
        let modes = Layout::ALL.map(Layout::label);
        let selected = usize::from(settings.layout == Layout::FullPage);
        if let Some(mode) = crate::widgets::segmented(ui, selected, &modes, palette) {
            settings.layout = Layout::ALL[mode];
            changed = true;
        }
        ui.add_space(14.0);
        ui.label(RichText::new(tr("layout-sidebar-sections")).strong());
        ui.small(tr("layout-reorder-sections"));
        ui.add_space(6.0);
        changed |= crate::widgets::toggle(
            ui,
            &mut settings.show_pinned_section,
            tr("sidebar-pinned-tabs"),
            palette,
        );
        let mut reorder = None;
        let sections = settings
            .sidebar_sections
            .into_iter()
            .enumerate()
            .filter(|(_, section)| *section != SidebarSection::Pinned)
            .collect::<Vec<_>>();
        for (position, &(index, section)) in sections.iter().enumerate() {
            let (row, dropped) = ui.dnd_drop_zone::<SidebarSection, _>(
                Frame::NONE
                    .inner_margin(Margin::symmetric(6, 3))
                    .corner_radius(6),
                |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        let id = ui.scope_id().with(("reorder_section", section));
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
                        if move_button(ui, palette, true, position > 0, section.label()).clicked() {
                            reorder = Some((index, sections[position - 1].0));
                        }
                        if move_button(
                            ui,
                            palette,
                            false,
                            position + 1 < sections.len(),
                            section.label(),
                        )
                        .clicked()
                        {
                            reorder = Some((index, sections[position + 1].0));
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
            settings.sidebar_sections.swap(from, to);
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
            actions.push(UiAction::SettingsChanged);
        }
    });
}

fn move_button(
    ui: &mut Ui,
    palette: &Palette,
    earlier: bool,
    enabled: bool,
    name: &str,
) -> egui::Response {
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
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &label));
    crate::overlay_glass::tooltip(response, palette, label)
}
