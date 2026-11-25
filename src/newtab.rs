//! The browser's New Tab page and its search entry point.

use egui::{CursorIcon, FontId, Id, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::glass::{self, Glass};
use crate::icons::{self, Icon};
use crate::overlay_glass::{self, Role, Surface};
use crate::settings::NewTabBackground;
use crate::theme::{self, Palette, Tier, Translucency};
use crate::ui::{ChromeContext, UiAction};

pub fn draw(
    root: &mut Ui,
    chrome: &ChromeContext,
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
    let ground = if palette.translucency.is_translucent() && chrome.native_glass.vibrancy_available
    {
        let veil = if material == Translucency::LiquidGlass {
            0.38
        } else {
            0.66
        };
        theme::page_veil(&palette, veil)
    } else {
        theme::page_base(&palette)
    };
    let painter = root
        .ctx()
        .layer_painter(egui::LayerId::background())
        .with_clip_rect(page);
    painter.rect_filled(page, theme::content_corners(&palette), ground);
    if background == NewTabBackground::Grid {
        paint_grid(&painter, page, &palette);
    }
    if let Some(capture) = &chrome.capture {
        crate::backdrop::capture_into(&painter, page, capture);
    }

    if content.width() < 116.0
        || content.height() < 68.0
        || chrome
            .browser
            .command_bar
            .as_ref()
            .is_some_and(crate::ui::CommandBar::from_home)
    {
        return;
    }
    let bounds = Rect::from_center_size(
        content.center(),
        vec2((content.width() - 36.0).min(410.0), 52.0),
    );
    let hint = crate::l10n::message(
        "newtab-search-compact",
        [("engine", chrome.settings.search_engine.label().into())],
    );
    let response = root.interact(bounds, Id::unique("newtab_search"), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &hint));

    let surface = Surface::begin_if(root, material == Translucency::LiquidGlass);
    let radius = Role::SearchPill.radius();
    if !surface.is_native() {
        glass::paint(
            root.painter(),
            bounds,
            &palette,
            Glass::tier(Tier::Card)
                .radius_exact(radius)
                .border(palette.border)
                .opaque(palette.surface),
        );
    }
    if response.hovered() || response.is_pointer_button_down_on() {
        root.painter()
            .rect_filled(bounds, radius, palette.surface_hover.gamma_multiply(0.65));
    }
    if response.has_focus() {
        root.painter().rect_stroke(
            bounds,
            radius,
            Stroke::new(1.0, palette.accent),
            egui::StrokeKind::Inside,
        );
    }
    let icon = Rect::from_center_size(bounds.left_center() + vec2(24.0, 0.0), vec2(18.0, 18.0));
    icons::draw_icon(root.painter(), icon, Icon::Search, palette.text_muted);
    overlay_glass::row_label(
        root,
        bounds
            .with_min_x(bounds.left() + 43.0)
            .with_max_x(bounds.right() - 19.0),
        &hint,
        FontId::proportional(15.0),
        palette.text,
    );
    surface.finish(root, bounds, Role::SearchPill);
    if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        actions.push(UiAction::OpenCommandFrom(bounds));
    }
}

fn paint_grid(painter: &egui::Painter, page: Rect, palette: &Palette) {
    let step = 42.0;
    let stroke = Stroke::new(
        1.0,
        palette
            .accent
            .gamma_multiply(if palette.dark { 0.10 } else { 0.13 }),
    );
    let mut x = page.left() + step;
    while x < page.right() {
        painter.line_segment([pos2(x, page.top()), pos2(x, page.bottom())], stroke);
        x += step;
    }
    let mut y = page.top() + step;
    while y < page.bottom() {
        painter.line_segment([pos2(page.left(), y), pos2(page.right(), y)], stroke);
        y += step;
    }
}
