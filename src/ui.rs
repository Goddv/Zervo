//! The Zervo chrome, drawn with egui. Everything lives in the left
//! sidebar — nav icons beside the traffic lights, a large search pill,
//! a grid of pinned "essential" tabs, workspaces with tab rows, and a
//! bottom utility bar — around a floating rounded card holding the web
//! content (or the zervo://settings tab). Pure UI — emits `UiAction`s.

use std::collections::HashMap;

use egui::epaint::Mesh;
use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Frame, Id, Key, Margin, Panel, Rect,
    RichText, Sense, Shape, Stroke, StrokeKind, TextEdit, TextureHandle, Ui, pos2, vec2,
};

use chrono::Timelike;

use crate::glass::{self, Glass};
use crate::icons::{self, Icon};
use crate::settings::{AppIcon, NewTabBackground, NewTabPage, SearchEngine, Settings};
use crate::state::{BrowserState, TabId, TabKind};
use crate::theme::{self, AccentColor, Palette, ThemeMode};
use crate::widgets;

#[derive(Debug)]
pub enum UiAction {
    Navigate(String),
    Back,
    Forward,
    Reload,
    SelectTab(TabId),
    NewTab {
        workspace: usize,
    },
    CloseTab(TabId),
    TogglePin(TabId),
    NewWorkspace,
    SelectWorkspace(usize),
    OpenSettings,
    OpenDownloads,
    ToggleSidebar,
    CancelDownload(u64),
    RemoveDownload(u64),
    RevealDownload(u64),
    OpenDownload(u64),
    ClearDownloads,
    /// Pointer started dragging empty chrome — move the OS window.
    DragWindow,
    /// A setting changed: persist and re-apply (theme, etc.).
    SettingsChanged,
    /// A setting changed that needs nothing re-applied — just write it out.
    PersistSettings,
}

pub struct UiOutput {
    pub actions: Vec<UiAction>,
    /// Window-space rect of the web-content card, in logical points,
    /// snapped to the physical pixel grid so the blit and masks align.
    pub content_rect: egui::Rect,
    /// When true an internal page covers the content area — skip the blit.
    pub settings_open: bool,
    /// Window-space rect of the autohide sidebar while it is revealed.
    /// Pointer events over it must not be forwarded to the webview.
    pub chrome_overlay: Option<egui::Rect>,
    /// True while an ambient animation is on screen (aurora new-tab page):
    /// the caller schedules ~30fps timed wakes instead of the idle cadence.
    pub ambient: bool,
}

pub struct ChromeContext<'a> {
    pub browser: &'a mut BrowserState,
    pub settings: &'a mut Settings,
    pub palette: Palette,
    pub favicons: &'a HashMap<TabId, TextureHandle>,
    pub downloads: &'a crate::downloads::DownloadManager,
}

/// Draw the chrome into the root `Ui` handed to us by `EguiGlow::run`. The
/// caller registers the webview blit at `content_rect` (unless an internal
/// page is open), then calls [`finish_content_frame`].
pub fn draw(root: &mut Ui, chrome: &mut ChromeContext) -> UiOutput {
    let mut actions = Vec::new();

    // One continuous chrome base for the whole window, painted before the
    // panels so the top gradient runs unbroken from the sidebar across to
    // the right edge instead of stopping at the sidebar boundary.
    paint_chrome_base(
        root,
        &chrome.palette,
        chrome.settings.top_glow,
        chrome.settings.chrome_opacity,
    );

    // Autohide reveal: decided before the sidebar is drawn, but painted last
    // as a floating overlay. The collapsed handle stays allocated underneath
    // either way, so revealing never reflows the content card — and so never
    // resizes the webview, which would relayout the page.
    let peek = sidebar_peek(root, chrome);
    if chrome.browser.sidebar_collapsed {
        draw_collapsed_sidebar_handle(root, chrome, &mut actions);
    } else {
        draw_sidebar(root, chrome, &mut actions);
    }

    let outer = root.available_rect_before_wrap();
    let content_rect = paint_content_backdrop(root, outer, &chrome.palette);

    let active_kind = chrome.browser.active_tab().map(|tab| tab.kind);
    let mut ambient = false;
    match active_kind {
        Some(TabKind::Settings) => {
            draw_settings_page(root, chrome, content_rect, &mut actions);
        },
        Some(TabKind::NewTab) => {
            ambient = draw_newtab_page(root, chrome, content_rect, &mut actions);
        },
        Some(TabKind::Downloads) => {
            draw_downloads_page(root, chrome, content_rect, &mut actions);
        },
        _ => {},
    }
    let settings_open = matches!(
        active_kind,
        Some(TabKind::Settings | TabKind::NewTab | TabKind::Downloads)
    );

    let chrome_overlay = draw_sidebar_peek(root, chrome, &mut actions, peek);

    UiOutput {
        actions,
        content_rect,
        settings_open,
        chrome_overlay,
        ambient,
    }
}

/// Snap a rect in points to the physical pixel grid, so shapes drawn in
/// points and the GL blit (computed in pixels) agree exactly.
fn snap_rect(rect: Rect, pixels_per_point: f32) -> Rect {
    let snap = |value: f32| (value * pixels_per_point).round() / pixels_per_point;
    Rect::from_min_max(
        pos2(snap(rect.min.x), snap(rect.min.y)),
        pos2(snap(rect.max.x), snap(rect.max.y)),
    )
}

/// A subtle vertical gradient, used to give the chrome surfaces depth.
fn vertical_gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
}

/// Height of the chrome's top glow strip, in points.
const CHROME_GRADIENT_HEIGHT: f32 = 280.0;

/// Smoothstep falloff for the glow strip. Its slope reaches zero at the
/// bottom, so the strip melts into the flat chrome instead of ending on a
/// visible line (a linear ramp leaves a mach band where the slope breaks).
fn glow_falloff(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A vertical gradient with an eased (non-linear) colour ramp, emitted as a
/// stack of strips so the curve is actually followed.
fn eased_gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    // NOTE: interpolate in PREMULTIPLIED space, alpha included. `theme::mix`
    // returns an opaque colour, so using it here would make the strip fully
    // opaque with darkened (already premultiplied) RGB — a black band.
    fn lerp_premultiplied(a: Color32, b: Color32, t: f32) -> Color32 {
        let inv = 1.0 - t;
        let channel = |a: u8, b: u8| (a as f32 * inv + b as f32 * t) as u8;
        Color32::from_rgba_premultiplied(
            channel(a.r(), b.r()),
            channel(a.g(), b.g()),
            channel(a.b(), b.b()),
            channel(a.a(), b.a()),
        )
    }

    const STRIPS: usize = 32;
    let mut mesh = Mesh::default();
    for strip in 0..=STRIPS {
        let t = strip as f32 / STRIPS as f32;
        let color = lerp_premultiplied(top, bottom, glow_falloff(t));
        let y = rect.min.y + rect.height() * t;
        mesh.colored_vertex(pos2(rect.min.x, y), color);
        mesh.colored_vertex(pos2(rect.max.x, y), color);
    }
    for strip in 0..STRIPS as u32 {
        let base = strip * 2;
        mesh.add_triangle(base, base + 1, base + 3);
        mesh.add_triangle(base, base + 3, base + 2);
    }
    painter.add(Shape::mesh(mesh));
}

/// The lightest chrome color, at the very top of the window: a whisper of
/// accent, echoing Zen Browser's workspace tinting. Light comes from above in both
/// themes, so the top is always the lighter end.
fn glow_strip_top(palette: &Palette, strength: f32) -> Color32 {
    let full = theme::mix(
        {
            let lift = if palette.dark { 14 } else { 6 };
            Color32::from_rgb(
                (palette.bg.r() as i16 + lift).clamp(0, 255) as u8,
                (palette.bg.g() as i16 + lift).clamp(0, 255) as u8,
                (palette.bg.b() as i16 + lift + 2).clamp(0, 255) as u8,
            )
        },
        palette.accent,
        0.07,
    );
    // Scale the whole effect back toward the flat chrome color.
    theme::mix(palette.bg, full, strength.clamp(0.0, 1.0))
}

/// The chrome color at a given window-space `y` — used both to paint the
/// gradient and to tint anything that must blend into it seamlessly.
fn chrome_color_at(root: &Ui, y: f32, palette: &Palette, top_glow: f32) -> Color32 {
    if top_glow <= 0.0 {
        return palette.bg;
    }
    let top = root.ctx().content_rect().top();
    let t = glow_falloff((y - top) / CHROME_GRADIENT_HEIGHT);
    theme::mix(glow_strip_top(palette, top_glow), palette.bg, t)
}

/// Flat chrome fill plus the window-wide top gradient, painted under
/// everything else.
fn paint_chrome_base(root: &Ui, palette: &Palette, top_glow: f32, opacity: f32) {
    let painter = root.ctx().layer_painter(egui::LayerId::background());
    let window = root.ctx().content_rect();
    paint_chrome_fill(&painter, window, window.top(), palette, top_glow, opacity);
}

/// Fill `rect` with the chrome's colour, including the glow band across the
/// top of the window. `window_top` anchors the band, so a rect that starts
/// below the window's top edge still lines up with the rest of the chrome.
fn paint_chrome_fill(
    painter: &egui::Painter,
    rect: Rect,
    window_top: f32,
    palette: &Palette,
    top_glow: f32,
    opacity: f32,
) {
    let opacity = opacity.clamp(0.2, 1.0);
    let bg = palette.bg.gamma_multiply(opacity);
    if top_glow <= 0.0 {
        painter.rect_filled(rect, CornerRadius::ZERO, bg);
        return;
    }

    // The glow band and the flat fill below it are painted as DISJOINT rects.
    // Overlapping them would composite two translucent layers on top of each
    // other inside the band, making it measurably more opaque than the chrome
    // below and leaving a hard horizontal seam where the band ends.
    let band_bottom = (window_top + CHROME_GRADIENT_HEIGHT).min(rect.max.y);
    let band = Rect::from_min_max(rect.min, pos2(rect.max.x, band_bottom));
    let rest = Rect::from_min_max(pos2(rect.min.x, band_bottom), rect.max);
    if rest.is_positive() {
        painter.rect_filled(rest, CornerRadius::ZERO, bg);
    }
    if band.is_positive() {
        eased_gradient(
            painter,
            band,
            glow_strip_top(palette, top_glow).gamma_multiply(opacity),
            bg,
        );
    }
}

/// Compute the inset card rect. (Nothing is painted here — see the note.)
fn paint_content_backdrop(root: &Ui, outer: Rect, _palette: &Palette) -> Rect {
    // No background fill here: the window-wide chrome base already covers
    // this area, and filling it again would cut the gradient off at the
    // sidebar edge.
    // The card's shadow is NOT painted here: the webview blit overwrites the
    // whole square content rect, which would erase the shadow inside the
    // rounded corners and leave square unshadowed patches. It is painted as
    // a ring in `finish_content_frame`, after the corner masks.
    snap_rect(outer.shrink(theme::CONTENT_MARGIN), root.pixels_per_point())
}

/// The rounded-rect outline as (point, outward normal).
///
/// Sampling the four corner arcs and joining them also yields the straight
/// edges: an arc's endpoints sit exactly at the edge tangent points, and their
/// normals are the edge normals.
fn card_outline(rect: Rect, radius: f32, arc_segments: usize) -> Vec<(egui::Pos2, egui::Vec2)> {
    let mut outline = Vec::with_capacity(4 * (arc_segments + 1));
    let corners = [
        (pos2(rect.max.x - radius, rect.max.y - radius), 0.0_f32),
        (pos2(rect.min.x + radius, rect.max.y - radius), 0.5),
        (pos2(rect.min.x + radius, rect.min.y + radius), 1.0),
        (pos2(rect.max.x - radius, rect.min.y + radius), 1.5),
    ];
    for (centre, quarter) in corners {
        for segment in 0..=arc_segments {
            let angle =
                (quarter + segment as f32 / arc_segments as f32 * 0.5) * std::f32::consts::PI;
            let normal = vec2(angle.cos(), angle.sin());
            outline.push((centre + normal * radius, normal));
        }
    }
    outline
}

/// Extrude `outline` outwards as a ring mesh, colouring each radial step with
/// `colour_at(t)` where `t` runs 0 (at the outline) to 1 (at `spread`).
fn paint_outline_ring(
    painter: &egui::Painter,
    outline: &[(egui::Pos2, egui::Vec2)],
    spread: f32,
    steps: usize,
    colour_at: impl Fn(f32) -> Color32,
) {
    let mut mesh = Mesh::default();
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let colour = colour_at(t);
        for (point, normal) in outline {
            mesh.colored_vertex(*point + *normal * (spread * t), colour);
        }
    }
    let ring = outline.len() as u32;
    for step in 0..steps as u32 {
        for index in 0..ring {
            let next = (index + 1) % ring;
            let (inner, outer) = (step * ring, (step + 1) * ring);
            mesh.add_triangle(inner + index, inner + next, outer + next);
            mesh.add_triangle(inner + index, outer + next, outer + index);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// A soft shadow hugging a rounded rect.
///
/// Concentric strokes are the obvious approach and the wrong one: each stroke
/// has a hard edge, so a handful of them read as visible bands rather than a
/// shadow. Interpolating vertex colours across a ring mesh gives a continuous
/// falloff instead.
fn paint_card_shadow(painter: &egui::Painter, rect: Rect, radius: f32, palette: &Palette) {
    let outline = card_outline(rect, radius, 10);
    let base = palette.shadow.gamma_multiply(0.9);
    // Quadratic falloff, close to how a real penumbra reads.
    paint_outline_ring(painter, &outline, 9.0, 10, |t| {
        base.gamma_multiply((1.0 - t).powi(2))
    });
}

/// Hide the seam between the opaque corner masks and translucent chrome.
///
/// The masks have to be opaque — they cover the square corners of the page
/// blit, and anything less would let the page show through. But when the
/// chrome is translucent, that leaves the card's *square* bounding box faintly
/// visible as a patch of fully opaque chrome. Ramping the chrome to opaque
/// along the card's rounded edge and fading back out moves that transition
/// onto the rounded silhouette, where it belongs, and softens it.
fn paint_card_opacity_blend(
    painter: &egui::Painter,
    rect: Rect,
    radius: f32,
    chrome: Color32,
    chrome_opacity: f32,
) {
    if chrome_opacity >= 1.0 {
        return; // Opaque chrome has no seam to hide.
    }
    let outline = card_outline(rect, radius, 10);
    // Reach past the corner wedge — the furthest the square box sits from the
    // arc is radius * (sqrt(2) - 1).
    let spread = radius * 0.42 + 6.0;
    paint_outline_ring(painter, &outline, spread, 8, |t| {
        chrome.gamma_multiply((1.0 - t).powi(2))
    });
}

/// Draw the rounded-corner masks and border over the (square) webview blit.
/// Must run after the blit callback is registered on the background layer.
/// The masks are oversized by a pixel so no sliver of the square blit can
/// peek out at fractional DPI. `mask_corners` is false for internal pages
/// whose fill is already rounded — masking there double-paints the corners.
pub fn finish_content_frame(
    root: &Ui,
    content_rect: Rect,
    palette: &Palette,
    mask_corners: bool,
    top_glow: f32,
    border: bool,
    chrome_opacity: f32,
) {
    let painter = root.ctx().layer_painter(egui::LayerId::background());
    // The oversized fans must only bleed inward over the blit — clip them so
    // they can't notch the halo painted around the card.
    let fan_painter = painter.with_clip_rect(content_rect);
    let radius = theme::CONTENT_RADIUS;
    let pad = 1.5;

    // One arc segment per physical pixel, so the facets are sub-pixel and the
    // curve cannot look polygonal on a Retina display.
    let segments = ((radius * root.pixels_per_point()).ceil() as usize).clamp(16, 64);
    // (corner, arc center, start angle, outward direction)
    let corners = [
        (
            content_rect.left_top(),
            pos2(content_rect.left() + radius, content_rect.top() + radius),
            std::f32::consts::PI,
            vec2(-1.0, -1.0),
        ),
        (
            content_rect.right_top(),
            pos2(content_rect.right() - radius, content_rect.top() + radius),
            1.5 * std::f32::consts::PI,
            vec2(1.0, -1.0),
        ),
        (
            content_rect.right_bottom(),
            pos2(
                content_rect.right() - radius,
                content_rect.bottom() - radius,
            ),
            0.0,
            vec2(1.0, 1.0),
        ),
        (
            content_rect.left_bottom(),
            pos2(content_rect.left() + radius, content_rect.bottom() - radius),
            0.5 * std::f32::consts::PI,
            vec2(-1.0, 1.0),
        ),
    ];
    // All four corner masks in ONE mesh: independent triangles each get their
    // own antialiased edges, and the AA seams between adjacent fan triangles
    // let the page underneath shine through as hairlines. A single mesh with
    // shared vertices has no interior seams.
    //
    // Meshes are not antialiased at all, though, so the arc where the mask
    // meets the page is a hard, stair-stepped edge. Each arc is therefore
    // retraced afterwards with a stroked line — which epaint *does*
    // antialias — in the same colour, feathering the boundary.
    if mask_corners {
        let mut mesh = Mesh::default();
        let mut arc_edges: Vec<(Vec<egui::Pos2>, Color32)> = Vec::new();
        for (corner, center, start_angle, outward) in corners {
            let corner_out = corner + outward * pad;
            let mut arc: Vec<egui::Pos2> = (0..=segments)
                .map(|segment| {
                    let angle = start_angle
                        + (segment as f32 / segments as f32) * 0.5 * std::f32::consts::PI;
                    pos2(
                        center.x + radius * angle.cos(),
                        center.y + radius * angle.sin(),
                    )
                })
                .collect();
            // Push the arc endpoints outward past the rect edges, so the fan
            // overlaps the border area instead of stopping exactly at it.
            let first = arc.first_mut().expect("arc has points");
            if (first.x - corner.x).abs() < (first.y - corner.y).abs() {
                first.x += outward.x * pad;
            } else {
                first.y += outward.y * pad;
            }
            let last = arc.last_mut().expect("arc has points");
            if (last.x - corner.x).abs() < (last.y - corner.y).abs() {
                last.x += outward.x * pad;
            } else {
                last.y += outward.y * pad;
            }

            // Tint each vertex with the chrome color at its own height, so
            // the masks disappear into the top gradient instead of stamping
            // flat background patches over it.
            let base = mesh.vertices.len() as u32;
            mesh.colored_vertex(
                corner_out,
                chrome_color_at(root, corner_out.y, palette, top_glow),
            );
            for point in &arc {
                mesh.colored_vertex(*point, chrome_color_at(root, point.y, palette, top_glow));
            }
            for segment in 0..segments as u32 {
                mesh.add_triangle(base, base + 1 + segment, base + 2 + segment);
            }
            // The true arc (endpoints not pushed out) for the AA pass.
            let true_arc: Vec<egui::Pos2> = (0..=segments)
                .map(|segment| {
                    let angle = start_angle
                        + (segment as f32 / segments as f32) * 0.5 * std::f32::consts::PI;
                    pos2(
                        center.x + radius * angle.cos(),
                        center.y + radius * angle.sin(),
                    )
                })
                .collect();
            arc_edges.push((true_arc, chrome_color_at(root, corner.y, palette, top_glow)));
        }
        fan_painter.add(Shape::mesh(mesh));
        for (arc, colour) in arc_edges {
            fan_painter.add(Shape::line(arc, Stroke::new(1.4_f32, colour)));
        }
    }

    // Drop shadow, drawn AFTER the corner masks: filling it beforehand works
    // on internal pages but not on web pages, where the blit wipes the square
    // content rect and leaves unshadowed patches in the corners.
    // Opacity blend first, so the shadow lies on top of it.
    paint_card_opacity_blend(
        &painter,
        content_rect,
        radius,
        chrome_color_at(root, content_rect.center().y, palette, top_glow),
        chrome_opacity,
    );
    paint_card_shadow(&painter, content_rect, radius, palette);

    // Flat: a single accent-tinted edge all the way around the card — no
    // white rim light, no highlights. It also antialiases the corner masks,
    // whose mesh triangles have hard edges.
    if border {
        painter.rect_stroke(
            content_rect,
            CornerRadius::same(radius as u8),
            Stroke::new(1.2_f32, theme::mix(palette.border, palette.accent, 0.55)),
            StrokeKind::Middle,
        );
    }
}

/// Width of the sidebar, in points.
pub const SIDEBAR_DEFAULT_WIDTH: f32 = 248.0;
const SIDEBAR_MIN_WIDTH: f32 = 200.0;
const SIDEBAR_MAX_WIDTH: f32 = 380.0;
const SIDEBAR_ID: &str = "zervo_sidebar";
const SIDEBAR_MARGIN: Margin = Margin {
    left: 12,
    right: 10,
    top: 8,
    bottom: 8,
};

fn draw_sidebar(root: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    let stored = chrome
        .settings
        .sidebar_width
        .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
    Panel::left(SIDEBAR_ID)
        .resizable(true)
        .default_size(stored)
        .size_range(SIDEBAR_MIN_WIDTH..=SIDEBAR_MAX_WIDTH)
        // Transparent: the window-wide chrome base paints behind it, so the
        // top gradient continues across the sidebar boundary unbroken.
        .frame(Frame::NONE.inner_margin(SIDEBAR_MARGIN))
        .show_separator_line(false)
        .show_inside(root, |ui| sidebar_body(ui, chrome, actions));

    // Remember a manual resize, so the width outlives the session — and so an
    // autohide reveal comes back the width the user left it. Written once the
    // drag ends rather than every frame of it, which would be a disk write per
    // frame.
    let width = sidebar_width(root.ctx(), chrome.settings);
    if !root.ctx().egui_is_using_pointer() && (width - stored).abs() > 0.5 {
        chrome.settings.sidebar_width = width;
        actions.push(UiAction::PersistSettings);
    }
}

/// The sidebar's contents, drawn into whatever `ui` it is handed: the docked
/// panel, or the floating overlay an autohide reveal puts over the content.
fn sidebar_body(ui: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    let palette = chrome.palette;
    let drag = ui.interact(ui.max_rect(), ui.id().with("sidebar_drag"), Sense::drag());
    if drag.drag_started() {
        actions.push(UiAction::DragWindow);
    }

    // Bottom utility bar first, so the scroll area gets the rest.
    Panel::bottom("zervo_sidebar_bottom")
        .frame(Frame::NONE.inner_margin(Margin {
            left: 0,
            right: 0,
            top: 6,
            bottom: 0,
        }))
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if icons::icon_button(ui, Icon::Gear, 17.0, &palette, true)
                    .on_hover_text("Settings (⌘,)")
                    .clicked()
                {
                    actions.push(UiAction::OpenSettings);
                }
                let active = chrome.downloads.active_count();
                if icons::icon_button(ui, Icon::Download, 16.0, &palette, true)
                    .on_hover_text(if active > 0 {
                        format!("Downloads ({active} active)")
                    } else {
                        "Downloads".to_owned()
                    })
                    .clicked()
                {
                    actions.push(UiAction::OpenDownloads);
                }
                if active > 0 {
                    // A small accent dot marks downloads in flight.
                    let rect = ui.min_rect();
                    ui.painter().circle_filled(
                        pos2(rect.max.x - 4.0, rect.min.y + 6.0),
                        3.5,
                        palette.accent,
                    );
                    ui.ctx().request_repaint();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::icon_button(ui, Icon::Plus, 15.0, &palette, true)
                        .on_hover_text("New workspace")
                        .clicked()
                    {
                        actions.push(UiAction::NewWorkspace);
                    }
                });
            });
        });

    // ── Top row: nav icons, right of the macOS traffic lights.
    ui.horizontal(|ui| {
        #[cfg(target_os = "macos")]
        ui.add_space(58.0);

        let (can_go_back, can_go_forward, is_settings) = chrome
            .browser
            .active_tab()
            .map(|tab| {
                (
                    tab.can_go_back,
                    tab.can_go_forward,
                    tab.kind != TabKind::Web,
                )
            })
            .unwrap_or_default();

        if icons::icon_button(ui, Icon::Sidebar, 17.0, &palette, true)
            .on_hover_text(if chrome.browser.sidebar_collapsed {
                "Show sidebar"
            } else {
                "Hide sidebar"
            })
            .clicked()
        {
            actions.push(UiAction::ToggleSidebar);
        }
        ui.add_space(2.0);

        if icons::icon_button(ui, Icon::Back, 18.0, &palette, can_go_back).clicked() {
            actions.push(UiAction::Back);
        }
        if chrome.settings.show_forward_button
            && icons::icon_button(ui, Icon::Forward, 18.0, &palette, can_go_forward)
                .clicked()
        {
            actions.push(UiAction::Forward);
        }
        if chrome.settings.show_reload_button
            && icons::icon_button(ui, Icon::Reload, 17.0, &palette, !is_settings)
                .on_hover_text("Reload (⌘R)")
                .clicked()
        {
            actions.push(UiAction::Reload);
        }
    });
    ui.add_space(10.0);

    // ── Search / address pill.
    draw_address_pill(ui, chrome, actions);
    ui.add_space(12.0);

    // ── Essentials: pinned tabs as a tile grid.
    if chrome.settings.show_essentials {
        draw_essentials_grid(ui, chrome, actions);
    }

    // ── Workspaces and tab rows.
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let active_workspace = chrome.browser.active_workspace;
            let active_tab = chrome.browser.active_tab;
            let always_close = chrome.settings.always_show_tab_close;

            for (workspace_index, workspace) in chrome.browser.workspaces.iter().enumerate()
            {
                workspace_header(
                    ui,
                    workspace_index,
                    &workspace.name,
                    workspace.tabs.iter().filter(|tab| !tab.pinned).count(),
                    chrome.settings.show_tab_counts,
                    workspace_index == active_workspace,
                    &palette,
                    actions,
                );

                for tab in workspace.tabs.iter().filter(|tab| !tab.pinned) {
                    let selected = active_tab == Some(tab.id);
                    let (clicked, close_clicked) = tab_row(
                        ui,
                        TabRowStyle {
                            tab_id: tab.id,
                            title: &tab.title,
                            url: &tab.url,
                            selected,
                            loading: tab.loading,
                            is_settings: tab.kind == TabKind::Settings,
                            always_show_close: always_close,
                            compact: chrome.settings.compact_sidebar,
                        },
                        &palette,
                        chrome.favicons.get(&tab.id),
                        actions,
                    );
                    if close_clicked {
                        actions.push(UiAction::CloseTab(tab.id));
                    } else if clicked {
                        actions.push(UiAction::SelectTab(tab.id));
                        if workspace_index != active_workspace {
                            actions.push(UiAction::SelectWorkspace(workspace_index));
                        }
                    }
                }

                // Ghost "new tab" row with a plus icon, like the reference.
                let desired = vec2(ui.available_width(), 32.0);
                let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
                let hover_t = glass::ease_out(ui.ctx().animate_bool_with_time(
                    ui.id().with(("new_tab_row", workspace_index)),
                    response.hovered(),
                    0.12,
                ));
                if hover_t > 0.0 {
                    ui.painter().rect_filled(
                        rect,
                        CornerRadius::same(8),
                        palette.surface_hover.gamma_multiply(hover_t * 0.8),
                    );
                }
                let plus_rect = Rect::from_center_size(
                    pos2(rect.min.x + 18.0, rect.center().y),
                    vec2(12.0, 12.0),
                );
                icons::draw_icon(ui.painter(), plus_rect, Icon::Plus, palette.text_muted);
                ui.painter().text(
                    pos2(rect.min.x + 32.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    "New Tab",
                    FontId::proportional(13.5),
                    palette.text_muted,
                );
                if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    actions.push(UiAction::NewTab {
                        workspace: workspace_index,
                    });
                }
                ui.add_space(12.0);
            }
        });
}

// ── Autohide reveal ────────────────────────────────────────────────────────

/// How far into the window's left edge the pointer must reach to reveal a
/// collapsed sidebar.
const PEEK_TRIGGER: f32 = 14.0;
/// Slack past the revealed panel before it counts as leaving.
const PEEK_GRACE: f32 = 12.0;
/// How long a reveal lingers after the pointer leaves it.
const PEEK_LINGER: f64 = 0.25;
/// Slide-in duration.
const PEEK_SLIDE: f32 = 0.14;

#[derive(Clone, Copy, Default)]
struct PeekState {
    open: bool,
    linger_until: f64,
}

/// The sidebar's width: what the user last dragged it to this session,
/// otherwise what they left it at in a previous one, otherwise the default.
fn sidebar_width(ctx: &egui::Context, settings: &Settings) -> f32 {
    egui::PanelState::load(ctx, Id::new(SIDEBAR_ID))
        .map(|state| state.rect.width())
        .unwrap_or(settings.sidebar_width)
        .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
}

/// Decide whether a collapsed sidebar is revealed, and how wide.
///
/// Reaching the trigger strip opens it, but what *keeps* it open is the
/// pointer being anywhere over the revealed panel. Testing only the strip —
/// which is what this used to do — meant moving towards a button in the
/// sidebar dismissed the very thing being reached for, so nothing in a
/// revealed sidebar could ever be clicked.
fn sidebar_peek(root: &Ui, chrome: &ChromeContext) -> Option<(Rect, bool)> {
    let ctx = root.ctx();
    let id = Id::new("zervo_sidebar_peek");
    if !(chrome.browser.sidebar_collapsed && chrome.settings.sidebar_autohide) {
        ctx.data_mut(|data| data.remove::<PeekState>(id));
        return None;
    }

    let window = ctx.content_rect();
    let rect = Rect::from_min_size(
        window.min,
        vec2(sidebar_width(ctx, chrome.settings), window.height()),
    );
    let mut state = ctx
        .data(|data| data.get_temp::<PeekState>(id))
        .unwrap_or_default();
    // `latest_pos` goes None when the pointer leaves the window, which is
    // exactly when a reveal should start closing.
    let pointer = ctx.input(|input| input.pointer.latest_pos());
    let now = ctx.input(|input| input.time);

    let in_trigger = pointer.is_some_and(|pos| {
        (window.left()..=window.left() + PEEK_TRIGGER).contains(&pos.x)
            && window.y_range().contains(pos.y)
    });
    // Still on screen while it slides away, so moving back onto one that is
    // half gone catches it rather than watching it go.
    let visible = state.open || now < state.linger_until + PEEK_SLIDE as f64;
    let hot = Rect::from_min_max(rect.min, rect.max + vec2(PEEK_GRACE, 0.0));
    let in_panel = visible && pointer.is_some_and(|pos| hot.contains(pos));
    // Never pull it away mid-gesture: a held button, a drag, or an open menu
    // keeps it up even if the pointer strays outside.
    let busy = state.open
        && (ctx.egui_is_using_pointer() || egui::Popup::is_any_open(ctx));
    // ⌘L focuses the address pill, which lives in the sidebar — so reveal it,
    // or the shortcut does nothing and the pending focus fires later, at
    // whatever moment the pointer next happens to brush the edge.
    let wanted = in_trigger
        || in_panel
        || busy
        || chrome.browser.editing_address
        || chrome.browser.focus_address;

    if wanted {
        state.linger_until = now + PEEK_LINGER;
    }
    state.open = wanted || now < state.linger_until;
    if state.open && !wanted {
        // Nothing else will wake us to close it.
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(
            (state.linger_until - now).max(0.0),
        ));
    }
    ctx.data_mut(|data| data.insert_temp(id, state));
    Some((rect, state.open))
}

/// Paint a revealed sidebar as a floating overlay above the content.
///
/// Deliberately not a panel: a panel takes layout space, so revealing would
/// shrink the content card and resize the webview — a full page relayout —
/// every time the pointer brushed the window edge.
fn draw_sidebar_peek(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    peek: Option<(Rect, bool)>,
) -> Option<Rect> {
    let ctx = root.ctx().clone();
    let slide = Id::new("zervo_sidebar_peek_slide");
    let Some((rect, open)) = peek else {
        // Snap shut, or the next reveal starts half open.
        ctx.animate_bool_with_time(slide, false, 0.0);
        return None;
    };
    let t = glass::ease_out(ctx.animate_bool_with_time(slide, open, PEEK_SLIDE));
    if t <= 0.0 {
        return None;
    }

    let drawn = rect.translate(vec2((t - 1.0) * rect.width(), 0.0));
    egui::Area::new(Id::new("zervo_sidebar_peek_area"))
        .order(egui::Order::Foreground)
        .fixed_pos(drawn.min)
        .constrain(false)
        .show(&ctx, |ui| {
            ui.set_clip_rect(ctx.content_rect());
            // Opaque, unlike the chrome: this floats over the page, and any
            // translucency would let the text under it read through the tabs.
            paint_chrome_fill(
                ui.painter(),
                drawn,
                ctx.content_rect().top(),
                &chrome.palette,
                chrome.settings.top_glow,
                1.0,
            );
            paint_edge_shadow(
                ui.painter(),
                Rect::from_min_max(
                    pos2(drawn.max.x, drawn.min.y),
                    pos2(drawn.max.x + 12.0, drawn.max.y),
                ),
                chrome.palette.shadow,
            );

            let inner = Rect::from_min_max(
                drawn.min + vec2(SIDEBAR_MARGIN.left as f32, SIDEBAR_MARGIN.top as f32),
                drawn.max - vec2(SIDEBAR_MARGIN.right as f32, SIDEBAR_MARGIN.bottom as f32),
            );
            let mut body = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            sidebar_body(&mut body, chrome, actions);
            // Claim the whole overlay, so the pointer over it resolves to this
            // layer rather than the chrome underneath.
            ui.advance_cursor_after_rect(drawn);
        });
    Some(drawn)
}

/// A horizontal fade, for the outer edge of a floating panel.
fn paint_edge_shadow(painter: &egui::Painter, rect: Rect, colour: Color32) {
    const STEPS: usize = 12;
    let mut mesh = Mesh::default();
    for step in 0..=STEPS {
        let t = step as f32 / STEPS as f32;
        let shade = colour.gamma_multiply((1.0 - t).powi(2) * 0.9);
        let x = rect.min.x + rect.width() * t;
        mesh.colored_vertex(pos2(x, rect.min.y), shade);
        mesh.colored_vertex(pos2(x, rect.max.y), shade);
    }
    for step in 0..STEPS as u32 {
        let base = step * 2;
        mesh.add_triangle(base, base + 1, base + 3);
        mesh.add_triangle(base, base + 3, base + 2);
    }
    painter.add(Shape::mesh(mesh));
}

fn draw_address_pill(ui: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    let palette = chrome.palette;
    let loading = chrome
        .browser
        .active_tab()
        .map(|tab| tab.loading)
        .unwrap_or_default();

    let pill_id = Id::new("zervo_address_pill");
    // Focus is the slowest transition — the glow should bloom, not pop.
    let focus_t = glass::ease_out(ui.ctx().animate_bool_with_time(
        pill_id,
        chrome.browser.editing_address,
        0.22,
    ));

    let pill_height = 36.0;
    let (pill_rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), pill_height), Sense::hover());

    glass::paint(
        ui.painter(),
        pill_rect,
        &palette,
        Glass::new(10).glow(focus_t),
    );
    if focus_t > 0.0 {
        ui.painter().rect_stroke(
            pill_rect,
            CornerRadius::same(10),
            Stroke::new(
                1.0 + 0.5 * focus_t,
                palette.accent.gamma_multiply(0.85 * focus_t),
            ),
            StrokeKind::Outside,
        );
    }

    let mut inner = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(pill_rect.shrink2(vec2(12.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let search_rect = Rect::from_center_size(
        pos2(inner.max_rect().min.x + 8.0, pill_rect.center().y),
        vec2(15.0, 15.0),
    );
    icons::draw_icon(
        inner.painter(),
        search_rect,
        Icon::Search,
        palette.text_muted,
    );
    inner.add_space(22.0);

    let hint = format!("Search with {}…", chrome.settings.search_engine.label());
    // The spinner slot is always reserved so text never jumps when a load
    // starts or finishes.
    let editor = TextEdit::singleline(&mut chrome.browser.address_bar)
        .frame(Frame::NONE)
        .font(FontId::proportional(14.0))
        .text_color(palette.text)
        .vertical_align(egui::Align::Center)
        .hint_text(RichText::new(hint).color(palette.text_muted))
        .desired_width(inner.available_width() - 24.0);
    let response = inner.add(editor);
    if chrome.browser.focus_address {
        response.request_focus();
        chrome.browser.focus_address = false;
    }
    chrome.browser.editing_address = response.has_focus();
    if response.lost_focus() && inner.input(|input| input.key_pressed(Key::Enter)) {
        actions.push(UiAction::Navigate(normalize_url(
            &chrome.browser.address_bar,
            chrome.settings.search_engine,
        )));
    }
    if loading {
        inner.add(egui::Spinner::new().size(14.0).color(palette.accent));
    }
}

/// Pinned tabs as a grid of large glass tiles.
fn draw_essentials_grid(ui: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    let palette = chrome.palette;
    struct Essential {
        tab_id: TabId,
        workspace: usize,
        title: String,
        selected: bool,
        loading: bool,
    }
    let essentials: Vec<Essential> = chrome
        .browser
        .workspaces
        .iter()
        .enumerate()
        .flat_map(|(workspace_index, workspace)| {
            workspace
                .tabs
                .iter()
                .filter(|tab| tab.pinned)
                .map(move |tab| Essential {
                    tab_id: tab.id,
                    workspace: workspace_index,
                    title: tab.title.clone(),
                    selected: false,
                    loading: tab.loading,
                })
        })
        .collect();
    if essentials.is_empty() {
        return;
    }
    let active_tab = chrome.browser.active_tab;

    let columns = 3;
    let gap = 8.0;
    let tile_width = (ui.available_width() - gap * (columns as f32 - 1.0)) / columns as f32;
    let tile_height = 44.0;

    for chunk in essentials.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for essential in chunk {
                let selected = active_tab == Some(essential.tab_id) || essential.selected;
                let (rect, response) =
                    ui.allocate_exact_size(vec2(tile_width, tile_height), Sense::click());
                let hover_t = glass::ease_out(ui.ctx().animate_bool_with_time(
                    ui.id().with(("essential_hover", essential.tab_id)),
                    response.hovered(),
                    0.12,
                ));
                let sel_t = glass::ease_out(ui.ctx().animate_bool_with_time(
                    ui.id().with(("essential_sel", essential.tab_id)),
                    selected,
                    0.18,
                ));
                glass::paint(
                    ui.painter(),
                    rect,
                    &palette,
                    Glass::new(10)
                        .strength(0.55 + 0.45 * hover_t.max(sel_t))
                        .glow(0.5 * sel_t)
                        .no_shadow(),
                );
                // Favicon (or spinner) centered in the tile.
                let icon_center = rect.center();
                if essential.loading {
                    let time = ui.input(|input| input.time) as f32;
                    let points: Vec<egui::Pos2> = (0..=12)
                        .map(|step| {
                            let angle = time * 5.0 + step as f32 * 0.35;
                            pos2(
                                icon_center.x + 6.0 * angle.cos(),
                                icon_center.y + 6.0 * angle.sin(),
                            )
                        })
                        .collect();
                    ui.painter()
                        .add(Shape::line(points, Stroke::new(1.8_f32, palette.accent)));
                    ui.ctx().request_repaint();
                } else if let Some(texture) = chrome.favicons.get(&essential.tab_id) {
                    let icon_rect = Rect::from_center_size(icon_center, vec2(20.0, 20.0));
                    ui.painter().image(
                        texture.id(),
                        icon_rect,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else {
                    let icon_rect = Rect::from_center_size(icon_center, vec2(17.0, 17.0));
                    icons::draw_icon(ui.painter(), icon_rect, Icon::Globe, palette.text_muted);
                }

                let response = response
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text(&essential.title);
                response.clone().context_menu(|ui| {
                    if ui.button("Unpin").clicked() {
                        actions.push(UiAction::TogglePin(essential.tab_id));
                        ui.close();
                    }
                    if ui.button("Close").clicked() {
                        actions.push(UiAction::CloseTab(essential.tab_id));
                        ui.close();
                    }
                });
                if response.clicked() {
                    actions.push(UiAction::SelectTab(essential.tab_id));
                    actions.push(UiAction::SelectWorkspace(essential.workspace));
                }
            }
        });
        ui.add_space(gap);
    }
    ui.add_space(4.0);
}

#[expect(clippy::too_many_arguments)]
fn workspace_header(
    ui: &mut Ui,
    index: usize,
    name: &str,
    tab_count: usize,
    show_count: bool,
    active: bool,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    let desired = vec2(ui.available_width(), 28.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    if response.clicked() {
        actions.push(UiAction::SelectWorkspace(index));
    }

    let hover_t = glass::ease_out(ui.ctx().animate_bool_with_time(
        ui.id().with(("ws_header", index)),
        response.hovered(),
        0.12,
    ));
    let painter = ui.painter();
    if hover_t > 0.0 {
        painter.rect_filled(
            rect,
            CornerRadius::same(8),
            palette.surface_hover.gamma_multiply(hover_t),
        );
    }
    painter.circle_filled(
        pos2(rect.min.x + 12.0, rect.center().y),
        4.0,
        theme::workspace_color(index),
    );
    painter.text(
        pos2(rect.min.x + 26.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(13.0),
        if active {
            palette.text
        } else {
            palette.text_muted
        },
    );
    if show_count {
        painter.text(
            pos2(rect.max.x - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            tab_count.to_string(),
            FontId::proportional(11.5),
            palette.text_muted,
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand);
}

struct TabRowStyle<'a> {
    tab_id: TabId,
    title: &'a str,
    url: &'a str,
    selected: bool,
    loading: bool,
    is_settings: bool,
    always_show_close: bool,
    compact: bool,
}

/// A custom-painted tab row with animated hover/selection fills, favicon or
/// loading spinner, a close button, and a pin/close context menu. Returns
/// (row clicked, close clicked).
fn tab_row(
    ui: &mut Ui,
    style: TabRowStyle,
    palette: &Palette,
    favicon: Option<&TextureHandle>,
    actions: &mut Vec<UiAction>,
) -> (bool, bool) {
    let row_height = if style.compact { 28.0 } else { 34.0 };
    let desired = vec2(ui.available_width(), row_height);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    let close_rect =
        Rect::from_center_size(pos2(rect.max.x - 17.0, rect.center().y), vec2(20.0, 20.0));
    let close_response = ui.interact(
        close_rect,
        ui.id().with(("tab_close", style.tab_id)),
        Sense::click(),
    );

    let hovered = response.hovered() || close_response.hovered();
    // Keyed by tab identity, not layout position, so closing a tab doesn't
    // shift animation state onto its neighbor.
    let hover_t = glass::ease_out(ui.ctx().animate_bool_with_time(
        ui.id().with(("tab_hover", style.tab_id)),
        hovered && !style.selected,
        0.12,
    ));
    let select_t = glass::ease_out(ui.ctx().animate_bool_with_time(
        ui.id().with(("tab_sel", style.tab_id)),
        style.selected,
        0.18,
    ));
    let painter = ui.painter();

    // Animated fill: transparent -> hover surface -> accent-tinted glass.
    if hover_t > 0.0 {
        painter.rect_filled(
            rect,
            CornerRadius::same(8),
            palette.surface_hover.gamma_multiply(hover_t),
        );
    }
    if select_t > 0.0 {
        glass::paint(
            painter,
            rect,
            palette,
            Glass::new(8)
                .strength(select_t * 0.8)
                .glow(select_t * 0.35)
                .tint(palette.active)
                .no_shadow(),
        );
    }

    // Favicon slot: loading spinner arc, gear, favicon, or globe placeholder.
    let icon_center = pos2(rect.min.x + 19.0, rect.center().y);
    if style.loading {
        let time = ui.input(|input| input.time) as f32;
        let points: Vec<egui::Pos2> = (0..=12)
            .map(|step| {
                let angle = time * 5.0 + step as f32 * 0.35;
                pos2(
                    icon_center.x + 5.5 * angle.cos(),
                    icon_center.y + 5.5 * angle.sin(),
                )
            })
            .collect();
        painter.add(Shape::line(points, Stroke::new(1.8_f32, palette.accent)));
        ui.ctx().request_repaint();
    } else if style.is_settings {
        let icon_rect = Rect::from_center_size(icon_center, vec2(14.0, 14.0));
        icons::draw_icon(painter, icon_rect, Icon::Gear, palette.text_muted);
    } else if let Some(texture) = favicon {
        let icon_rect = Rect::from_center_size(icon_center, vec2(15.0, 15.0));
        painter.image(
            texture.id(),
            icon_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        let icon_rect = Rect::from_center_size(icon_center, vec2(14.0, 14.0));
        icons::draw_icon(painter, icon_rect, Icon::Globe, palette.text_muted);
    }

    let text_color = if style.selected || hovered {
        palette.text
    } else {
        palette.text_muted
    };
    painter.text(
        pos2(rect.min.x + 34.0, rect.center().y),
        Align2::LEFT_CENTER,
        truncate(style.title, 24),
        FontId::proportional(13.5),
        text_color,
    );

    if hovered || style.selected || style.always_show_close {
        if close_response.hovered() {
            painter.rect_filled(close_rect, CornerRadius::same(6), palette.surface_hover);
        }
        let glyph_rect = Rect::from_center_size(close_rect.center(), vec2(11.0, 11.0));
        icons::draw_icon(
            painter,
            glyph_rect,
            Icon::Close,
            if close_response.hovered() {
                palette.text
            } else {
                palette.text_muted
            },
        );
    }

    if !style.is_settings {
        response.clone().context_menu(|ui| {
            if ui.button("Pin as Essential").clicked() {
                actions.push(UiAction::TogglePin(style.tab_id));
                ui.close();
            }
            if ui.button("Close").clicked() {
                actions.push(UiAction::CloseTab(style.tab_id));
                ui.close();
            }
        });
    }

    let row_clicked = response.clicked() && !close_response.clicked();
    let close_clicked = close_response.clicked();
    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(style.url);
    (row_clicked, close_clicked)
}

/// A soft radial light blob: a triangle-fan mesh fading from `color` at the
/// center to transparent at the edge — the ambient light of the aurora page.
fn soft_blob(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: Color32) {
    const SEGMENTS: u32 = 48;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color);
    for segment in 0..=SEGMENTS {
        let angle = segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(
            pos2(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            ),
            Color32::TRANSPARENT,
        );
    }
    for segment in 0..SEGMENTS {
        mesh.add_triangle(0, 1 + segment, 2 + segment);
    }
    painter.add(Shape::mesh(mesh));
}

/// The Zervo "Z" mark, drawn as a stroked path (same geometry as the app
/// icon: straight bars, curved diagonal).
fn draw_zervo_mark(painter: &egui::Painter, center: egui::Pos2, height: f32, color: Color32) {
    let scale = height / 364.0;
    let map = |x: f32, y: f32| {
        pos2(
            center.x + (x - 512.0) * scale,
            center.y + (y - 512.0) * scale,
        )
    };
    let mut points = vec![map(318.0, 330.0), map(706.0, 330.0)];
    // Cubic bezier for the diagonal: (706,330) -> (640,432),(462,570) -> (318,694).
    for step in 1..=16 {
        let t = step as f32 / 16.0;
        let inv = 1.0 - t;
        let x = inv * inv * inv * 706.0
            + 3.0 * inv * inv * t * 640.0
            + 3.0 * inv * t * t * 462.0
            + t * t * t * 318.0;
        let y = inv * inv * inv * 330.0
            + 3.0 * inv * inv * t * 432.0
            + 3.0 * inv * t * t * 570.0
            + t * t * t * 694.0;
        points.push(map(x, y));
    }
    points.push(map(706.0, 694.0));
    painter.add(Shape::line(points, Stroke::new(height * 0.27, color)));
}

/// Deterministic pseudo-random in 0..1 from an integer seed — used to place
/// particles without pulling in an RNG (and so they stay put across frames).
fn hashed_unit(seed: u32) -> f32 {
    let x = (seed as f32 * 12.9898).sin() * 43758.547;
    x - x.floor()
}

/// Paint the selected new tab backdrop. Returns true if it animates.
fn paint_newtab_background(
    root: &Ui,
    painter: &egui::Painter,
    content_rect: Rect,
    palette: &Palette,
    background: NewTabBackground,
    base: Color32,
) -> bool {
    let time = root.input(|input| input.time) as f32;
    let span = content_rect.width().min(content_rect.height());
    let alpha = if palette.dark { 0.16 } else { 0.12 };

    match background {
        NewTabBackground::Plain => false,

        NewTabBackground::Gradient => {
            vertical_gradient(
                painter,
                content_rect,
                theme::mix(base, palette.accent, if palette.dark { 0.16 } else { 0.12 }),
                base,
            );
            false
        },

        NewTabBackground::Aurora => {
            let blobs = [
                (0.30, 0.35, 0.9, 0.055, palette.accent, 0.62),
                (0.72, 0.30, 1.4, 0.042, theme::workspace_color(1), 0.5),
                (0.55, 0.75, 2.2, 0.035, theme::workspace_color(4), 0.55),
            ];
            for (fx, fy, phase, speed, color, size) in blobs {
                let drift_x = (time * speed + phase).sin() * span * 0.10;
                let drift_y = (time * speed * 0.8 + phase * 1.7).cos() * span * 0.08;
                soft_blob(
                    painter,
                    pos2(
                        content_rect.min.x + content_rect.width() * fx + drift_x,
                        content_rect.min.y + content_rect.height() * fy + drift_y,
                    ),
                    span * size,
                    color.gamma_multiply(alpha),
                );
            }
            true
        },

        NewTabBackground::Mesh => {
            // Same soft-blob technique, but fixed in place: a static wash.
            let blobs = [
                (0.18, 0.22, palette.accent, 0.55),
                (0.82, 0.18, theme::workspace_color(1), 0.45),
                (0.30, 0.85, theme::workspace_color(2), 0.5),
                (0.88, 0.78, theme::workspace_color(4), 0.45),
            ];
            for (fx, fy, color, size) in blobs {
                soft_blob(
                    painter,
                    pos2(
                        content_rect.min.x + content_rect.width() * fx,
                        content_rect.min.y + content_rect.height() * fy,
                    ),
                    span * size,
                    color.gamma_multiply(alpha),
                );
            }
            false
        },

        NewTabBackground::Waves => {
            // Stacked sine bands, each drifting at its own speed. Every band
            // is one mesh so its top edge is a smooth curve and its body
            // fades downward.
            const SAMPLES: usize = 64;
            for band in 0..3_u32 {
                let color = match band {
                    0 => palette.accent,
                    1 => theme::workspace_color(1),
                    _ => theme::workspace_color(2),
                };
                let color = color.gamma_multiply(alpha * 0.9);
                let clear = Color32::TRANSPARENT;
                let base_y =
                    content_rect.min.y + content_rect.height() * (0.55 + band as f32 * 0.14);
                let amplitude = span * (0.05 + band as f32 * 0.015);
                let speed = 0.25 + band as f32 * 0.12;
                let wavelength = 1.6 + band as f32 * 0.7;

                let mut mesh = Mesh::default();
                for sample in 0..=SAMPLES {
                    let t = sample as f32 / SAMPLES as f32;
                    let x = content_rect.min.x + content_rect.width() * t;
                    let y = base_y
                        + (t * wavelength * std::f32::consts::TAU + time * speed).sin() * amplitude;
                    mesh.colored_vertex(pos2(x, y), color);
                    mesh.colored_vertex(pos2(x, content_rect.max.y), clear);
                }
                for sample in 0..SAMPLES as u32 {
                    let index = sample * 2;
                    mesh.add_triangle(index, index + 1, index + 3);
                    mesh.add_triangle(index, index + 3, index + 2);
                }
                painter.add(Shape::mesh(mesh));
            }
            true
        },

        NewTabBackground::Particles => {
            for index in 0..46_u32 {
                let fx = hashed_unit(index * 3 + 1);
                let fy = hashed_unit(index * 7 + 5);
                let speed = 0.02 + hashed_unit(index * 11 + 3) * 0.05;
                let size = 1.2 + hashed_unit(index * 13 + 9) * 2.4;
                // Drift upward, wrapping around the top edge.
                let y_wrapped = (fy - time * speed).rem_euclid(1.0);
                let sway = (time * speed * 6.0 + fx * 10.0).sin() * span * 0.012;
                let color = if index % 5 == 0 {
                    theme::workspace_color(1)
                } else {
                    palette.accent
                };
                painter.circle_filled(
                    pos2(
                        content_rect.min.x + content_rect.width() * fx + sway,
                        content_rect.min.y + content_rect.height() * y_wrapped,
                    ),
                    size,
                    color.gamma_multiply(0.10 + hashed_unit(index * 17 + 2) * 0.22),
                );
            }
            true
        },

        NewTabBackground::Grid => {
            let step = 42.0;
            let line = palette
                .accent
                .gamma_multiply(if palette.dark { 0.10 } else { 0.13 });
            let stroke = Stroke::new(1.0_f32, line);
            let mut x = content_rect.min.x + step;
            while x < content_rect.max.x {
                painter.line_segment(
                    [pos2(x, content_rect.min.y), pos2(x, content_rect.max.y)],
                    stroke,
                );
                x += step;
            }
            let mut y = content_rect.min.y + step;
            while y < content_rect.max.y {
                painter.line_segment(
                    [pos2(content_rect.min.x, y), pos2(content_rect.max.x, y)],
                    stroke,
                );
                y += step;
            }
            false
        },
    }
}

/// Time-of-day greeting, or the user's own message when they set one.
fn newtab_greeting(settings: &Settings) -> String {
    if !settings.newtab_message.trim().is_empty() {
        return settings.newtab_message.trim().to_owned();
    }
    let hour = chrono::Local::now().hour();
    match hour {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        18..=21 => "Good evening",
        _ => "Good night",
    }
    .to_owned()
}

/// Returns true while the ambient animation is running (caller schedules
/// timed repaints).
fn draw_newtab_page(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    content_rect: Rect,
    actions: &mut Vec<UiAction>,
) -> bool {
    let palette = chrome.palette;
    let painter = root
        .ctx()
        .layer_painter(egui::LayerId::background())
        .with_clip_rect(content_rect);

    // Deep base: darker than the chrome in dark mode, airy in light mode.
    let base = if palette.dark {
        theme::mix(palette.bg, Color32::BLACK, 0.35)
    } else {
        theme::mix(palette.bg, Color32::WHITE, 0.45)
    };
    painter.rect_filled(
        content_rect,
        CornerRadius::same(theme::CONTENT_RADIUS as u8),
        base,
    );

    let ambient = paint_newtab_background(
        root,
        &painter,
        content_rect,
        &palette,
        chrome.settings.new_tab_background,
        base,
    );

    // ── Widgets, stacked around the centre of the page.
    let center = content_rect.center();
    let text_color = if palette.dark {
        Color32::from_white_alpha(228)
    } else {
        palette.text
    };

    if chrome.settings.newtab_clock {
        let now = chrono::Local::now();
        painter.text(
            pos2(center.x, content_rect.min.y + content_rect.height() * 0.17),
            Align2::CENTER_CENTER,
            now.format("%H:%M").to_string(),
            FontId::proportional(58.0),
            text_color,
        );
        painter.text(
            pos2(
                center.x,
                content_rect.min.y + content_rect.height() * 0.17 + 44.0,
            ),
            Align2::CENTER_CENTER,
            now.format("%A, %e %B").to_string().replace("  ", " "),
            FontId::proportional(14.0),
            palette.text_muted,
        );
    }

    if chrome.settings.newtab_greeting {
        painter.text(
            pos2(center.x, center.y - 132.0),
            Align2::CENTER_CENTER,
            newtab_greeting(chrome.settings),
            FontId::proportional(21.0),
            text_color,
        );
    }

    if chrome.settings.newtab_logo {
        let mark_color = if palette.dark {
            Color32::from_white_alpha(200)
        } else {
            theme::mix(palette.text, palette.accent, 0.35)
        };
        draw_zervo_mark(&painter, pos2(center.x, center.y - 66.0), 62.0, mark_color);
    }

    if chrome.settings.newtab_search {
        let pill_width = (content_rect.width() - 120.0).clamp(240.0, 520.0);
        let pill_rect =
            Rect::from_center_size(pos2(center.x, center.y + 12.0), vec2(pill_width, 46.0));
        glass::paint(root.painter(), pill_rect, &palette, Glass::new(14));

        let mut inner = root.new_child(
            egui::UiBuilder::new()
                .max_rect(pill_rect.shrink2(vec2(16.0, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let search_rect = Rect::from_center_size(
            pos2(inner.max_rect().min.x + 8.0, pill_rect.center().y),
            vec2(17.0, 17.0),
        );
        icons::draw_icon(
            inner.painter(),
            search_rect,
            Icon::Search,
            palette.text_muted,
        );
        inner.add_space(24.0);
        let hint = format!(
            "Search with {} or enter address…",
            chrome.settings.search_engine.label()
        );
        let editor = TextEdit::singleline(&mut chrome.browser.newtab_query)
            .frame(Frame::NONE)
            .font(FontId::proportional(15.0))
            .text_color(palette.text)
            .vertical_align(egui::Align::Center)
            .hint_text(RichText::new(hint).color(palette.text_muted))
            .desired_width(inner.available_width() - 8.0);
        let response = inner.add(editor);
        if response.lost_focus()
            && inner.input(|input| input.key_pressed(Key::Enter))
            && !chrome.browser.newtab_query.trim().is_empty()
        {
            let target = normalize_url(&chrome.browser.newtab_query, chrome.settings.search_engine);
            chrome.browser.newtab_query.clear();
            actions.push(UiAction::Navigate(target));
        }
    }

    if chrome.settings.newtab_quick_links {
        draw_newtab_quick_links(root, chrome, content_rect, actions);
    }

    ambient
}

/// Pinned tabs as shortcut tiles under the search pill.
fn draw_newtab_quick_links(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    content_rect: Rect,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    struct Link {
        tab_id: TabId,
        workspace: usize,
        title: String,
    }
    let links: Vec<Link> = chrome
        .browser
        .workspaces
        .iter()
        .enumerate()
        .flat_map(|(workspace, space)| {
            space
                .tabs
                .iter()
                .filter(|tab| tab.pinned)
                .map(move |tab| Link {
                    tab_id: tab.id,
                    workspace,
                    title: tab.title.clone(),
                })
        })
        .take(8)
        .collect();
    if links.is_empty() {
        return;
    }

    let tile = vec2(76.0, 72.0);
    let gap = 10.0;
    let total = links.len() as f32 * tile.x + (links.len() as f32 - 1.0) * gap;
    let row = Rect::from_center_size(
        pos2(content_rect.center().x, content_rect.center().y + 110.0),
        vec2(total, tile.y),
    );
    if row.min.x < content_rect.min.x + 12.0 {
        return; // Not enough room; the sidebar essentials still have them.
    }

    let mut ui = root.new_child(
        egui::UiBuilder::new()
            .max_rect(row)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    ui.spacing_mut().item_spacing.x = gap;
    for link in links {
        let (rect, response) = ui.allocate_exact_size(tile, Sense::click());
        let hover = glass::ease_out(ui.ctx().animate_bool_with_time(
            ui.id().with(("quick_link", link.tab_id)),
            response.hovered(),
            0.12,
        ));
        glass::paint(
            ui.painter(),
            rect,
            &palette,
            Glass::new(12).strength(0.5 + 0.5 * hover).no_shadow(),
        );
        let icon_center = pos2(rect.center().x, rect.min.y + 26.0);
        if let Some(texture) = chrome.favicons.get(&link.tab_id) {
            ui.painter().image(
                texture.id(),
                Rect::from_center_size(icon_center, vec2(22.0, 22.0)),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            icons::draw_icon(
                ui.painter(),
                Rect::from_center_size(icon_center, vec2(19.0, 19.0)),
                Icon::Globe,
                palette.text_muted,
            );
        }
        ui.painter().text(
            pos2(rect.center().x, rect.max.y - 14.0),
            Align2::CENTER_CENTER,
            truncate(&link.title, 10),
            FontId::proportional(11.0),
            palette.text_muted,
        );
        if response
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(&link.title)
            .clicked()
        {
            actions.push(UiAction::SelectTab(link.tab_id));
            actions.push(UiAction::SelectWorkspace(link.workspace));
        }
    }
}

/// When the sidebar is collapsed, a small floating control stays reachable so
/// the chrome can be brought back (and the traffic lights keep their room).
fn draw_collapsed_sidebar_handle(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let window = root.ctx().content_rect();
    // Sits to the right of the macOS traffic lights.
    let origin = pos2(window.min.x + 84.0, window.min.y + 8.0);
    let rect = Rect::from_min_size(origin, vec2(112.0, 30.0));

    let mut ui = root.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    ui.spacing_mut().item_spacing.x = 2.0;
    if icons::icon_button(&mut ui, Icon::Sidebar, 17.0, &palette, true)
        .on_hover_text("Show sidebar")
        .clicked()
    {
        actions.push(UiAction::ToggleSidebar);
    }
    let (back, forward) = chrome
        .browser
        .active_tab()
        .map(|tab| (tab.can_go_back, tab.can_go_forward))
        .unwrap_or_default();
    if icons::icon_button(&mut ui, Icon::Back, 17.0, &palette, back).clicked() {
        actions.push(UiAction::Back);
    }
    if icons::icon_button(&mut ui, Icon::Forward, 17.0, &palette, forward).clicked() {
        actions.push(UiAction::Forward);
    }
    // Reserve the strip so the content card starts below the handle.
    root.allocate_rect(rect, Sense::hover());
}

/// zervo://downloads — the download manager.
fn draw_downloads_page(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    content_rect: Rect,
    actions: &mut Vec<UiAction>,
) {
    use crate::downloads::{DownloadState, format_bytes};

    let palette = chrome.palette;
    root.ctx()
        .layer_painter(egui::LayerId::background())
        .rect_filled(
            content_rect,
            CornerRadius::same(theme::CONTENT_RADIUS as u8),
            palette.surface,
        );

    let mut pane = root.new_child(
        egui::UiBuilder::new()
            .max_rect(content_rect.shrink2(vec2(6.0, 10.0)))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let ui = &mut pane;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let column = (ui.available_width() - 60.0).clamp(280.0, 640.0);
            let margin = ((ui.available_width() - column) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_width(column);
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Downloads")
                                .size(21.0)
                                .strong()
                                .color(palette.text),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !chrome.downloads.items.is_empty()
                                && icons::icon_button(ui, Icon::Trash, 15.0, &palette, true)
                                    .on_hover_text("Clear finished")
                                    .clicked()
                            {
                                actions.push(UiAction::ClearDownloads);
                            }
                        });
                    });
                    ui.label(
                        RichText::new(
                            crate::downloads::downloads_dir()
                                .to_string_lossy()
                                .to_string(),
                        )
                        .size(11.5)
                        .color(palette.text_muted),
                    );
                    ui.add_space(14.0);

                    if chrome.downloads.items.is_empty() {
                        ui.add_space(40.0);
                        ui.vertical_centered(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                            icons::draw_icon(
                                ui.painter(),
                                rect,
                                Icon::FileArrowDown,
                                palette.text_muted.gamma_multiply(0.6),
                            );
                            ui.label(
                                RichText::new("No downloads yet")
                                    .size(14.0)
                                    .color(palette.text_muted),
                            );
                            ui.label(
                                RichText::new("Files you download will appear here.")
                                    .size(11.5)
                                    .color(palette.text_muted),
                            );
                        });
                        return;
                    }

                    for item in chrome.downloads.items.iter().rev() {
                        let height = 62.0;
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), height),
                            Sense::hover(),
                        );
                        glass::paint(
                            ui.painter(),
                            rect,
                            &palette,
                            Glass::new(10).strength(0.7).no_shadow(),
                        );

                        let icon_rect = Rect::from_center_size(
                            pos2(rect.min.x + 26.0, rect.center().y),
                            vec2(20.0, 20.0),
                        );
                        let (icon, tint) = match &item.state {
                            DownloadState::Running => (Icon::Download, palette.accent),
                            DownloadState::Complete => (Icon::CheckCircle, palette.accent),
                            DownloadState::Cancelled => (Icon::XCircle, palette.text_muted),
                            DownloadState::Failed(_) => (Icon::Warning, palette.text_muted),
                        };
                        icons::draw_icon(ui.painter(), icon_rect, icon, tint);

                        ui.painter().text(
                            pos2(rect.min.x + 48.0, rect.min.y + 20.0),
                            Align2::LEFT_CENTER,
                            truncate(&item.filename, 42),
                            FontId::proportional(13.5),
                            palette.text,
                        );
                        let status = match &item.state {
                            DownloadState::Running => match item.total {
                                Some(total) => format!(
                                    "{} of {}",
                                    format_bytes(item.received),
                                    format_bytes(total)
                                ),
                                None => format_bytes(item.received),
                            },
                            DownloadState::Complete => format_bytes(item.received),
                            DownloadState::Cancelled => "Cancelled".to_owned(),
                            DownloadState::Failed(error) => format!("Failed — {error}"),
                        };
                        ui.painter().text(
                            pos2(rect.min.x + 48.0, rect.min.y + 40.0),
                            Align2::LEFT_CENTER,
                            truncate(&status, 52),
                            FontId::proportional(11.5),
                            palette.text_muted,
                        );
                        ui.painter().text(
                            pos2(rect.max.x - 86.0, rect.min.y + 20.0),
                            Align2::RIGHT_CENTER,
                            truncate(
                                url::Url::parse(&item.url)
                                    .ok()
                                    .and_then(|parsed| parsed.host_str().map(str::to_owned))
                                    .unwrap_or_default()
                                    .as_str(),
                                24,
                            ),
                            FontId::proportional(11.0),
                            palette.text_muted,
                        );

                        // Progress bar for running transfers.
                        if item.state == DownloadState::Running {
                            let track = Rect::from_min_size(
                                pos2(rect.min.x + 48.0, rect.max.y - 12.0),
                                vec2(rect.width() - 110.0, 4.0),
                            );
                            ui.painter().rect_filled(
                                track,
                                CornerRadius::same(2),
                                palette.surface_hover,
                            );
                            if let Some(fraction) = item.fraction() {
                                let filled = Rect::from_min_size(
                                    track.min,
                                    vec2(track.width() * fraction, track.height()),
                                );
                                ui.painter().rect_filled(
                                    filled,
                                    CornerRadius::same(2),
                                    palette.accent,
                                );
                            }
                            ui.ctx().request_repaint();
                        }

                        // Row actions.
                        let mut controls = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(Rect::from_min_size(
                                    pos2(rect.max.x - 78.0, rect.center().y - 14.0),
                                    vec2(70.0, 28.0),
                                ))
                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        );
                        match item.state {
                            DownloadState::Running => {
                                if icons::icon_button(
                                    &mut controls,
                                    Icon::XCircle,
                                    15.0,
                                    &palette,
                                    true,
                                )
                                .on_hover_text("Cancel")
                                .clicked()
                                {
                                    actions.push(UiAction::CancelDownload(item.id));
                                }
                            },
                            DownloadState::Complete => {
                                if icons::icon_button(
                                    &mut controls,
                                    Icon::FolderOpen,
                                    15.0,
                                    &palette,
                                    true,
                                )
                                .on_hover_text("Reveal in Finder")
                                .clicked()
                                {
                                    actions.push(UiAction::RevealDownload(item.id));
                                }
                                if icons::icon_button(
                                    &mut controls,
                                    Icon::ExternalLink,
                                    15.0,
                                    &palette,
                                    true,
                                )
                                .on_hover_text("Open")
                                .clicked()
                                {
                                    actions.push(UiAction::OpenDownload(item.id));
                                }
                            },
                            _ => {
                                if icons::icon_button(
                                    &mut controls,
                                    Icon::Trash,
                                    15.0,
                                    &palette,
                                    true,
                                )
                                .on_hover_text("Remove")
                                .clicked()
                                {
                                    actions.push(UiAction::RemoveDownload(item.id));
                                }
                            },
                        }
                        ui.add_space(8.0);
                    }
                });
            });
        });
}

fn draw_settings_page(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    content_rect: Rect,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let painter = root.ctx().layer_painter(egui::LayerId::background());
    let card_radius = CornerRadius::same(theme::CONTENT_RADIUS as u8);
    painter.rect_filled(content_rect, card_radius, palette.surface);

    // ── Left navigation column, Chrome-style: categories on the left, the
    // selected category's panel on the right.
    let nav_width = 188.0_f32.min(content_rect.width() * 0.34);
    let nav_rect = Rect::from_min_size(content_rect.min, vec2(nav_width, content_rect.height()));
    painter.rect_filled(
        nav_rect,
        CornerRadius {
            nw: theme::CONTENT_RADIUS as u8,
            sw: theme::CONTENT_RADIUS as u8,
            ne: 0,
            se: 0,
        },
        theme::mix(palette.surface, palette.bg, 0.75),
    );

    let mut nav_ui = root.new_child(
        egui::UiBuilder::new()
            .max_rect(nav_rect.shrink2(vec2(10.0, 14.0)))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    {
        let ui = &mut nav_ui;
        ui.label(
            RichText::new("Settings")
                .size(17.0)
                .strong()
                .color(palette.text),
        );
        ui.add_space(10.0);
        for section in crate::state::SettingsSection::ALL {
            let selected = chrome.browser.settings_section == section;
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
            let hover_t = glass::ease_out(ui.ctx().animate_bool_with_time(
                ui.id().with(("settings_nav", section.label())),
                response.hovered() && !selected,
                0.12,
            ));
            let select_t = glass::ease_out(ui.ctx().animate_bool_with_time(
                ui.id().with(("settings_nav_sel", section.label())),
                selected,
                0.18,
            ));
            if hover_t > 0.0 {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(8),
                    palette.surface_hover.gamma_multiply(hover_t),
                );
            }
            if select_t > 0.0 {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(8),
                    palette.active.gamma_multiply(select_t),
                );
            }
            ui.painter().text(
                pos2(rect.min.x + 12.0, rect.center().y),
                Align2::LEFT_CENTER,
                section.label(),
                FontId::proportional(13.5),
                if selected {
                    palette.text
                } else {
                    palette.text_muted
                },
            );
            if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                chrome.browser.settings_section = section;
            }
        }
    }

    // ── Content pane: scrolls independently, scrollbar at the card's edge.
    let pane_rect = Rect::from_min_max(pos2(nav_rect.max.x, content_rect.min.y), content_rect.max)
        .shrink2(vec2(6.0, 10.0));
    let mut pane_ui = root.new_child(
        egui::UiBuilder::new()
            .max_rect(pane_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let ui = &mut pane_ui;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let column = (ui.available_width() - 40.0).clamp(240.0, 520.0);
            let margin = ((ui.available_width() - column) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_width(column);
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new(chrome.browser.settings_section.label())
                            .size(21.0)
                            .strong()
                            .color(palette.text),
                    );
                    ui.add_space(12.0);
                    match chrome.browser.settings_section {
                        crate::state::SettingsSection::Appearance => {
                            settings_appearance(ui, chrome, &palette, actions)
                        },
                        crate::state::SettingsSection::General => {
                            settings_general(ui, chrome, &palette, actions)
                        },
                        crate::state::SettingsSection::Customize => {
                            settings_customize(ui, chrome, &palette, actions)
                        },
                        crate::state::SettingsSection::About => settings_about(ui, &palette),
                    }
                    ui.add_space(18.0);
                });
            });
        });
}

fn settings_appearance(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    settings_section(ui, palette, "Theme", |ui| {
        let labels: Vec<&str> = ThemeMode::ALL.iter().map(|mode| mode.label()).collect();
        let current = ThemeMode::ALL
            .iter()
            .position(|mode| *mode == chrome.settings.theme)
            .unwrap_or(0);
        if let Some(index) = widgets::segmented(ui, current, &labels, palette) {
            chrome.settings.theme = ThemeMode::ALL[index];
            actions.push(UiAction::SettingsChanged);
        }
        ui.add_space(2.0);
        ui.label(
            RichText::new("Auto follows the system appearance and day/night cycle.")
                .size(11.5)
                .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, "Accent color", |ui| {
        ui.horizontal(|ui| {
            for accent in AccentColor::ALL {
                let selected = chrome.settings.accent == accent;
                let color = accent.color(palette.dark);
                let (rect, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                let center = rect.center();
                let t = ui.ctx().animate_bool(response.id, selected);
                ui.painter().circle_filled(center, 10.0 + 2.0 * t, color);
                if t > 0.0 {
                    ui.painter().circle_stroke(
                        center,
                        14.0,
                        Stroke::new(1.5 + 0.5 * t, palette.text),
                    );
                }
                if response
                    .clone()
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text(accent.label())
                    .clicked()
                    && !selected
                {
                    chrome.settings.accent = accent;
                    actions.push(UiAction::SettingsChanged);
                }
            }
        });
    });

    settings_section(ui, palette, "App icon", |ui| {
        let icons = AppIcon::ALL;
        let labels: Vec<&str> = icons.iter().map(|icon| icon.label()).collect();
        let current = icons
            .iter()
            .position(|icon| *icon == chrome.settings.app_icon)
            .unwrap_or(0);
        if let Some(index) = widgets::segmented(ui, current, &labels, palette) {
            chrome.settings.app_icon = icons[index];
            actions.push(UiAction::SettingsChanged);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Transparent lets the desktop show through the icon's backdrop. \
                 Changes apply to the Dock immediately.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, "Window", |ui| {
        ui.label(
            RichText::new("Top glow strip")
                .size(13.0)
                .color(palette.text),
        );
        if widgets::slider(ui, &mut chrome.settings.top_glow, 0.0..=1.0, palette) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.label(
            RichText::new(if chrome.settings.top_glow <= 0.0 {
                "Off — flat chrome, no light across the top.".to_owned()
            } else {
                format!(
                    "{:.0}% — accent-tinted light across the top of the window.",
                    chrome.settings.top_glow * 100.0
                )
            })
            .size(11.5)
            .color(palette.text_muted),
        );
        ui.add_space(10.0);

        if widgets::toggle(
            ui,
            &mut chrome.settings.content_border,
            "Outline around content",
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.label(
            RichText::new("Accent-tinted edge framing the web page.")
                .size(11.5)
                .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, "Transparency", |ui| {
        ui.label(
            RichText::new("Chrome opacity")
                .size(12.0)
                .color(palette.text_muted),
        );
        if widgets::slider(ui, &mut chrome.settings.chrome_opacity, 0.35..=1.0, palette) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.label(
            RichText::new(format!(
                "{:.0}% — the web page itself always stays opaque.",
                chrome.settings.chrome_opacity * 100.0
            ))
            .size(11.5)
            .color(palette.text_muted),
        );
    });
}

fn settings_general(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    settings_section(ui, palette, "Startup", |ui| {
        ui.label(
            RichText::new("Homepage")
                .size(12.0)
                .color(palette.text_muted),
        );
        let response = ui.add(
            TextEdit::singleline(&mut chrome.settings.homepage)
                .font(FontId::proportional(13.0))
                .desired_width(f32::INFINITY),
        );
        if response.lost_focus() {
            actions.push(UiAction::SettingsChanged);
        }
    });

    settings_section(ui, palette, "Search", |ui| {
        ui.label(
            RichText::new("Search engine")
                .size(12.0)
                .color(palette.text_muted),
        );
        egui::ComboBox::from_id_salt("search_engine")
            .selected_text(chrome.settings.search_engine.label())
            .width(200.0)
            .show_ui(ui, |ui| {
                for engine in SearchEngine::ALL {
                    if ui
                        .selectable_value(
                            &mut chrome.settings.search_engine,
                            engine,
                            engine.label(),
                        )
                        .changed()
                    {
                        actions.push(UiAction::SettingsChanged);
                    }
                }
            });
    });

    settings_section(ui, palette, "New tabs", |ui| {
        ui.label(
            RichText::new("Open with")
                .size(12.0)
                .color(palette.text_muted),
        );
        let pages = [NewTabPage::ZervoPage, NewTabPage::Homepage];
        let current = pages
            .iter()
            .position(|page| *page == chrome.settings.new_tab_page)
            .unwrap_or(0);
        if let Some(index) = widgets::segmented(ui, current, &["Zervo page", "Homepage"], palette) {
            chrome.settings.new_tab_page = pages[index];
            actions.push(UiAction::SettingsChanged);
        }
    });

    settings_section(ui, palette, "New tab background", |ui| {
        let all = NewTabBackground::ALL;
        let current = all
            .iter()
            .position(|background| *background == chrome.settings.new_tab_background);
        // Two rows, so the labels stay readable in a narrow pane.
        let (first, second) = all.split_at(4);
        let first_labels: Vec<&str> = first.iter().map(|b| b.label()).collect();
        let second_labels: Vec<&str> = second.iter().map(|b| b.label()).collect();
        if let Some(index) =
            widgets::segmented(ui, current.unwrap_or(usize::MAX), &first_labels, palette)
        {
            chrome.settings.new_tab_background = first[index];
            actions.push(UiAction::SettingsChanged);
        }
        ui.add_space(6.0);
        let offset_selected = current
            .filter(|index| *index >= first.len())
            .map(|index| index - first.len())
            .unwrap_or(usize::MAX);
        if let Some(index) = widgets::segmented(ui, offset_selected, &second_labels, palette) {
            chrome.settings.new_tab_background = second[index];
            actions.push(UiAction::SettingsChanged);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(if chrome.settings.new_tab_background.animated() {
                "Animated — repaints at ~30fps while a new tab is open."
            } else {
                "Static — costs nothing while idle."
            })
            .size(11.5)
            .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, "New tab widgets", |ui| {
        for (value, label) in [
            (&mut chrome.settings.newtab_clock, "Clock and date"),
            (&mut chrome.settings.newtab_greeting, "Greeting"),
            (&mut chrome.settings.newtab_logo, "Zervo mark"),
            (&mut chrome.settings.newtab_search, "Search box"),
            (
                &mut chrome.settings.newtab_quick_links,
                "Quick links (pinned tabs)",
            ),
        ] {
            if widgets::toggle(ui, value, label, palette) {
                actions.push(UiAction::SettingsChanged);
            }
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new("Custom greeting")
                .size(12.0)
                .color(palette.text_muted),
        );
        let response = ui.add(
            TextEdit::singleline(&mut chrome.settings.newtab_message)
                .font(FontId::proportional(13.0))
                .hint_text("Leave empty for the time of day")
                .desired_width(f32::INFINITY),
        );
        if response.lost_focus() {
            actions.push(UiAction::SettingsChanged);
        }
    });
}

fn settings_customize(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
) {
    settings_section(ui, palette, "Toolbar", |ui| {
        for (value, label) in [
            (
                &mut chrome.settings.show_forward_button,
                "Show forward button",
            ),
            (
                &mut chrome.settings.show_reload_button,
                "Show reload button",
            ),
        ] {
            if widgets::toggle(ui, value, label, palette) {
                actions.push(UiAction::SettingsChanged);
            }
        }
    });

    settings_section(ui, palette, "Sidebar", |ui| {
        for (value, label) in [
            (
                &mut chrome.settings.show_essentials,
                "Show pinned essentials grid",
            ),
            (
                &mut chrome.settings.show_tab_counts,
                "Show workspace tab counts",
            ),
            (
                &mut chrome.settings.always_show_tab_close,
                "Always show tab close buttons",
            ),
            (&mut chrome.settings.compact_sidebar, "Compact rows"),
            (
                &mut chrome.settings.sidebar_autohide,
                "Reveal a hidden sidebar on hover",
            ),
        ] {
            if widgets::toggle(ui, value, label, palette) {
                actions.push(UiAction::SettingsChanged);
            }
        }
    });
}

fn settings_about(ui: &mut Ui, palette: &Palette) {
    settings_section(ui, palette, "Zervo", |ui| {
        ui.label(
            RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                .size(13.0)
                .color(palette.text),
        );
        ui.label(
            RichText::new("Rendering engine: Servo (WebRender GPU compositing; WebGPU via Metal).")
                .size(12.0)
                .color(palette.text_muted),
        );
        ui.label(
            RichText::new("Chrome: egui on winit, painted against Servo's GL context.")
                .size(12.0)
                .color(palette.text_muted),
        );
    });
}

fn settings_section(
    ui: &mut Ui,
    palette: &Palette,
    title: &str,
    add_contents: impl FnOnce(&mut Ui),
) {
    ui.label(
        RichText::new(title.to_uppercase())
            .size(10.5)
            .strong()
            .color(palette.text_muted),
    );
    ui.add_space(4.0);
    Frame::NONE.inner_margin(Margin::same(14)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        // Reserve a slot, lay out the contents, then backfill the glass
        // card sized to what was actually laid out (max_rect here spans
        // the whole remaining scroll viewport, not the section).
        let placeholder = ui.painter().add(Shape::Noop);
        add_contents(ui);
        let card_rect = ui.min_rect().expand(14.0);
        ui.painter().set(
            placeholder,
            Shape::Vec(glass::shapes(
                card_rect,
                palette,
                Glass::new(10).strength(0.8),
            )),
        );
    });
    ui.add_space(14.0);
}

/// Turn address-bar input into a loadable URL: pass URLs through, prefix bare
/// domains with https://, and send everything else to the chosen search engine.
pub fn normalize_url(input: &str, search_engine: SearchEngine) -> String {
    let input = input.trim();
    if input.starts_with("http://")
        || input.starts_with("https://")
        || input.starts_with("about:")
        || input.starts_with("zervo://")
    {
        input.to_owned()
    } else if input.contains('.') && !input.contains(' ') {
        format!("https://{input}")
    } else {
        search_engine.query_url(&url_escape(input))
    }
}

fn url_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            },
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_owned()
    } else {
        let truncated: String = text.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}
