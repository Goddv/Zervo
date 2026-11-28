//! Native New Tab dashboard. The browser owns search, and the profile owns
//! the cards' data so the page needs no network requests of its own.

use std::time::Duration;

use egui::{
    Color32, CursorIcon, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2,
    vec2,
};

use crate::glass::{self, Glass};
use crate::grid::{Cell, Metrics};
use crate::icons::{self, Icon};
use crate::l10n::text as tr;
use crate::newtab_custom::{self as custom, Block, CustomWidget};
use crate::newtab_widgets::{self as widgets, Kind, Widget};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::overlay_glass::ComboBoxGlassExt as _;
use crate::overlay_glass::{self, Role, Surface};
use crate::settings::{NewTabBackground, SearchEngine};
use crate::theme::{self, Palette, Tier, Translucency};
use crate::ui::{ChromeContext, UiAction};

const GAP: f32 = 16.0;

fn editing_id() -> Id {
    Id::unique("new_tab_edit_widgets")
}

fn reveal_id() -> Id {
    Id::unique("new_tab_reveal_widget")
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub fn reveal_created(ctx: &egui::Context, id: u64) {
    ctx.data_mut(|data| {
        data.remove::<bool>(editing_id());
        data.insert_temp(reveal_id(), id);
    });
}

pub fn draw(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    page: Rect,
    content: Rect,
    actions: &mut Vec<UiAction>,
) {
    let background = if chrome.settings.new_tab_background == NewTabBackground::CustomImage
        && chrome.wallpaper.texture.is_none()
    {
        NewTabBackground::Grid
    } else {
        chrome.settings.new_tab_background
    };
    let has_wallpaper = background.animated() || background == NewTabBackground::CustomImage;
    let material = match background.material() {
        _ if chrome.native_glass.reduce_transparency => Translucency::Solid,
        Translucency::LiquidGlass if !chrome.native_glass.window_backdrop => Translucency::Frosted,
        material => material,
    };
    let mut palette = chrome.palette.with_translucency(material);
    palette.material.animation = chrome.palette.material.animation;
    backdrop(root, chrome, page, content, &palette, background);
    if content.width() < 140.0 || content.height() < 80.0 {
        return;
    }

    let viewport = content.shrink2(vec2(0.0, 12.0));
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let can_create = content.height() >= 250.0
        && content.width() >= 190.0
        && (chrome.widget_composer.tab.is_some() || crate::macos_sheet::widget_available());
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let viewport = if can_create {
        viewport.with_max_y(viewport.bottom() - 65.0)
    } else {
        viewport
    };
    let width = dashboard_width(viewport.width());
    let area = Rect::from_center_size(viewport.center(), vec2(width, viewport.height()));
    let mut dashboard = root.new_child(UiBuilder::new().id_salt("newtab_dashboard").max_rect(area));
    // Keep the gutters available for paint. The column controls layout, while
    // the wider clip allows the outer cards' shadows to fade out into the page.
    dashboard.set_clip_rect(root.clip_rect().intersect(viewport));
    egui::ScrollArea::vertical()
        .id_salt("newtab_scroll")
        .max_height(area.height())
        .auto_shrink([false, false])
        .show(&mut dashboard, |ui| {
            let width = ui.available_width();
            ui.add_space(chrome.window_controls_room());
            header(ui, chrome, &palette, width, has_wallpaper, actions);
            ui.add_space((area.height() * 0.10).clamp(20.0, 76.0));
            search(ui, chrome, &palette, width, actions);
            if chrome.settings.new_tab_show_widgets {
                ui.add_space(22.0);
                cards(ui, chrome, &palette, width, actions);
                // Leave space for the final row's shadow at the bottom of the
                // scroll range.
                let shadow_reach = palette.material.shadow_reach
                    + f32::from(Role::DashboardCard.radius())
                        * palette.material.shadow_reach_per_radius;
                ui.add_space(shadow_reach.ceil().max(10.0));
            }
        });
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    if can_create {
        draw_widget_composer(root, chrome.widget_composer, content, &palette, actions);
    }
}

fn dashboard_width(width: f32) -> f32 {
    (width - 2.0 * (width * 0.045).clamp(18.0, 48.0)).min(1040.0)
}

fn columns_for_width(width: f32) -> u8 {
    if width >= 770.0 {
        widgets::COLUMNS
    } else if width >= 490.0 {
        8
    } else {
        ((width + GAP) / 76.0).floor().clamp(2.0, 6.0) as u8
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn draw_widget_composer(
    ui: &mut Ui,
    composer: &mut crate::widget_composer::Composer,
    content: Rect,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    use crate::widget_composer::{Dictation, PROMPT_LIMIT};

    let open = composer.tab.is_some();
    let progress = ui.ctx().animate_bool_with_time(
        Id::unique("newtab_widget_composer_morph"),
        open,
        0.24 * palette.material.animation,
    );
    let max_width = (content.width() - 24.0).min(660.0);
    let width = egui::lerp(198.0_f32.min(max_width)..=max_width, progress);
    let height = egui::lerp(44.0..=54.0, progress);
    let rect = Rect::from_center_size(
        pos2(content.center().x, content.bottom() - 21.0 - height / 2.0),
        vec2(width, height),
    );
    let surface = glass_surface(ui, rect, palette, Role::SearchPill);
    if !open && progress < 0.35 {
        let left = rect.shrink(4.0).with_max_x(rect.center().x - 2.0);
        let right = rect.shrink(4.0).with_min_x(rect.center().x + 2.0);
        let speak = tr("newtab-composer-speak");
        let write = tr("newtab-composer-write");
        for (bounds, label, icon, speak) in [
            (left, speak, Icon::Microphone, true),
            (right, write, Icon::Pencil, false),
        ] {
            let target = button(
                ui,
                bounds,
                ("widget_composer_choice", speak),
                label,
                palette,
            )
            .on_hover_text(tr(if speak {
                "newtab-composer-speak-help"
            } else {
                "newtab-composer-write-help"
            }));
            if width > 166.0 {
                draw_icon(
                    ui,
                    bounds.left_center() + vec2(15.0, 0.0),
                    icon,
                    palette.accent,
                );
                centered_text(
                    ui,
                    bounds.with_min_x(bounds.left() + 25.0),
                    label,
                    13.0,
                    palette.text,
                );
            } else {
                draw_icon(ui, bounds.center(), icon, palette.text);
            }
            if target.clicked() {
                actions.push(UiAction::OpenWidgetComposer { speak });
            }
        }
        ui.painter().line_segment(
            [
                rect.center() - vec2(0.0, 10.0),
                rect.center() + vec2(0.0, 10.0),
            ],
            Stroke::new(1.0, palette.border),
        );
    } else if open && (progress > 0.68 || width == max_width) {
        let busy = composer.busy();
        let dismiss =
            Rect::from_center_size(rect.left_center() + vec2(24.0, 0.0), vec2(34.0, 34.0));
        let closing = button(
            ui,
            dismiss,
            "close_widget_composer",
            tr("common-close"),
            palette,
        )
        .on_hover_text(tr("common-close"))
        .clicked();
        draw_icon(ui, dismiss.center(), Icon::Close, palette.text_muted);
        if closing
            || ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            actions.push(UiAction::CloseWidgetComposer);
        }
        let submit_width = if width >= 370.0 { 105.0 } else { 36.0 };
        let submit = Rect::from_min_size(
            pos2(rect.right() - submit_width - 8.0, rect.center().y - 18.0),
            vec2(submit_width, 36.0),
        );
        let mic = Rect::from_center_size(submit.left_center() - vec2(20.0, 0.0), vec2(34.0, 34.0));
        let listening = composer.dictation.is_some();
        let voice_label = tr(if listening {
            "newtab-dictation-stop"
        } else {
            "newtab-composer-speak"
        });
        let voice_icon = if listening {
            Icon::Square
        } else {
            Icon::Microphone
        };
        let voice_enabled =
            !composer.generation && composer.dictation != Some(Dictation::Finishing);
        if voice_enabled
            && button(ui, mic, "widget_composer_mic", voice_label, palette)
                .on_hover_text(voice_label)
                .clicked()
        {
            actions.push(if listening {
                UiAction::StopWidgetDictation
            } else {
                UiAction::StartWidgetDictation
            });
        }
        draw_icon(
            ui,
            mic.center(),
            voice_icon,
            if listening {
                palette.accent
            } else {
                palette.text_muted
            },
        );
        let field = Rect::from_min_max(
            pos2(dismiss.right() + 1.0, rect.center().y - 18.0),
            pos2(
                (mic.left() - 3.0).max(dismiss.right() + 1.0),
                rect.center().y + 18.0,
            ),
        );
        if field.width() >= 25.0 {
            let enter = if listening {
                let shown = if composer.prompt.is_empty() {
                    tr("newtab-composer-placeholder")
                } else {
                    composer.prompt.as_str()
                };
                let font = egui::TextStyle::Body.resolve(ui.style());
                let galley = ui
                    .painter()
                    .layout_no_wrap(shown.to_owned(), font, palette.text);
                let origin = pos2(
                    field.left().min(field.right() - galley.size().x),
                    field.center().y - galley.size().y / 2.0,
                );
                ui.painter()
                    .with_clip_rect(field)
                    .galley(origin, galley, palette.text);
                false
            } else {
                let response = ui.put(
                    field,
                    egui::TextEdit::singleline(&mut composer.prompt)
                        .id(Id::unique("newtab_widget_prompt"))
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .align(egui::Align2::LEFT_CENTER)
                        .min_size(field.size())
                        .font(egui::TextStyle::Body)
                        .text_color(palette.text)
                        .hint_text(tr("newtab-composer-placeholder"))
                        .desired_width(field.width())
                        .char_limit(PROMPT_LIMIT)
                        .interactive(!busy),
                );
                if !busy && std::mem::take(&mut composer.focus_prompt) {
                    response.request_focus();
                }
                if response.changed() {
                    composer.preview = None;
                    composer.issue = None;
                }
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
            };
            let enabled = !busy && !composer.prompt.trim().is_empty();
            let label = tr(if composer.preview.is_some() {
                "newtab-composer-add"
            } else {
                "newtab-composer-generate"
            });
            let submit_action = if composer.preview.is_some() {
                UiAction::SaveCustomWidget
            } else {
                UiAction::GenerateWidget
            };
            let ink = if enabled {
                palette.text
            } else {
                palette.text_muted
            };
            if enabled {
                ui.painter().rect_filled(
                    submit,
                    18,
                    theme::mix(
                        palette.surface,
                        palette.accent,
                        if palette.dark { 0.34 } else { 0.18 },
                    ),
                );
            }
            if enabled
                && (button(ui, submit, "widget_composer_submit", label, palette)
                    .on_hover_text(label)
                    .clicked()
                    || enter)
            {
                actions.push(submit_action);
            }
            if submit_width > 55.0 {
                centered_text(ui, submit, label, 13.0, ink);
            } else {
                draw_icon(ui, submit.center(), Icon::Forward, ink);
            }
        }
    }
    surface.finish(ui, rect, Role::SearchPill);

    if progress < 0.95 || !open {
        return;
    }
    let status = if let Some(issue) = composer.issue {
        Some(tr(issue))
    } else if composer.loading_photo {
        Some(tr("newtab-widget-fetching-photo"))
    } else if composer.generation {
        Some(tr("newtab-composer-creating"))
    } else {
        composer.dictation.map(|phase| {
            tr(match phase {
                Dictation::Starting => "newtab-dictation-starting",
                Dictation::Listening => "newtab-dictation-listening",
                Dictation::Finishing => "newtab-dictation-finishing",
            })
        })
    };
    if let Some(preview) = composer.preview.as_mut() {
        let max_height = (content.height() * 0.38).clamp(88.0, 280.0);
        let grid_width = dashboard_width(content.width());
        let columns = columns_for_width(grid_width);
        let preview_span = preview.span.fit(columns, widgets::MAX_HEIGHT);
        let stride = (grid_width + GAP) / f32::from(columns);
        let panel_width = (stride * f32::from(preview_span.w) + 14.0)
            .max(286.0)
            .min(rect.width());
        let size_key = Id::unique((
            "newtab_widget_preview_height",
            &preview.title,
            preview.span.w,
            preview.span.h,
        ));
        let cached_height = ui.ctx().data(|data| data.get_temp::<f32>(size_key));
        let height = cached_height.unwrap_or(max_height).clamp(88.0, max_height);
        let bounds = Rect::from_min_size(
            pos2(
                rect.center().x - panel_width / 2.0,
                rect.top() - height - 10.0,
            ),
            vec2(panel_width, height),
        );
        ui.interact(
            bounds,
            Id::unique("newtab_widget_preview_surface"),
            Sense::click(),
        );
        let surface = glass_surface(ui, bounds, palette, Role::DashboardCard);
        let mut body = ui.new_child(
            UiBuilder::new()
                .id_salt("newtab_widget_preview")
                .max_rect(bounds.shrink(15.0)),
        );
        body.visuals_mut().override_text_color = Some(palette.text);
        body.horizontal_wrapped(|ui| {
            ui.strong(&preview.title);
            let summary = format!("{} × {}", preview.span.w, preview.span.h);
            egui::ComboBox::from_id_salt("newtab_preview_widget_size")
                .selected_text(if preview.automatic {
                    format!("{} · {summary}", tr("newtab-widget-fit-content"))
                } else {
                    summary
                })
                .show_glass(ui, |ui| {
                    if ui
                        .selectable_label(preview.automatic, tr("newtab-widget-fit-content"))
                        .clicked()
                    {
                        preview.automatic = true;
                        preview.span = custom::preferred_span(&preview.title, &preview.blocks);
                    }
                    for (span, label) in widgets::SIZES {
                        if ui
                            .selectable_label(!preview.automatic && span == preview.span, tr(label))
                            .clicked()
                        {
                            preview.span = span;
                            preview.automatic = false;
                        }
                    }
                });
        });
        if let Some(issue) = composer.issue {
            body.small(egui::RichText::new(tr(issue)).color(palette.text_muted));
        }
        body.add_space(6.0);
        let scroll = egui::ScrollArea::vertical()
            .max_height(body.available_height())
            .show(&mut body, |ui| {
                custom_widget_controls(
                    ui,
                    palette,
                    &mut preview.blocks,
                    jiff::Timestamp::now().as_second(),
                    (f32::from(preview_span.h) * stride - 84.0)
                        .clamp(46.0, (max_height - 80.0).max(46.0)),
                    None,
                );
            });
        let wanted = (scroll.inner_rect.top() - bounds.top() + scroll.content_size.y + 15.0)
            .clamp(88.0, max_height)
            .ceil();
        if (height - wanted).abs() >= 1.0 {
            ui.ctx().data_mut(|data| data.insert_temp(size_key, wanted));
            ui.ctx().request_repaint();
        }
        surface.finish(ui, bounds, Role::DashboardCard);
    } else if let Some(status) = status {
        let bounds = Rect::from_min_size(
            pos2(rect.left(), rect.top() - 43.0),
            vec2(rect.width(), 34.0),
        );
        let surface = glass_surface(ui, bounds, palette, Role::SearchPill);
        centered_text(ui, bounds.shrink(8.0), status, 12.0, palette.text);
        surface.finish(ui, bounds, Role::SearchPill);
    }
}

fn backdrop(
    ui: &Ui,
    chrome: &ChromeContext,
    page: Rect,
    content: Rect,
    palette: &Palette,
    background: NewTabBackground,
) {
    let painter = ui
        .ctx()
        .layer_painter(egui::LayerId::background())
        .with_clip_rect(page);
    // The native window already clips a full-bleed page. Inset pages share
    // the rounded path used by the content outline.
    let corners = if crate::ui::new_tab_fills_native_window(ui, page) {
        egui::CornerRadius::ZERO
    } else {
        theme::content_corners(palette)
    };

    let motion_enabled =
        chrome.settings.new_tab_animate_background && palette.material.animation > 0.0;
    if background.animated() || background == NewTabBackground::CustomImage {
        let animation = background
            .animated()
            .then(|| crate::newtab_wallpaper::phase(ui, motion_enabled));
        crate::newtab_wallpaper::paint(
            &painter,
            page,
            corners,
            background,
            chrome.wallpaper.texture.as_ref(),
            animation,
        );
    } else if !chrome.native_glass.new_tab_backdrop {
        painter.rect_filled(page, corners, theme::page_base(palette));
    }
    let glow = GridGlow::sample(
        ui,
        content,
        background == NewTabBackground::Grid && motion_enabled,
    );
    if background == NewTabBackground::Grid {
        paint_grid(&painter, page, corners, palette, &glow);
    }
    if let Some(capture) = &chrome.capture {
        crate::backdrop::capture_into(&painter, page, capture);
    }
}

const GRID_TRAIL_SECONDS: f64 = 0.38;

#[derive(Clone, Copy, Default)]
struct GridGlow {
    cursor: Option<Pos2>,
    trail: [Option<(Pos2, f64)>; 8],
    time: f64,
}

impl GridGlow {
    fn sample(ui: &Ui, content: Rect, enabled: bool) -> Self {
        let id = ui.scope_id().with("new_tab_grid_glow");
        let (cursor, time, focused) =
            ui.input(|input| (input.pointer.hover_pos(), input.time, input.focused));
        if !enabled || !focused {
            ui.ctx().data_mut(|data| data.remove::<Self>(id));
            return Self::default();
        }

        let cursor = cursor.filter(|point| content.contains(*point));
        let glow = ui.ctx().data_mut(|data| {
            let glow = data.get_temp_mut_or_default::<Self>(id);
            glow.update(cursor, time);
            *glow
        });
        // Pointer events move the glow. The trail alone needs timed repaints.
        if glow.trail[0].is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
        glow
    }

    fn update(&mut self, cursor: Option<Pos2>, time: f64) {
        self.time = time;
        for sample in &mut self.trail {
            if sample.is_some_and(|(_, at)| time - at >= GRID_TRAIL_SECONDS) {
                *sample = None;
            }
        }
        if let Some(previous) = self.cursor
            && cursor != self.cursor
            && (cursor.is_none()
                || self.trail[0]
                    .is_none_or(|(last, _)| last.distance_sq(previous) > 16.0_f32.powi(2)))
        {
            self.trail.rotate_right(1);
            self.trail[0] = Some((previous, time));
        }
        self.cursor = cursor;
    }

    fn brightness(&self, point: Pos2) -> f32 {
        let falloff = |center: Pos2, radius: f32| {
            (1.0 - point.distance_sq(center) / (radius * radius))
                .max(0.0)
                .powi(2)
        };
        let head = self.cursor.map_or(0.0, |cursor| falloff(cursor, 112.0));
        self.trail
            .iter()
            .flatten()
            .fold(head, |light, &(center, at)| {
                let life = (1.0 - (self.time - at) / GRID_TRAIL_SECONDS) as f32;
                light.max(falloff(center, 80.0) * life * life * 0.65)
            })
    }
}

fn paint_grid(
    painter: &egui::Painter,
    page: Rect,
    corners: egui::CornerRadius,
    palette: &Palette,
    glow: &GridGlow,
) {
    const SPACING: f32 = 29.0;
    const DOT: f32 = 1.15;
    let opacity = if palette.dark { 0.55 } else { 0.42 };
    let tint = palette.accent.gamma_multiply(opacity);
    let highlight = if palette.dark {
        theme::mix(palette.accent, Color32::WHITE, 0.36)
    } else {
        palette.accent
    };
    let active = glow.cursor.is_some() || glow.trail[0].is_some();
    let mut y = page.top() + SPACING;
    while y < page.bottom() - DOT {
        // Leave room for the entire dot at the corners, even with large
        // custom corner radii. The rest of the grid keeps its spacing.
        let inset = |top: u8, bottom: u8| {
            let top = f32::from(top) + DOT;
            let bottom = f32::from(bottom) + DOT;
            if y < page.top() + top {
                top
            } else if y > page.bottom() - bottom {
                bottom
            } else {
                DOT
            }
        };
        let left = page.left() + inset(corners.nw, corners.sw);
        let right = page.right() - inset(corners.ne, corners.se);
        let mut x = page.left() + SPACING;
        while x < right {
            if x > left {
                let dot = pos2(x, y);
                let light = if active { glow.brightness(dot) } else { 0.0 };
                if light > 0.0 {
                    let room = (x - left)
                        .min(right - x)
                        .min(page.bottom() - y - DOT)
                        .max(DOT);
                    painter.circle_filled(
                        dot,
                        (3.0 + 4.0 * light).min(room),
                        highlight.gamma_multiply(light * 0.18),
                    );
                    painter.circle_filled(
                        dot,
                        (DOT + 1.45 * light).min(room),
                        theme::mix(palette.accent, highlight, light)
                            .gamma_multiply(opacity + (1.0 - opacity) * light),
                    );
                } else {
                    painter.circle_filled(dot, DOT, tint);
                }
            }
            x += SPACING;
        }
        y += SPACING;
    }
}

fn header(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    width: f32,
    has_wallpaper: bool,
    actions: &mut Vec<UiAction>,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 40.0), Sense::hover());
    let badge = Rect::from_center_size(rect.left_center() + vec2(19.0, 0.0), vec2(38.0, 38.0));
    crate::ui::paint_application_icon(ui, badge, palette.dark);
    let settings = Rect::from_center_size(rect.right_center() - vec2(19.0, 0.0), vec2(38.0, 38.0));
    let label = tr("newtab-customize-page");
    let response = button(ui, settings, "newtab_settings", label, palette).on_hover_text(label);
    let name = rect.shrink2(vec2(46.0, 0.0));
    let draw_labels = |offset, ink| {
        text(ui, name.translate(offset), zervo_core::APP_NAME, 20.0, ink);
        draw_icon(ui, settings.center() + offset, Icon::Sliders, ink);
    };
    let ink = if has_wallpaper {
        // These two sit directly on the wallpaper. Outline the foreground
        // so it reads over both bright and dark parts of an image.
        let outline = palette.ink_on(Color32::WHITE).0;
        for offset in [
            vec2(-1.0, 0.0),
            vec2(1.0, 0.0),
            vec2(0.0, -1.0),
            vec2(0.0, 1.0),
        ] {
            draw_labels(offset, outline);
        }
        palette.ink_on(Color32::BLACK).0
    } else {
        palette.text
    };
    draw_labels(egui::Vec2::ZERO, ink);

    let popup = egui::Popup::menu(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .gap(6.0)
        .width((width - 20.0).min(330.0));
    let max_height = (ui.ctx().content_rect().height() - 90.0).max(140.0);
    overlay_glass::popup(popup, ui.ctx(), |ui| {
        egui::ScrollArea::vertical()
            .max_height(max_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                wallpaper_settings(ui, chrome, palette, label, actions)
            });
    });
}

fn wallpaper_settings(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    label: &str,
    actions: &mut Vec<UiAction>,
) {
    let available = ui.available_width();
    let columns = if available >= 265.0 { 3 } else { 2 };
    let preview_width = (available - (columns - 1) as f32 * 8.0) / columns as f32;
    ui.label(egui::RichText::new(label).strong().color(palette.text));
    ui.add_space(6.0);
    let mut chosen = chrome.settings.new_tab_background;
    for row in NewTabBackground::ALL.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for &background in row {
                if wallpaper_preview(ui, palette, background, chosen == background, preview_width) {
                    chosen = background;
                }
            }
        });
    }

    if chrome.wallpaper.texture.is_some()
        && ui
            .selectable_label(
                chosen == NewTabBackground::CustomImage,
                NewTabBackground::CustomImage.label(),
            )
            .clicked()
    {
        chosen = NewTabBackground::CustomImage;
    }
    ui.add_space(4.0);
    if ui
        .add_enabled(
            !chrome.wallpaper.busy,
            egui::Button::new(
                egui::RichText::new(tr("newtab-wallpaper-choose-file")).color(palette.text),
            ),
        )
        .clicked()
    {
        actions.push(UiAction::ImportWallpaper);
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    if chrome.wallpaper.can_generate {
        if ui
            .add_enabled(
                !chrome.wallpaper.busy,
                egui::Button::new(
                    egui::RichText::new(tr("newtab-wallpaper-generate")).color(palette.text),
                ),
            )
            .clicked()
        {
            actions.push(UiAction::GenerateWallpaper);
        }
    } else {
        ui.label(
            egui::RichText::new(tr("newtab-playground-unavailable"))
                .small()
                .color(palette.text_muted),
        );
    }
    if chrome.wallpaper.busy {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(tr("newtab-wallpaper-loading"));
        });
    } else if let Some(error) = chrome.wallpaper.error {
        ui.label(egui::RichText::new(error.label()).color(palette.text));
    }
    ui.separator();
    let motion_changed = crate::widgets::toggle(
        ui,
        &mut chrome.settings.new_tab_animate_background,
        tr("newtab-wallpaper-animate"),
        palette,
    );
    let widgets_changed = crate::widgets::toggle(
        ui,
        &mut chrome.settings.new_tab_show_widgets,
        tr("newtab-customize-widgets"),
        palette,
    );
    if widgets_changed && !chrome.settings.new_tab_show_widgets {
        ui.ctx().data_mut(|data| data.remove::<bool>(editing_id()));
    }
    if ui.button(tr("newtab-edit-widgets")).clicked() {
        chrome.settings.new_tab_show_widgets = true;
        ui.ctx()
            .data_mut(|data| data.insert_temp(editing_id(), true));
        actions.push(UiAction::PersistSettings);
        ui.close();
    }
    if chosen != chrome.settings.new_tab_background || motion_changed || widgets_changed {
        chrome.settings.new_tab_background = chosen;
        actions.push(UiAction::PersistSettings);
        ui.ctx().request_repaint();
    }
}

fn wallpaper_preview(
    ui: &mut Ui,
    palette: &Palette,
    background: NewTabBackground,
    selected: bool,
    width: f32,
) -> bool {
    let (bounds, response) = ui.allocate_exact_size(vec2(width, 68.0), Sense::click());
    let radius = egui::CornerRadius::same(8);
    let thumbnail = bounds.with_max_y(bounds.top() + 43.0).shrink(2.0);
    let tint = match background {
        NewTabBackground::Frosted => palette.surface,
        NewTabBackground::LiquidGlass => palette.active,
        _ => theme::page_base(palette),
    };
    if background.animated() {
        crate::newtab_wallpaper::paint(ui.painter(), thumbnail, radius, background, None, None);
    } else {
        ui.painter().rect_filled(thumbnail, radius, tint);
    }
    if background == NewTabBackground::Grid {
        for column in 1..10 {
            for row in 1..4 {
                let point = pos2(
                    thumbnail.left() + thumbnail.width() * column as f32 / 10.0,
                    thumbnail.top() + thumbnail.height() * row as f32 / 4.0,
                );
                ui.painter()
                    .circle_filled(point, 0.75, palette.accent.gamma_multiply(0.5));
            }
        }
    }
    if selected || response.has_focus() || response.hovered() {
        ui.painter().rect_stroke(
            thumbnail.expand(1.0),
            radius,
            Stroke::new(
                if selected { 2.0 } else { 1.0 },
                if selected || response.has_focus() {
                    palette.accent
                } else {
                    palette.border
                },
            ),
            StrokeKind::Inside,
        );
    }
    if selected {
        let center = thumbnail.right_top() + vec2(-10.0, 10.0);
        ui.painter().circle_filled(center, 7.5, palette.accent);
        icons::draw_icon(
            ui.painter(),
            Rect::from_center_size(center, vec2(11.0, 11.0)),
            Icon::Check,
            Color32::WHITE,
        );
    }
    let label = background.label();
    text(
        ui,
        bounds
            .with_min_y(thumbnail.bottom())
            .shrink2(vec2(3.0, 0.0)),
        label,
        12.0,
        palette.text,
    );
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, label));
    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(label)
        .clicked()
}

fn search(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    width: f32,
    actions: &mut Vec<UiAction>,
) {
    let (slot, _) = ui.allocate_exact_size(vec2(width, 54.0), Sense::hover());
    let bounds = Rect::from_center_size(slot.center(), vec2(width.min(660.0), 54.0));
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
    let engine = chrome.settings.search_engine;
    let compact = bounds.width() < 300.0;
    let chip_width = if compact { 51.0 } else { 140.0 };
    let chip = Rect::from_min_size(bounds.min + vec2(7.0, 7.0), vec2(chip_width, 40.0));
    ui.painter().rect(
        chip,
        20,
        theme::mix(
            palette.surface,
            palette.accent,
            if palette.dark { 0.28 } else { 0.16 },
        ),
        Stroke::new(1.0, palette.accent.gamma_multiply(0.35)),
        StrokeKind::Inside,
    );
    let engine_label = format!("{}: {}", tr("settings-search-engine"), engine.label());
    let picker = button(ui, chip, "newtab_engine_picker", &engine_label, palette)
        .on_hover_text(&engine_label);
    draw_icon(
        ui,
        chip.left_center() + vec2(17.0, 0.0),
        Icon::Globe,
        palette.text,
    );
    icons::draw_icon(
        ui.painter(),
        Rect::from_center_size(chip.right_center() - vec2(13.0, 0.0), vec2(10.0, 10.0)),
        Icon::CaretDown,
        palette.text_muted,
    );
    if !compact {
        text(
            ui,
            chip.with_min_x(chip.left() + 31.0)
                .with_max_x(chip.right() - 23.0),
            engine.label(),
            13.0,
            palette.text,
        );
    }

    let hint = tr("chrome-search-or-enter-an-address");
    let input = bounds.with_min_x(chip.right() + 3.0).shrink(4.0);
    let response = button(ui, input, "newtab_search", hint, palette);
    let mut title = input
        .with_min_x(input.left() + 7.0)
        .with_max_x(input.right() - 13.0);
    if bounds.width() >= 330.0 {
        let (shortcut, badge_width) = if cfg!(target_os = "macos") {
            ("⌘L", 36.0)
        } else {
            ("Ctrl+L", 57.0)
        };
        let badge = Rect::from_center_size(
            pos2(input.right() - 12.0 - badge_width * 0.5, input.center().y),
            vec2(badge_width, 27.0),
        );
        ui.painter().rect_stroke(
            badge,
            8,
            Stroke::new(1.0, palette.border),
            StrokeKind::Inside,
        );
        ui.painter().text(
            badge.center(),
            egui::Align2::CENTER_CENTER,
            shortcut,
            FontId::proportional(12.0),
            palette.text_muted,
        );
        title.set_right(badge.left() - 11.0);
    }
    text(ui, title, hint, 15.0, palette.text);
    surface.finish(ui, bounds, Role::SearchPill);

    if response.clicked() {
        actions.push(UiAction::OpenCommandFrom(bounds));
    }
    let picked = overlay_glass::popup(
        egui::Popup::menu(&picker).gap(4.0).width(148.0),
        ui.ctx(),
        |ui| {
            let mut picked = None;
            for option in SearchEngine::ALL {
                if ui
                    .selectable_label(engine == option, option.label())
                    .clicked()
                {
                    picked = Some(option);
                    ui.close();
                }
            }
            picked
        },
    );
    if let Some(option) = picked.and_then(|popup| popup.inner)
        && option != engine
    {
        chrome.settings.search_engine = option;
        actions.push(UiAction::PersistSettings);
        ui.ctx().request_repaint();
    }
}

fn cards(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    width: f32,
    actions: &mut Vec<UiAction>,
) {
    let columns = columns_for_width(width);
    let mut placed = widgets::arrange(&custom::tiles(chrome.settings), columns);
    let editing = ui.ctx().data(|data| data.get_temp::<bool>(editing_id())) == Some(true);
    if editing {
        ui.horizontal_wrapped(|ui| {
            let add = ui.button(tr("newtab-add-card"));
            overlay_glass::popup(egui::Popup::menu(&add), ui.ctx(), |ui| {
                for kind in Kind::ALL {
                    let available = placed.iter().all(|widget| widget.kind != kind);
                    let before = placed.clone();
                    if ui
                        .add_enabled(available, egui::Button::new(kind.label()))
                        .clicked()
                        && widgets::add(&mut placed, kind, columns)
                    {
                        save_widgets(chrome, &before, &placed, actions);
                        ui.close();
                    }
                }
            });
            if ui.button(tr("common-done")).clicked() {
                ui.ctx().data_mut(|data| data.remove::<bool>(editing_id()));
            }
        });
        ui.add_space(8.0);
    }
    if placed.is_empty() && !editing {
        return;
    }
    let occupied = placed
        .iter()
        .map(|widget| widget.cell.row + widget.span.h)
        .max()
        .unwrap_or(0);
    let active_drag = ui.ctx().dragged_id().or_else(|| ui.ctx().drag_stopped_id());
    let rows = if editing
        || placed
            .iter()
            .any(|widget| is_edit_handle(ui, widget.kind, active_drag))
    {
        (occupied + widgets::MAX_HEIGHT).min(widgets::ROWS)
    } else {
        occupied
    };
    let cell = (width - GAP * f32::from(columns - 1)) / f32::from(columns);
    let height = cell * f32::from(rows) + GAP * f32::from(rows.saturating_sub(1));
    let (board, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let grid = Metrics {
        rows,
        ..Metrics::new(board, columns, cell, GAP)
    };
    let mut change = None;
    let reveal = ui.ctx().data(|data| data.get_temp::<u64>(reveal_id()));
    for (index, widget) in placed.iter().enumerate() {
        let rect = grid.rect(widget.cell, widget.span);
        if let Kind::Custom(id) = widget.kind
            && Some(id) == reveal
        {
            ui.scroll_to_rect(rect, Some(egui::Align::Center));
            ui.ctx().data_mut(|data| data.remove::<u64>(reveal_id()));
        }
        let dragging = is_edit_handle(ui, widget.kind, active_drag);
        if !ui.is_rect_visible(rect) && !dragging {
            continue;
        }
        let control = widget.span.h == 1;
        let role = if control {
            Role::DashboardControl
        } else {
            Role::DashboardCard
        };
        let surface = glass_surface(ui, rect, palette, role);
        let compact = control || rect.width() < 126.0 || rect.height() < 108.0;
        let options_open = egui::Popup::is_id_open(ui.ctx(), widget_options_id(ui, widget.kind));
        let edit_controls = (editing || dragging || options_open || ui.rect_contains_pointer(rect))
            .then(|| CardEditControls::new(rect, compact));
        let body_rect = edit_controls.map_or(rect, |controls| controls.content);
        let mut card = ui.new_child(
            UiBuilder::new()
                .id_salt(("newtab_card", widget.kind))
                .max_rect(body_rect),
        );
        card.set_clip_rect(ui.clip_rect().intersect(body_rect));
        if editing {
            card.disable();
            card.set_opacity(ui.opacity());
        }
        let symbol_only = edit_controls.is_some_and(|controls| controls.symbol_only);
        if let Kind::Custom(id) = widget.kind {
            if let Some(custom) = chrome
                .settings
                .new_tab_custom_widgets
                .iter_mut()
                .find(|item| item.id == id)
            {
                draw_custom_widget(
                    &mut card,
                    palette,
                    body_rect,
                    custom,
                    compact,
                    symbol_only,
                    actions,
                );
            }
        } else if compact {
            compact_widget(
                &card,
                chrome,
                palette,
                body_rect,
                *widget,
                symbol_only,
                actions,
            );
        } else {
            let inner = rect.shrink(15.0);
            match widget.kind {
                Kind::Clock => clock(&card, palette, inner),
                Kind::Recent => recent(&card, chrome, palette, inner, actions),
                Kind::Pinned => pinned(&card, chrome, palette, inner, actions),
                Kind::Checklist => checklist(&mut card, chrome, palette, inner),
                Kind::Workspace => pulse(&card, chrome, palette, inner),
                Kind::Custom(_) => {},
            }
        }
        if let Some(controls) = edit_controls {
            let fit = if let Kind::Custom(id) = widget.kind {
                chrome
                    .settings
                    .new_tab_custom_widgets
                    .iter()
                    .find(|card| card.id == id)
                    .map(|card| custom::preferred_span(&card.title, &card.blocks))
            } else {
                widgets::defaults()
                    .iter()
                    .find(|item| item.kind == widget.kind)
                    .map(|item| item.span)
            };
            change = edit_widget(ui, palette, grid, &placed, index, controls, fit).or(change);
        }
        surface.finish(ui, rect, role);
    }
    match change {
        Some(WidgetEdit::Commit(next)) => save_widgets(chrome, &placed, &next, actions),
        Some(WidgetEdit::Preview { widget, resizing }) => {
            paint_widget_preview(ui, palette, grid, widget, resizing);
        },
        None => {},
    }
}

fn save_widgets(
    chrome: &mut ChromeContext,
    before: &[Widget],
    after: &[Widget],
    actions: &mut Vec<UiAction>,
) {
    let updated = widgets::apply_view_edit(&custom::tiles(chrome.settings), before, after);
    custom::save_layout(chrome.settings, &updated);
    actions.push(UiAction::PersistSettings);
}

fn draw_custom_widget(
    ui: &mut Ui,
    palette: &Palette,
    rect: Rect,
    card: &mut CustomWidget,
    compact: bool,
    symbol_only: bool,
    actions: &mut Vec<UiAction>,
) {
    let timestamp = jiff::Timestamp::now();
    let now = timestamp.as_second();
    let repaint = if card
        .blocks
        .iter()
        .any(|block| matches!(block, Block::Timer { ends_at: Some(end), .. } if *end > now))
    {
        Some(1)
    } else if card
        .blocks
        .iter()
        .any(|block| matches!(block, Block::Date { .. } | Block::Clock { .. }))
    {
        Some(60 - now.rem_euclid(60))
    } else {
        None
    };
    if let Some(seconds) = repaint {
        ui.ctx()
            .request_repaint_after(Duration::from_secs(seconds as u64));
    }
    let glyph = match card.blocks.first() {
        Some(Block::Checklist(_)) => Icon::Todo,
        Some(Block::Timer { .. } | Block::Clock { .. }) => Icon::Clock,
        Some(Block::Links(_)) => Icon::Globe,
        Some(Block::Image(_)) => Icon::Image,
        Some(Block::Counter { .. } | Block::Metric { .. }) => Icon::TrendUp,
        Some(Block::Date { .. }) => Icon::Calendar,
        _ => Icon::Note,
    };
    if symbol_only {
        if rect.height() >= 12.0 {
            icons::draw_icon(
                ui.painter(),
                Rect::from_center_size(rect.center(), vec2(14.0, 14.0)),
                glyph,
                palette.accent,
            );
        }
        return;
    }
    if compact {
        let title = match card.blocks.as_slice() {
            [
                Block::Timer {
                    minutes, ends_at, ..
                },
            ] => timer_text(*minutes, *ends_at, now),
            [Block::Counter { value, .. }] => format!("{} · {value}", card.title),
            [Block::Clock { zone, .. }] => {
                custom::clock_time(zone, timestamp).unwrap_or_else(|| card.title.clone())
            },
            _ => card.title.clone(),
        };
        let response = button(ui, rect, ("custom_widget", card.id), &card.title, palette)
            .on_hover_text(&card.title);
        centered_text(ui, rect.shrink2(vec2(5.0, 0.0)), &title, 12.0, palette.text);
        overlay_glass::popup(egui::Popup::menu(&response).width(260.0), ui.ctx(), |ui| {
            ui.strong(&card.title);
            egui::ScrollArea::vertical()
                .max_height(285.0)
                .show(ui, |ui| {
                    custom_widget_controls(
                        ui,
                        palette,
                        &mut card.blocks,
                        now,
                        220.0,
                        Some(actions),
                    );
                });
        });
        return;
    }
    let inner = rect.shrink(15.0);
    heading(ui, inner, palette, glyph, &card.title);
    let body = inner.with_min_y(inner.top() + 27.0);
    let mut body_ui = ui.new_child(UiBuilder::new().id_salt("widget_body").max_rect(body));
    egui::ScrollArea::vertical()
        .max_height(body.height())
        .auto_shrink([false; 2])
        .show(&mut body_ui, |ui| {
            custom_widget_controls(
                ui,
                palette,
                &mut card.blocks,
                now,
                (body.height() - 24.0).max(30.0),
                Some(actions),
            );
        });
}

fn timer_text(minutes: u16, ends_at: Option<i64>, now: i64) -> String {
    let seconds = ends_at.map_or(i64::from(minutes) * 60, |end| {
        end.saturating_sub(now).max(0)
    });
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn custom_widget_controls(
    ui: &mut Ui,
    palette: &Palette,
    blocks: &mut [Block],
    now: i64,
    image_height: f32,
    mut actions: Option<&mut Vec<UiAction>>,
) {
    ui.visuals_mut().override_text_color = Some(palette.text);
    let single = blocks.len() == 1;
    for (index, block) in blocks.iter_mut().enumerate() {
        if index > 0 {
            ui.add_space(5.0);
        }
        ui.push_id(index, |ui| {
            let interactive = actions.is_some();
            let mut changed = false;
            match block {
                Block::Image(photo) => {
                    let height = if single {
                        image_height
                    } else {
                        image_height.min(160.0)
                    };
                    draw_widget_photo(ui, photo, height, actions.as_deref_mut());
                },
                Block::Text(text) => {
                    ui.label(text.as_str());
                },
                Block::Bullets(rows) => {
                    for row in rows {
                        ui.label(format!("• {row}"));
                    }
                },
                Block::Checklist(items) => {
                    for item in items {
                        changed |= ui
                            .add_enabled(
                                interactive,
                                egui::Checkbox::new(&mut item.done, &item.text),
                            )
                            .changed();
                    }
                },
                Block::Note { label, text } => {
                    ui.strong(label.as_str());
                    if interactive {
                        changed = ui
                            .add(
                                egui::TextEdit::multiline(text)
                                    .desired_rows(2)
                                    .desired_width(f32::INFINITY)
                                    .hint_text(tr("newtab-write-note")),
                            )
                            .changed();
                        if changed {
                            Block::limit_note(text);
                        }
                    } else {
                        ui.label(if text.is_empty() {
                            tr("newtab-write-note")
                        } else {
                            text.as_str()
                        });
                    }
                },
                Block::Links(links) => {
                    for link in links {
                        if interactive {
                            if ui.link(&link.label).on_hover_text(&link.url).clicked()
                                && let Some(actions) = actions.as_deref_mut()
                            {
                                actions.push(UiAction::Navigate(link.url.clone()));
                            }
                        } else {
                            ui.label(egui::RichText::new(&link.label).color(palette.accent))
                                .on_hover_text(&link.url);
                        }
                    }
                },
                Block::Metric { label, text } => {
                    ui.strong(label.as_str());
                    ui.label(egui::RichText::new(text.as_str()).size(20.0));
                },
                Block::Counter {
                    label,
                    value,
                    goal,
                    step,
                } => {
                    ui.strong(label.as_str());
                    ui.horizontal_wrapped(|ui| {
                        let (limit, delta) = (custom::COUNTER_LIMIT, *step);
                        if ui
                            .add_enabled(interactive && *value > -limit, egui::Button::new("−"))
                            .on_hover_text(tr("newtab-decrease-counter"))
                            .clicked()
                        {
                            *value = (*value - delta).max(-limit);
                            changed = true;
                        }
                        ui.label(
                            egui::RichText::new(value.to_string())
                                .monospace()
                                .size(17.0),
                        );
                        if ui
                            .add_enabled(interactive && *value < limit, egui::Button::new("+"))
                            .on_hover_text(tr("newtab-increase-counter"))
                            .clicked()
                        {
                            *value = (*value + delta).min(limit);
                            changed = true;
                        }
                    });
                    if let Some(goal) = *goal {
                        ui.add(
                            egui::ProgressBar::new((*value as f32 / goal as f32).clamp(0.0, 1.0))
                                .fill(palette.accent)
                                .text(format!("{value} / {goal}")),
                        );
                    }
                },
                Block::Timer {
                    label,
                    minutes,
                    ends_at,
                } => {
                    if !label.is_empty() {
                        ui.strong(label.as_str());
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(timer_text(*minutes, *ends_at, now)).size(22.0),
                        );
                        if interactive
                            && ends_at.is_none_or(|end| end <= now)
                            && ui.button(tr("newtab-start-countdown")).clicked()
                        {
                            *ends_at = Some(now + i64::from(*minutes) * 60);
                            changed = true;
                        }
                        if interactive
                            && ends_at.is_some()
                            && ui.button(tr("newtab-reset-countdown")).clicked()
                        {
                            *ends_at = None;
                            changed = true;
                        }
                    });
                },
                Block::Date { label, date } => {
                    if !label.is_empty() {
                        ui.strong(label.as_str());
                    }
                    let today = jiff::ZonedDateTime::now().date();
                    if let Some(days) = date
                        .parse::<jiff::civil::Date>()
                        .ok()
                        .and_then(|day| today.until(day).ok())
                        .map(|span| span.get_days())
                    {
                        let text = if days < 0 {
                            tr("newtab-date-passed").to_owned()
                        } else if days == 0 {
                            tr("newtab-date-today").to_owned()
                        } else {
                            crate::l10n::message("newtab-days-left", [("days", days.into())])
                        };
                        ui.label(text);
                    }
                    ui.small(date.as_str());
                },
                Block::Clock { label, zone } => {
                    if !label.is_empty() {
                        ui.strong(label.as_str());
                    }
                    if let Some(time) = custom::clock_time(zone, jiff::Timestamp::now()) {
                        ui.label(egui::RichText::new(time).size(22.0));
                    } else {
                        ui.label(tr("newtab-widget-zone-unavailable"));
                    }
                },
            }
            if changed && let Some(actions) = actions.as_deref_mut() {
                actions.push(UiAction::PersistSettings);
            }
        });
    }
}

fn draw_widget_photo(
    ui: &mut Ui,
    photo: &crate::widget_photo::Photo,
    max_height: f32,
    actions: Option<&mut Vec<UiAction>>,
) {
    let Some((asset, texture)) = photo
        .asset
        .as_ref()
        .and_then(|asset| Some((asset, asset.texture(ui.ctx())?)))
    else {
        ui.label(tr("newtab-widget-photo-unavailable"));
        return;
    };
    let size = texture.resolution_vec2();
    let scale = (ui.available_width() / size.x)
        .min(max_height / size.y)
        .max(0.01);
    let size = size * scale;
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), size.y), Sense::hover());
    let rect = Rect::from_center_size(line.center(), size);
    ui.painter().add(
        egui::epaint::RectShape::filled(rect, 9, Color32::WHITE)
            .with_texture(texture.id(), Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0))),
    );
    let description = if photo.title.is_empty() {
        tr("newtab-widget-photo")
    } else {
        &photo.title
    };
    ui.interact(rect, ui.scope_id().with("photo"), Sense::hover())
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, description));
    if let Some(actions) = actions {
        if ui
            .link(egui::RichText::new(&asset.credit).small())
            .on_hover_text(&asset.source_page)
            .clicked()
        {
            actions.push(UiAction::Navigate(asset.source_page.clone()));
        }
    } else {
        ui.small(&asset.credit);
    }
}

fn move_handle_id(ui: &Ui, kind: Kind) -> Id {
    ui.scope_id().with(("move_widget", kind))
}

fn resize_handle_id(ui: &Ui, kind: Kind) -> Id {
    ui.scope_id().with(("resize_widget", kind))
}

fn widget_options_id(ui: &Ui, kind: Kind) -> Id {
    move_handle_id(ui, kind).with("options")
}

fn is_edit_handle(ui: &Ui, kind: Kind, active: Option<Id>) -> bool {
    active.is_some_and(|id| id == move_handle_id(ui, kind) || id == resize_handle_id(ui, kind))
}

#[derive(Clone, Copy)]
struct CardEditControls {
    grip: Rect,
    remove: Rect,
    resize: Rect,
    content: Rect,
    symbol_only: bool,
}

impl CardEditControls {
    fn new(rect: Rect, compact: bool) -> Self {
        let hit = (rect.size().min_elem() * 0.5).clamp(16.0, 22.0);
        let half = hit / 2.0;
        let box_at = |x, y| Rect::from_center_size(pos2(x, y), vec2(hit, hit));
        if compact && rect.width() >= 112.0 {
            // On a pill the actions share its center line and the content
            // stops before the controls, clear of the rounded cap.
            let y = rect.center().y;
            let x = rect.right() - half - 9.0;
            let spacing = hit + 3.0;
            let grip = box_at(x - 2.0 * spacing, y);
            let content = rect.with_max_x(grip.left() - 2.0);
            return Self {
                grip,
                remove: box_at(x, y),
                resize: box_at(x - spacing, y),
                symbol_only: content.width() < 82.0,
                content,
            };
        }
        if compact {
            let top = rect.top() + half + 4.0;
            let bottom = rect.bottom() - half - 4.0;
            let x = rect.center().x;
            let offset = (half + 2.0).min(rect.width() / 4.0);
            return Self {
                grip: box_at(x - offset, top),
                remove: box_at(x + offset, top),
                resize: box_at(x + offset, bottom),
                content: box_at(x - offset, bottom),
                symbol_only: true,
            };
        }
        let right = rect.right() - half - 8.0;
        let top = rect.top() + 25.0;
        Self {
            grip: box_at(right - hit - 4.0, top),
            remove: box_at(right, top),
            resize: box_at(right, rect.bottom() - half - 8.0),
            content: rect,
            symbol_only: false,
        }
    }
}

enum WidgetEdit {
    Commit(Vec<Widget>),
    Preview { widget: Widget, resizing: bool },
}

fn edit_widget(
    ui: &Ui,
    palette: &Palette,
    grid: Metrics,
    placed: &[Widget],
    index: usize,
    controls: CardEditControls,
    fit: Option<crate::grid::Span>,
) -> Option<WidgetEdit> {
    let widget = placed[index];
    let rect = grid.rect(widget.cell, widget.span);
    let remove = tr("newtab-take-this-card-off");
    let glyph = (controls.remove.width() - 6.0).min(15.0);
    let draw = |rect: Rect, icon| {
        icons::draw_icon(
            ui.painter(),
            Rect::from_center_size(rect.center(), vec2(glyph, glyph)),
            icon,
            palette.text,
        );
    };
    let remove_response = button(
        ui,
        controls.remove,
        ("newtab_remove_widget", widget.kind),
        remove,
        palette,
    );
    draw(controls.remove, Icon::Close);
    if overlay_glass::tooltip(remove_response, palette, remove).clicked() {
        return Some(WidgetEdit::Commit(
            placed
                .iter()
                .copied()
                .filter(|other| other.kind != widget.kind)
                .collect(),
        ));
    }

    let move_label = tr("newtab-widget-options");
    let resize_label = tr("newtab-resize-widget");
    let drag = ui.interact(
        controls.grip,
        move_handle_id(ui, widget.kind),
        Sense::click_and_drag(),
    );
    drag.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, move_label));
    let corner = ui.interact(
        controls.resize,
        resize_handle_id(ui, widget.kind),
        Sense::drag(),
    );
    for response in [&drag, &corner] {
        if response.hovered() || response.is_pointer_button_down_on() {
            ui.painter()
                .rect_filled(response.rect, 7, palette.surface_hover);
        }
    }
    draw(controls.grip, Icon::DragHandle);
    draw(controls.resize, Icon::Resize);
    corner.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, resize_label));
    let drag = drag.on_hover_cursor(CursorIcon::Grab);
    let corner = corner.on_hover_cursor(CursorIcon::ResizeNwSe);
    if !egui::Popup::is_id_open(ui.ctx(), widget_options_id(ui, widget.kind)) {
        overlay_glass::tooltip(drag.clone(), palette, tr("newtab-drag-widget"));
        overlay_glass::tooltip(corner.clone(), palette, resize_label);
    }

    let mut change = None;
    overlay_glass::popup(
        egui::Popup::menu(&drag).id(widget_options_id(ui, widget.kind)),
        ui.ctx(),
        |ui| {
            for (label, col, row) in [
                ("newtab-move-left", -1, 0),
                ("newtab-move-right", 1, 0),
                ("newtab-move-up", 0, -1),
                ("newtab-move-down", 0, 1),
            ] {
                let candidate = Widget {
                    cell: Cell::new(
                        widget.cell.col.saturating_add_signed(col),
                        widget.cell.row.saturating_add_signed(row),
                    ),
                    ..widget
                };
                let next = widgets::reposition(placed, index, candidate, grid.columns, grid.rows);
                if ui
                    .add_enabled(next.is_some(), egui::Button::new(tr(label)))
                    .clicked()
                {
                    change = next.map(WidgetEdit::Commit);
                    ui.close();
                }
            }
            ui.separator();
            if let Some(span) = fit {
                let span = span.fit(grid.columns - widget.cell.col, grid.rows - widget.cell.row);
                let candidate = Widget { span, ..widget };
                let next = widgets::reposition(placed, index, candidate, grid.columns, grid.rows);
                if ui
                    .add_enabled(
                        next.is_some(),
                        egui::Button::new(tr("newtab-widget-fit-content")),
                    )
                    .clicked()
                {
                    change = next.map(WidgetEdit::Commit);
                    ui.close();
                }
            }
            for (span, label) in widgets::SIZES {
                let candidate = Widget { span, ..widget };
                let next = widgets::reposition(placed, index, candidate, grid.columns, grid.rows);
                if ui
                    .add_enabled(
                        next.is_some(),
                        egui::Button::new(tr(label)).selected(span == widget.span),
                    )
                    .clicked()
                {
                    change = next.map(WidgetEdit::Commit);
                    ui.close();
                }
            }
        },
    );
    for (response, resizing) in [(drag, false), (corner, true)] {
        let delta = if let Some(delta) = response.total_drag_delta() {
            ui.ctx()
                .data_mut(|data| data.insert_temp(response.id, delta));
            delta
        } else if response.drag_stopped() {
            ui.ctx()
                .data_mut(|data| data.remove_temp::<egui::Vec2>(response.id))
                .unwrap_or_default()
        } else {
            continue;
        };
        let mut candidate = widget;
        if resizing {
            let stride = grid.cell + egui::Vec2::splat(GAP);
            let steps = delta / stride;
            if steps.x.abs().max(steps.y.abs()) < 0.5 {
                continue;
            }
            let size = vec2(f32::from(widget.span.w), f32::from(widget.span.h)) + steps;
            let Some(span) = widgets::snap_size(
                size,
                grid.columns - widget.cell.col,
                grid.rows - widget.cell.row,
            ) else {
                continue;
            };
            candidate.span = span;
        } else {
            candidate.cell = grid.cell_at(rect.min + delta, widget.span);
        }
        if let Some(next) = widgets::reposition(placed, index, candidate, grid.columns, grid.rows) {
            change = if response.drag_stopped() {
                Some(WidgetEdit::Commit(next))
            } else {
                Some(WidgetEdit::Preview {
                    widget: candidate,
                    resizing,
                })
            };
        }
    }
    change
}

fn paint_widget_preview(ui: &Ui, palette: &Palette, grid: Metrics, widget: Widget, resizing: bool) {
    let outline = grid.rect(widget.cell, widget.span);
    if !ui.is_rect_visible(outline) {
        return;
    }
    // Capture after every card so the native glass compositor places this
    // ink above the old card, including when the target fits inside it.
    let preview = Surface::begin_if(ui, palette.translucency == Translucency::LiquidGlass);
    let role = if widget.span.h == 1 {
        Role::DashboardControl
    } else {
        Role::DashboardCard
    };
    let radius = role.radius().min((outline.size().min_elem() * 0.5) as u8);
    let painter = ui.painter();
    painter.rect_stroke(
        outline,
        radius,
        Stroke::new(4.0, palette.surface),
        StrokeKind::Inside,
    );
    painter.rect_stroke(
        outline,
        radius,
        Stroke::new(2.0, palette.accent),
        StrokeKind::Inside,
    );
    if resizing {
        let size = crate::l10n::message(
            "newtab-widget-grid-size",
            [
                ("columns", i32::from(widget.span.w).into()),
                ("rows", i32::from(widget.span.h).into()),
            ],
        );
        let center = outline.center_bottom() - vec2(0.0, 16.0);
        let chip = Rect::from_center_size(center, vec2(45.0_f32.min(outline.width() - 8.0), 20.0));
        painter.rect_filled(chip, 8, palette.surface);
        centered_text(ui, chip.shrink2(vec2(2.0, 0.0)), &size, 11.0, palette.text);
    }
    preview.finish(ui, outline, Role::DashboardDragPreview);
}

fn clock(ui: &Ui, palette: &Palette, rect: Rect) {
    let now = local_clock(ui);
    let date_top = rect.height() - 25.0;
    let time_height = date_top - 25.0;
    heading(ui, rect, palette, Icon::Clock, tr("newtab-clock"));
    text(
        ui,
        band(rect, 22.0, time_height),
        &now.strftime("%H:%M").to_string(),
        (time_height - 5.0).clamp(20.0, 55.0),
        palette.text,
    );
    text(
        ui,
        band(rect, date_top, 24.0),
        &crate::dashboard::calendar_date(&now, false),
        13.0,
        palette.text_muted,
    );
}

fn local_clock(ui: &Ui) -> jiff::ZonedDateTime {
    let now = jiff::ZonedDateTime::now();
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs((60 - now.second()) as u64));
    now
}

fn compact_widget(
    ui: &Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    rect: Rect,
    widget: Widget,
    symbol_only: bool,
    actions: &mut Vec<UiAction>,
) {
    let (glyph, value, icon, action) = match widget.kind {
        Kind::Clock => (
            Icon::Clock,
            local_clock(ui).strftime("%H:%M").to_string(),
            None,
            None,
        ),
        Kind::Recent => {
            let last = chrome.library.history.last();
            (
                Icon::Recent,
                last.map_or("", |visit| {
                    crate::ui::display_name(&visit.title, &visit.url)
                })
                .to_owned(),
                last.and_then(|visit| chrome.saved_favicons.get(ui.ctx(), &visit.url)),
                last.map(|visit| UiAction::Navigate(visit.url.clone())),
            )
        },
        Kind::Pinned => {
            let pin = chrome
                .browser
                .workspaces
                .iter()
                .flat_map(|space| &space.tabs)
                .find(|tab| tab.pinned);
            (
                Icon::Pin,
                pin.map_or("", |tab| crate::ui::display_name(&tab.title, &tab.url))
                    .to_owned(),
                pin.and_then(|tab| chrome.tab_icon(ui.ctx(), tab)),
                pin.map(|tab| UiAction::SelectTab(tab.id)),
            )
        },
        Kind::Checklist => (
            Icon::Todo,
            format!(
                "{}/{}",
                chrome.library.tasks.iter().filter(|task| task.done).count(),
                chrome.library.tasks.len()
            ),
            None,
            None,
        ),
        Kind::Workspace => (
            Icon::TrendUp,
            chrome
                .browser
                .workspaces
                .get(chrome.browser.active_workspace)
                .map_or("", |space| space.name.as_str())
                .to_owned(),
            None,
            None,
        ),
        Kind::Custom(_) => return,
    };
    if symbol_only {
        if rect.height() >= 12.0 {
            icons::draw_icon(
                ui.painter(),
                Rect::from_center_size(rect.center(), vec2(14.0, 14.0)),
                glyph,
                palette.accent,
            );
        }
        return;
    }
    let title = widget.kind.label();
    let label = if value.is_empty() {
        title.to_owned()
    } else {
        format!("{title} · {value}")
    };
    let checklist = widget.kind == Kind::Checklist;
    if action.is_some() || checklist {
        let response = button(ui, rect, ("newtab_compact", widget.kind), &label, palette)
            .on_hover_text(&label);
        if let Some(action) = action
            && response.clicked()
        {
            actions.push(action);
        }
        if checklist {
            overlay_glass::popup(egui::Popup::menu(&response).width(280.0), ui.ctx(), |ui| {
                let (bounds, _) = ui.allocate_exact_size(vec2(280.0, 148.0), Sense::hover());
                self::checklist(ui, chrome, palette, bounds);
            });
        }
    } else {
        ui.interact(rect, ui.scope_id().with("compact_summary"), Sense::hover())
            .on_hover_text(label);
    }
    let horizontal = rect.width() >= rect.height() * 1.5;
    let icon_at = if horizontal {
        rect.left_center() + vec2(20.0, 0.0)
    } else {
        rect.center_top() + vec2(0.0, rect.height() * 0.32)
    };
    if let Some(icon) = icon {
        icon.paint(
            ui.painter(),
            Rect::from_center_size(icon_at, vec2(19.0, 19.0)),
            palette.dark,
            1.0,
        );
    } else {
        draw_icon(ui, icon_at, glyph, palette.accent);
    }
    if horizontal {
        let labels = rect
            .with_min_x(rect.left() + 35.0)
            .with_max_x(rect.right() - 9.0);
        if value.is_empty() {
            text(ui, labels, title, 12.5, palette.text);
        } else {
            let mid = labels.center().y;
            text(ui, labels.with_max_y(mid), title, 10.0, palette.text_muted);
            text(ui, labels.with_min_y(mid), &value, 13.0, palette.text);
        }
    } else {
        let label = if value.is_empty() { title } else { &value };
        let bounds = rect.shrink2(vec2(5.0, 0.0));
        centered_text(
            ui,
            band(bounds, rect.height() * 0.5, rect.height() * 0.34),
            label,
            12.0,
            palette.text,
        );
    }
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
    let dense = rect.height() < 137.0;
    let (top, step) = if dense { (24.0, 23.0) } else { (35.0, 37.0) };
    for (index, visit) in chrome
        .library
        .history
        .iter()
        .rev()
        .filter(|visit| shown.insert(visit.url.as_str()))
        .take(((rect.height() - top) / step).floor().max(1.0) as usize)
        .enumerate()
    {
        let row = band(rect, top + index as f32 * step, step - 2.0);
        let title = crate::ui::display_name(&visit.title, &visit.url);
        if button(ui, row, ("newtab_visit", index), title, palette)
            .on_hover_text(title)
            .clicked()
        {
            actions.push(UiAction::Navigate(visit.url.clone()));
        }
        let icon_rect =
            Rect::from_center_size(row.left_center() + vec2(13.0, 0.0), vec2(15.0, 15.0));
        if !crate::ui::paint_site_icon(ui, chrome.saved_favicons, &visit.url, icon_rect) {
            icons::draw_icon(ui.painter(), icon_rect, Icon::Globe, palette.text_muted);
        }
        let labels = row
            .with_min_x(row.left() + 30.0)
            .with_max_x(row.right() - 4.0);
        if dense {
            text(ui, labels, title, 12.5, palette.text);
        } else {
            text(ui, band(labels, 1.0, 18.0), title, 12.5, palette.text);
            text(
                ui,
                band(labels, 18.0, 14.0),
                &crate::ui::host_of(&visit.url),
                10.5,
                palette.text_muted,
            );
        }
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
    let columns = ((rect.width() + 8.0) / 72.0).floor().max(1.0) as usize;
    let rows = ((rect.height() - 31.0) / 55.0).floor().max(1.0) as usize;
    let width = (rect.width() - 8.0 * (columns - 1) as f32) / columns as f32;
    for tab in tabs.take(columns * rows) {
        let x = count % columns;
        let y = count / columns;
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
        if let Some(icon) = chrome.tab_icon(ui.ctx(), tab) {
            icon.paint(
                ui.painter(),
                Rect::from_center_size(mark, vec2(16.0, 16.0)),
                palette.dark,
                1.0,
            );
        } else {
            draw_icon(ui, mark, Icon::Globe, palette.text_muted);
        }
        let label = band(tile.shrink2(vec2(5.0, 0.0)), 25.0, 16.0);
        centered_text(ui, label, title, 10.5, palette.text);
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
    let list = band(rect, 30.0, rect.height() - 68.0);
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

    let input = band(rect, rect.height() - 32.0, 32.0);
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
    let height = (rect.height() - 52.0).clamp(24.0, 77.0);
    for (index, (value, label)) in stats.into_iter().enumerate() {
        let box_rect = Rect::from_min_size(
            rect.min + vec2((width + 7.0) * index as f32, 52.0),
            vec2(width, height),
        );
        ui.painter()
            .rect_filled(box_rect, 12, palette.surface_hover.gamma_multiply(0.44));
        let body = box_rect.shrink2(vec2(8.0, 0.0));
        text(
            ui,
            band(body, 2.0, height - 19.0),
            &value.to_string(),
            (height - 22.0).clamp(14.0, 28.0),
            palette.text,
        );
        text(
            ui,
            band(body, height - 19.0, 17.0),
            label,
            10.5,
            palette.text_muted,
        );
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
                .radius_exact(
                    role.radius()
                        .min((rect.width().min(rect.height()) * 0.5) as u8),
                )
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

fn centered_text(ui: &Ui, rect: Rect, label: &str, size: f32, ink: Color32) {
    let mut job =
        egui::text::LayoutJob::simple_singleline(label.to_owned(), FontId::proportional(size), ink);
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width().max(0.0));
    let galley = ui.painter().layout_job(job);
    ui.painter()
        .galley(rect.center() - galley.size() * 0.5, galley, ink);
}
