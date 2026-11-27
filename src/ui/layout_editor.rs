use super::*;
use crate::settings::Layout;

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
        ui.add_space(6.0);
        for (visible, label) in [
            (&mut settings.show_essentials, "common-favourites"),
            (&mut settings.show_pinned_section, "sidebar-pinned-tabs"),
            (&mut settings.show_open_tabs, "sidebar-open-tabs"),
            (&mut settings.show_sidebar_tools, "layout-sidebar-tools"),
            (&mut settings.show_sidebar_widgets, "layout-widget-shelf"),
        ] {
            changed |= crate::widgets::toggle(ui, visible, tr(label), palette);
        }
        if ui.button(tr("layout-reset-sidebar")).clicked() {
            settings.reset_sidebar_sections();
            changed = true;
        }
        if changed {
            actions.push(UiAction::SettingsChanged);
        }
    });
}
