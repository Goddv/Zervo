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

use crate::glass::{self, Glass};
use crate::icons::{self, Icon};
use crate::settings::{AppIcon, NewTabBackground, NewTabPage, SearchEngine, Settings};
use crate::state::{BrowserState, TabId, TabKind};
use crate::theme::{self, AccentColor, Palette, Surface, ThemeMode, Tier};
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
    /// Dragged in the sidebar: put this tab at `index` in `workspace`.
    MoveTab {
        tab: TabId,
        workspace: usize,
        index: usize,
    },
    /// Dropped one tab onto another: the two become a workspace, and the
    /// sidebar opens its name for editing.
    GroupTabs {
        onto: TabId,
        dragged: TabId,
    },
    RenameWorkspace(usize, String),
    NewWorkspace,
    SelectWorkspace(usize),
    OpenSettings,
    OpenDownloads,
    OpenHistory,
    /// Add or remove the active page from favourites.
    ToggleFavourite,
    RemoveFavourite(String),
    RenameFavourite(String, String),
    ForgetVisit(usize),
    ClearHistory,
    AddWidget(crate::dashboard::WidgetKind),
    RemoveWidget(usize),
    SwapWidgets(usize, usize),
    MoveNavItem {
        item: NavItem,
        side: NavSide,
        index: usize,
    },
    RemoveNavItem(NavItem),
    AddNavItem(NavItem),
    PlaceWidget {
        index: usize,
        col: u8,
        row: u8,
    },
    ResizeWidget(usize, crate::dashboard::Size),
    MediaAction(servo::MediaSessionActionType),
    SavePassword,
    RemovePassword(String, String),
    ExportPasswords,
    ImportPasswords,
    ToggleSidebar,
    CancelDownload(u64),
    RemoveDownload(u64),
    RevealDownload(u64),
    OpenDownload(u64),
    ClearDownloads,
    RestartDownload(u64),
    /// Fetch another wallpaper from whichever source is chosen.
    ShuffleWallpaper,
    /// Choose a picture from this machine instead.
    PickWallpaper,
    /// Put every dragged-into-place thing back where it started.
    ResetLayout,
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
    /// True while a page-initiated dialog or menu is up; the chrome owns the
    /// pointer over the content area for as long as it is.
    pub controls_open: bool,
    /// True while an ambient animation is on screen (aurora new-tab page):
    /// the caller schedules ~30fps timed wakes instead of the idle cadence.
    pub ambient: bool,
}

pub struct ChromeContext<'a> {
    pub browser: &'a mut BrowserState,
    /// Page-initiated dialogs, pickers and context menus awaiting an answer.
    pub controls: &'a mut crate::controls::Controls,
    /// Favourites and history.
    pub library: &'a mut crate::library::Library,
    /// Saved logins.
    pub vault: &'a mut crate::passwords::Vault,
    /// What the page is playing.
    pub media: &'a crate::dashboard::Media,
    pub settings: &'a mut Settings,
    pub palette: Palette,
    pub favicons: &'a HashMap<TabId, TextureHandle>,
    pub downloads: &'a crate::downloads::DownloadManager,
    /// The new tab page's photograph, and whatever is known about it.
    pub wallpaper: crate::wallpaper::View<'a>,
    /// Where to take the blurred copy of the page, when one is due this frame.
    ///
    /// Handed to the page rather than taken by the window, because only the
    /// page knows the moment it has finished its background and not yet drawn
    /// anything that sits on it.
    pub capture: Option<crate::backdrop::Capture>,
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
        chrome.palette.chrome_tint(),
    );

    // Autohide reveal: decided before the sidebar is drawn, but painted last
    // as a floating overlay. The collapsed handle stays allocated underneath
    // either way, so revealing never reflows the content card — and so never
    // resizes the webview, which would relayout the page.
    let peek = sidebar_peek(root, chrome);
    if chrome.browser.sidebar_collapsed {
        // Navigation moves into a bar across the top, so the sidebar has
        // nothing to hold but tabs.
        draw_navbar(root, chrome, &mut actions);
    } else {
        draw_sidebar(root, chrome, &mut actions);
    }

    let outer = root.available_rect_before_wrap();
    // With the bar up, the card starts where the bar ends: the gap under the
    // pill is then the bar's own, and matches the gap above it.
    let top_margin = if chrome.browser.sidebar_collapsed {
        0.0
    } else {
        theme::CONTENT_MARGIN
    };
    let content_rect = paint_content_backdrop(root, outer, &chrome.palette, top_margin);

    let active_kind = chrome.browser.active_tab().map(|tab| tab.kind);
    let mut ambient = false;
    match active_kind {
        Some(TabKind::Settings) => {
            draw_settings_page(root, chrome, content_rect, &mut actions);
        },
        Some(TabKind::NewTab) => {
            ambient = crate::newtab::draw(root, chrome, content_rect, &mut actions);
        },
        Some(TabKind::Downloads) => {
            draw_downloads_page(root, chrome, content_rect, &mut actions);
        },
        Some(TabKind::History) => {
            draw_history_page(root, chrome, content_rect, &mut actions);
        },
        _ => {},
    }
    let settings_open = matches!(
        active_kind,
        Some(TabKind::Settings | TabKind::NewTab | TabKind::Downloads | TabKind::History)
    );

    // Whether it is showing no longer needs reporting: egui is asked directly
    // which layer the pointer is over.
    let _ = draw_sidebar_peek(root, chrome, &mut actions, peek);

    // Page-initiated UI last, over everything else it might overlap.
    let origin = chrome
        .browser
        .active_tab()
        .and_then(|tab| url::Url::parse(&tab.url).ok())
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "This page".to_owned());
    let scale = root.pixels_per_point();
    chrome
        .controls
        .draw(root, &chrome.palette, content_rect, scale, &origin);
    let controls_open = !chrome.controls.is_empty();

    UiOutput {
        actions,
        content_rect,
        settings_open,
        controls_open,
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
pub fn vertical_gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
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
/// `top` is separate because the navigation bar already ends in the space the
/// card would otherwise add for itself. Applying both put 14pt under the
/// address pill against 6pt above it, which is the card sitting lower than it
/// needs to rather than a margin anyone chose.
fn paint_content_backdrop(root: &Ui, outer: Rect, _palette: &Palette, top: f32) -> Rect {
    // No background fill here: the window-wide chrome base already covers
    // this area, and filling it again would cut the gradient off at the
    // sidebar edge.
    // The card's shadow is NOT painted here: the webview blit overwrites the
    // whole square content rect, which would erase the shadow inside the
    // rounded corners and leave square unshadowed patches. It is painted as
    // a ring in `finish_content_frame`, after the corner masks.
    let inset = Rect::from_min_max(
        pos2(outer.min.x + theme::CONTENT_MARGIN, outer.min.y + top),
        outer.max - vec2(theme::CONTENT_MARGIN, theme::CONTENT_MARGIN),
    );
    snap_rect(inset, root.pixels_per_point())
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
    root: &Ui,
    painter: &egui::Painter,
    rect: Rect,
    radius: f32,
    palette: &Palette,
    top_glow: f32,
    chrome_opacity: f32,
) {
    if chrome_opacity >= 1.0 {
        return; // Opaque chrome has no seam to hide.
    }
    // The square box has to be *covered*, not merely approached. Its furthest
    // point from the arc is radius * (sqrt(2) - 1) at each corner, and a
    // falloff that begins at the silhouette has decayed to a third of its
    // strength by the time it gets there — which is why the box stayed
    // faintly visible as a right angle outside every rounded corner. So the
    // ring holds full chrome out to the wedge and only then fades.
    let wedge = radius * (std::f32::consts::SQRT_2 - 1.0);
    painter.add(glass::shadow_tinted(
        rect,
        radius,
        wedge + radius * 0.42 + 6.0,
        wedge,
        glass::Inner::Outside,
        // Per vertex, not once for the whole ring: the chrome is a gradient,
        // and a ring painted in the colour from the card's centre is far too
        // dark along the top, where the glow strip is brightest.
        |at| chrome_color_at(root, at.y, palette, top_glow),
    ));
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
        // One physical pixel wide, so it feathers the mesh edge and no more.
        // At 1.4 points it was nearly three pixels, laid half over the page —
        // a chrome-coloured bite out of the page along the arcs but not along
        // the straight edges, which read as a chip just past each corner.
        let hairline = 1.0 / root.pixels_per_point();
        for (arc, colour) in arc_edges {
            fan_painter.add(Shape::line(arc, Stroke::new(hairline, colour)));
        }
    }

    // No drop shadow. The card fills nearly the whole window, so its shadow
    // only ever fell on the few points of chrome around it — a dark seam
    // tracing the edge rather than any impression of depth, and one more thing
    // between the page and the window's own edge. The accent stroke below is
    // the whole boundary now.
    paint_card_opacity_blend(
        root,
        &painter,
        content_rect,
        radius,
        palette,
        top_glow,
        chrome_opacity,
    );
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
pub const SIDEBAR_ID: &str = "zervo_sidebar";
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
        .show_inside(root, |ui| sidebar_body(ui, chrome, actions, false));

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
/// `compact` drops everything the navigation bar is already showing, leaving
/// the sidebar to do the one thing the bar cannot: tabs and workspaces.
fn sidebar_body(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    compact: bool,
) {
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
                // Settings and downloads sit in the navigation bar when it is up,
                // so the sidebar does not show them twice.
                if !compact {
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

    // ── Top row: nav icons, right of the macOS traffic lights. All of these
    // are in the navigation bar when the sidebar is collapsed.
    if !compact {
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
                && icons::icon_button(ui, Icon::Forward, 18.0, &palette, can_go_forward).clicked()
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
        draw_address_pill(ui, chrome, actions, 36.0);
        ui.add_space(12.0);
    } else {
        // The traffic lights still sit over the top of the sidebar.
        #[cfg(target_os = "macos")]
        ui.add_space(28.0);
    }

    // ── Essentials: pinned tabs as a tile grid.
    if chrome.settings.show_essentials {
        draw_essentials_grid(ui, chrome, actions);
    }

    // ── Workspaces and tab rows.
    //
    // The tab being dragged lives in egui's temp memory, like the navigation
    // bar's held item and the shelf's — it is scratch that lasts one gesture,
    // not part of the browser model.
    let ctx = ui.ctx().clone();
    let drag_id = Id::new("zervo_tab_drag");
    let dragging = ctx.data(|data| data.get_temp::<TabId>(drag_id));
    let pointer = ctx.input(|input| input.pointer.latest_pos());
    let editing = chrome.browser.workspace_edit.clone();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let active_workspace = chrome.browser.active_workspace;
            let active_tab = chrome.browser.active_tab;
            let always_close = chrome.settings.always_show_tab_close;

            // Both of these are resolved across every workspace rather than
            // within one, and that is the whole trick. Where the tab lands is
            // decided by the pointer, which may be over any workspace; the
            // release is only ever seen by the row being dragged, which is in
            // exactly one. Deciding them in the same pass means a tab dragged
            // from one workspace to another finds its target in one iteration
            // and its release in a different one, and the gesture silently
            // does nothing.
            let mut target: Option<(usize, usize, Option<TabId>)> = None;
            let mut released = false;
            let mut ghost: Option<(TabId, String)> = None;
            let mut rename: Option<(usize, WorkspaceName)> = None;

            for (workspace_index, workspace) in chrome.browser.workspaces.iter().enumerate() {
                let header = workspace_header(
                    ui,
                    workspace_index,
                    &workspace.name,
                    workspace.tabs.iter().filter(|tab| !tab.pinned).count(),
                    chrome.settings.show_tab_counts,
                    workspace_index == active_workspace,
                    editing
                        .as_ref()
                        .filter(|(index, _)| *index == workspace_index)
                        .map(|(_, name)| name.as_str()),
                    &palette,
                    actions,
                    &mut rename,
                );
                // A header takes the drop at the top of its list, which is the
                // only way to aim at a workspace with no tabs in it.
                if dragging.is_some() && pointer.is_some_and(|pos| header.contains(pos)) {
                    target = Some((workspace_index, 0, None));
                }

                // Enumerated over the whole list, not the filtered one: the
                // index is an insertion point into `workspace.tabs`, and
                // pinned tabs are in there too even though they are drawn in
                // the essentials grid instead.
                for (tab_index, tab) in workspace.tabs.iter().enumerate() {
                    if tab.pinned {
                        continue;
                    }
                    if dragging == Some(tab.id) {
                        ghost = Some((tab.id, tab.title.clone()));
                    }
                    let selected = active_tab == Some(tab.id);
                    let out = tab_row(
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
                    match out.drop_at {
                        Some(DropAt::Before) => target = Some((workspace_index, tab_index, None)),
                        Some(DropAt::After) => {
                            target = Some((workspace_index, tab_index + 1, None))
                        },
                        Some(DropAt::Onto) => {
                            target = Some((workspace_index, tab_index, Some(tab.id)))
                        },
                        None => {},
                    }
                    if out.close_clicked {
                        actions.push(UiAction::CloseTab(tab.id));
                    } else if out.clicked {
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
                // A drop below the last row files at the end of that
                // workspace, which is what aiming at the empty space under it
                // obviously means.
                if dragging.is_some() && pointer.is_some_and(|pos| rect.contains(pos)) {
                    target = Some((workspace_index, workspace.tabs.len(), None));
                }
                ui.add_space(12.0);
            }

            if let (Some(held), Some(pos)) = (dragging, pointer) {
                // Scroll when the pointer is pressed against either end of the
                // list, so a tab can reach a workspace that is off screen.
                let viewport = ui.clip_rect().intersect(ui.max_rect());
                let edge = 28.0;
                if pos.x > viewport.min.x && pos.x < viewport.max.x {
                    if pos.y < viewport.min.y + edge {
                        ui.scroll_with_delta(vec2(0.0, 6.0));
                    } else if pos.y > viewport.max.y - edge {
                        ui.scroll_with_delta(vec2(0.0, -6.0));
                    }
                }
                if let Some((id, title)) = &ghost {
                    let height = if chrome.settings.compact_sidebar {
                        28.0
                    } else {
                        34.0
                    };
                    paint_tab_ghost(
                        ui.painter(),
                        Rect::from_min_size(
                            pos2(viewport.min.x + 6.0, pos.y - height / 2.0),
                            vec2(viewport.width() - 12.0, height),
                        ),
                        title,
                        chrome.favicons.get(id),
                        &palette,
                    );
                }
                let _ = held;
                ctx.request_repaint();
            }

            // A drag can end without its row ever seeing the release: close
            // the tab mid-drag with Cmd-W and the row that would have reported
            // it is not built at all, which used to leave the sidebar showing
            // drop carets for the rest of the session.
            let orphaned = dragging.is_some_and(|id| chrome.browser.tab(id).is_none())
                || (dragging.is_some() && !ctx.input(|input| input.pointer.any_down()));
            if orphaned {
                ctx.data_mut(|data| data.remove::<TabId>(drag_id));
            }

            if released {
                ctx.data_mut(|data| data.remove::<TabId>(drag_id));
                if let (Some(held), Some((workspace, index, onto))) = (dragging, target) {
                    match onto {
                        // Dropped on a tab rather than between two. Dropping a
                        // tab on itself is the one case that means nothing.
                        Some(onto) if onto != held => {
                            actions.push(UiAction::GroupTabs {
                                onto,
                                dragged: held,
                            });
                        },
                        _ => actions.push(UiAction::MoveTab {
                            tab: held,
                            workspace,
                            index,
                        }),
                    }
                }
            }

            if let Some((index, edit)) = rename {
                match edit {
                    WorkspaceName::Typing(name) => {
                        chrome.browser.workspace_edit = Some((index, name));
                    },
                    WorkspaceName::Keep(name) => {
                        chrome.browser.workspace_edit = None;
                        ctx.data_mut(|data| data.remove::<usize>(Id::new("zervo_ws_focus")));
                        actions.push(UiAction::RenameWorkspace(index, name));
                    },
                    WorkspaceName::Discard => {
                        chrome.browser.workspace_edit = None;
                        ctx.data_mut(|data| data.remove::<usize>(Id::new("zervo_ws_focus")));
                    },
                }
            }
        });
}

/// zervo://history — everything visited, newest first, grouped by how long ago
/// and searchable.
fn draw_history_page(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    content_rect: Rect,
    actions: &mut Vec<UiAction>,
) {
    const ROW: f32 = 34.0;

    let palette = chrome.palette;
    root.ctx()
        .layer_painter(egui::LayerId::background())
        .rect_filled(
            content_rect,
            CornerRadius::same(theme::CONTENT_RADIUS as u8),
            palette.bg,
        );

    let mut ui = root.new_child(
        egui::UiBuilder::new()
            .max_rect(content_rect.shrink(28.0))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );

    ui.horizontal(|ui| {
        ui.label(RichText::new("History").size(24.0).color(palette.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !chrome.library.history.is_empty()
                && icons::icon_button(ui, Icon::Trash, 15.0, &palette, true)
                    .on_hover_text("Clear all history")
                    .clicked()
            {
                actions.push(UiAction::ClearHistory);
            }
        });
    });
    ui.add_space(12.0);

    // ── Search.
    let field = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width().min(420.0), 32.0));
    glass::paint(ui.painter(), field, &palette, Glass::tier(Tier::Card));
    icons::draw_icon(
        ui.painter(),
        Rect::from_center_size(pos2(field.min.x + 16.0, field.center().y), vec2(14.0, 14.0)),
        Icon::Search,
        palette.text_muted,
    );
    let mut editor = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(field.min.x + 30.0, field.min.y),
                field.max - vec2(10.0, 0.0),
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    editor.add(
        TextEdit::singleline(&mut chrome.browser.history_query)
            .frame(Frame::NONE)
            .font(FontId::proportional(13.5))
            .text_color(palette.text)
            .hint_text(RichText::new("Search history").color(palette.text_muted))
            .desired_width(editor.available_width()),
    );
    ui.advance_cursor_after_rect(field);
    ui.add_space(14.0);

    let now = crate::app::now();
    let groups = chrome.library.browse(&chrome.browser.history_query, now);
    if groups.is_empty() {
        ui.label(
            RichText::new(if chrome.library.history.is_empty() {
                "Nothing here yet."
            } else {
                "No pages match that search."
            })
            .size(13.5)
            .color(palette.text_muted),
        );
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(&mut ui, |ui| {
            for (bucket, rows) in groups {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(bucket.label())
                        .size(11.5)
                        .color(palette.text_muted),
                );
                ui.add_space(4.0);
                for (index, visit) in rows {
                    let (row, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                    if response.hovered() {
                        ui.painter()
                            .rect_filled(row, CornerRadius::same(8), palette.surface_hover);
                    }
                    icons::draw_icon(
                        ui.painter(),
                        Rect::from_center_size(
                            pos2(row.min.x + 16.0, row.center().y),
                            vec2(14.0, 14.0),
                        ),
                        Icon::Globe,
                        palette.text_muted,
                    );
                    let title = if visit.title.is_empty() {
                        visit.url.as_str()
                    } else {
                        visit.title.as_str()
                    };
                    ui.painter().text(
                        pos2(row.min.x + 34.0, row.center().y - 7.0),
                        Align2::LEFT_CENTER,
                        ellipsize(title, 64),
                        FontId::proportional(13.5),
                        palette.text,
                    );
                    ui.painter().text(
                        pos2(row.min.x + 34.0, row.center().y + 8.0),
                        Align2::LEFT_CENTER,
                        ellipsize(&visit.url, 74),
                        FontId::proportional(11.5),
                        palette.text_muted,
                    );
                    ui.painter().text(
                        pos2(row.max.x - 34.0, row.center().y),
                        Align2::RIGHT_CENTER,
                        visit.local_time().format("%H:%M").to_string(),
                        FontId::proportional(11.5),
                        palette.text_muted,
                    );
                    let forget = Rect::from_center_size(
                        pos2(row.max.x - 14.0, row.center().y),
                        vec2(18.0, 18.0),
                    );
                    if response.hovered() {
                        icons::draw_icon(
                            ui.painter(),
                            forget.shrink(4.0),
                            Icon::Close,
                            palette.text_muted,
                        );
                    }
                    if ui
                        .interact(forget, Id::new("zervo_forget").with(index), Sense::click())
                        .clicked()
                    {
                        actions.push(UiAction::ForgetVisit(index));
                    } else if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                        actions.push(UiAction::Navigate(visit.url.clone()));
                    }
                }
            }
        });
}

/// The favourites star, sitting just after the forward button. Returns its
/// rect so the hover card can be anchored to it.
fn draw_favourite_star(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
) -> Rect {
    let palette = chrome.palette;
    let url = chrome
        .browser
        .active_tab()
        .map(|tab| tab.url.clone())
        .unwrap_or_default();
    let saved = !url.is_empty() && chrome.library.is_favourite(&url);

    let response = icons::icon_button(ui, Icon::Star, NAVBAR_ICON, &palette, !url.is_empty());
    let rect = response.rect;
    if saved {
        // No filled star in the vendored pack, so say it with colour.
        icons::draw_icon(
            ui.painter(),
            Rect::from_center_size(rect.center(), vec2(NAVBAR_ICON, NAVBAR_ICON)),
            Icon::Star,
            palette.accent,
        );
    }
    if response
        .on_hover_text(if saved {
            "Remove from favourites"
        } else {
            "Add to favourites"
        })
        .clicked()
    {
        actions.push(UiAction::ToggleFavourite);
    }
    rect
}

/// A card that opens while the pointer is on `anchor` and stays open while it
/// is on either. Returns the rect it drew into, so a caller can tell whether
/// it is showing.
///
/// Shared because every reveal in the bar wants the same behaviour, and the
/// fiddly parts — the gap between trigger and card counting as "still here",
/// growing downward so the card never covers what opened it — are worth
/// getting right once.
fn hover_card(
    root: &mut Ui,
    palette: &Palette,
    key: &str,
    anchor: Rect,
    size: egui::Vec2,
    add: impl FnOnce(&mut Ui, Rect, Palette),
) -> Option<Rect> {
    let ctx = root.ctx().clone();
    let id = Id::new(key);
    let pointer = ctx.input(|input| input.pointer.latest_pos());
    let was_open = ctx.data(|data| data.get_temp::<bool>(id)).unwrap_or(false);

    let card = clamp_into(
        Rect::from_min_size(
            pos2(anchor.center().x - size.x / 2.0, anchor.max.y + 6.0),
            size,
        ),
        ctx.content_rect(),
    );
    // The gap between trigger and card counts as still hovering, or the card
    // closes in the space between them.
    let bridge = Rect::from_min_max(
        pos2(anchor.min.x.min(card.min.x), anchor.max.y),
        pos2(anchor.max.x.max(card.min.x + 40.0), card.min.y + 2.0),
    );
    let open = pointer.is_some_and(|pos| {
        anchor.contains(pos)
            || (was_open && (card.expand(6.0).contains(pos) || bridge.contains(pos)))
    });
    ctx.data_mut(|data| data.insert_temp(id, open));

    let grow = glass::ease_out(ctx.animate_bool_with_time(id.with("grow"), open, 0.13));
    if grow <= 0.0 {
        return None;
    }
    // Grows downward out of the trigger's underside, so it never covers it.
    let drawn = Rect::from_min_size(
        card.min,
        vec2(card.width(), (card.height() * grow).max(1.0)),
    );

    egui::Area::new(id.with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(drawn.min)
        .constrain(false)
        .show(&ctx, |ui| {
            // Room for the shadow, which is painted outside the card. Clipping
            // tight to the card scissors it off with a hard rectangle and
            // leaves a square-cornered wedge of shadow outside each rounded
            // corner — the second corner that kept showing up in screenshots.
            ui.set_clip_rect(drawn.expand(glass::room(palette.radius(Tier::Panel))));
            let painter = ui.painter();
            // Drawn at `drawn`, not `card`: while it grows it is a whole
            // rounded card of its current height, rather than a full-height
            // one cut off square at the bottom.
            for shape in glass::shapes(
                drawn,
                palette,
                Glass::of(Surface::Menu)
                    .radius(Tier::Panel)
                    .opaque(palette.bg)
                    .border(palette.border),
            ) {
                painter.add(shape);
            }
            // The contents keep the tight clip, so a half-grown card shows the
            // top of a whole one rather than a squashed one — and so its
            // widgets are not interactive before they are visible.
            ui.set_clip_rect(drawn);
            // The card knows where it landed; its contents do not. Handing the
            // palette down rather than letting the closure capture the outer
            // one is what lets pale text turn dark when this card opens over a
            // white page in dark mode.
            add(ui, card, palette.over(drawn));
            ui.advance_cursor_after_rect(drawn);
        });
    Some(drawn)
}

/// A small floating list, anchored under `at`. Used for right-click menus in
/// the chrome, which egui's own menus do not cover because these hang off
/// hand-drawn rows rather than widgets.
fn popup_menu<T: Clone>(
    ctx: &egui::Context,
    palette: &Palette,
    key: Id,
    at: egui::Pos2,
    rows: &[(String, T)],
) -> Option<T> {
    const ROW: f32 = 28.0;
    let width = 200.0;
    let rect = clamp_into(
        Rect::from_min_size(at, vec2(width, rows.len() as f32 * ROW + 12.0)),
        ctx.content_rect(),
    );
    let palette = &palette.over(rect);
    let mut chosen = None;
    egui::Area::new(key)
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(ctx, |ui| {
            let painter = ui.painter();
            for shape in glass::shapes(
                rect,
                palette,
                Glass::of(Surface::Menu)
                    .opaque(palette.bg)
                    .border(palette.border),
            ) {
                painter.add(shape);
            }
            for (index, (label, value)) in rows.iter().enumerate() {
                let row = Rect::from_min_size(
                    pos2(rect.min.x + 6.0, rect.min.y + 6.0 + index as f32 * ROW),
                    vec2(rect.width() - 12.0, ROW),
                );
                let response = ui.interact(row, key.with(index), Sense::click());
                if response.hovered() {
                    ui.painter()
                        .rect_filled(row, CornerRadius::same(7), palette.surface_hover);
                }
                ui.painter().text(
                    pos2(row.min.x + 8.0, row.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(13.0),
                    palette.text,
                );
                if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    chosen = Some(value.clone());
                }
            }
            ui.advance_cursor_after_rect(rect);
        });
    chosen
}

/// Hovering the star opens a card of the favourites, rather than spending a
/// whole bar on them.
fn draw_favourites_card(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    star: Rect,
) {
    const ROW: f32 = 32.0;
    const TILE: f32 = 62.0;

    let palette = chrome.palette;
    let grid = chrome.settings.favourites_grid;
    let count = chrome.library.favourites.len().max(1);
    let size = if grid {
        let rows = count.div_ceil(4).clamp(1, 4) as f32;
        vec2(4.0 * TILE + 20.0, rows * TILE + 46.0)
    } else {
        vec2(300.0, count.clamp(1, 10) as f32 * ROW + 46.0)
    };

    let favourites: Vec<(String, String)> = chrome
        .library
        .favourites
        .iter()
        .map(|entry| (entry.url.clone(), entry.title.clone()))
        .collect();
    let editing = chrome.browser.favourite_edit.clone();
    let id = Id::new("zervo_favourites_card");

    let mut toggle_layout = false;
    let mut edit: Option<Option<(String, String)>> = None;

    hover_card(
        root,
        &palette,
        "zervo_favourites_card",
        star,
        size,
        |ui, card, palette| {
            let pointer = ui.ctx().input(|input| input.pointer.latest_pos());

            // ── Header: what this is, and how to show it.
            let header =
                Rect::from_min_size(card.min + vec2(12.0, 8.0), vec2(card.width() - 24.0, 20.0));
            ui.painter().text(
                header.left_center(),
                Align2::LEFT_CENTER,
                "Favourites",
                FontId::proportional(11.5),
                palette.text_muted,
            );
            let switch = Rect::from_center_size(
                pos2(header.max.x - 9.0, header.center().y),
                vec2(18.0, 18.0),
            );
            icons::draw_icon(
                ui.painter(),
                switch.shrink(3.0),
                if grid { Icon::Sidebar } else { Icon::Browser },
                palette.text_muted,
            );
            if ui
                .interact(switch, id.with("layout"), Sense::click())
                .on_hover_text(if grid {
                    "Show as a list"
                } else {
                    "Show as tiles"
                })
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                toggle_layout = true;
            }

            let body = Rect::from_min_max(
                pos2(card.min.x + 10.0, card.min.y + 34.0),
                card.max - vec2(10.0, 8.0),
            );
            if favourites.is_empty() {
                ui.painter().text(
                    body.left_top() + vec2(4.0, 12.0),
                    Align2::LEFT_TOP,
                    "Nothing saved yet — the star adds this page.",
                    FontId::proportional(12.0),
                    palette.text_muted,
                );
                return;
            }

            if grid {
                for (index, (url, title)) in favourites.iter().enumerate() {
                    let tile = Rect::from_min_size(
                        body.min + vec2((index % 4) as f32 * TILE, (index / 4) as f32 * TILE),
                        vec2(TILE - 6.0, TILE - 6.0),
                    );
                    if tile.max.y > body.max.y {
                        break;
                    }
                    let response = ui.interact(tile, id.with(("tile", index)), Sense::click());
                    let over = pointer.is_some_and(|pos| tile.contains(pos));
                    if over {
                        ui.painter().rect_filled(
                            tile,
                            CornerRadius::same(9),
                            palette.surface_hover,
                        );
                    }
                    // No favicon store yet, so the initial stands in for one.
                    let badge = Rect::from_center_size(
                        pos2(tile.center().x, tile.min.y + 20.0),
                        vec2(26.0, 26.0),
                    );
                    ui.painter().rect_filled(
                        badge,
                        CornerRadius::same(7),
                        palette.accent.gamma_multiply(0.22),
                    );
                    ui.painter().text(
                        badge.center(),
                        Align2::CENTER_CENTER,
                        initial(title, url),
                        FontId::proportional(13.0),
                        palette.accent,
                    );
                    ui.painter().text(
                        pos2(tile.center().x, tile.max.y - 12.0),
                        Align2::CENTER_CENTER,
                        ellipsize(display_name(title, url), 9),
                        FontId::proportional(10.5),
                        palette.text,
                    );
                    if over {
                        let close = Rect::from_center_size(
                            pos2(tile.max.x - 8.0, tile.min.y + 8.0),
                            vec2(14.0, 14.0),
                        );
                        icons::draw_icon(
                            ui.painter(),
                            close.shrink(2.0),
                            Icon::Close,
                            palette.text_muted,
                        );
                        if ui
                            .interact(close, id.with(("gx", index)), Sense::click())
                            .clicked()
                        {
                            actions.push(UiAction::RemoveFavourite(url.clone()));
                        }
                    }
                    if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                        actions.push(UiAction::Navigate(url.clone()));
                    }
                }
                return;
            }

            for (index, (url, title)) in favourites.iter().enumerate() {
                let row = Rect::from_min_size(
                    pos2(body.min.x, body.min.y + index as f32 * ROW),
                    vec2(body.width(), ROW - 2.0),
                );
                if row.max.y > body.max.y {
                    break;
                }
                let being_edited = editing.as_ref().is_some_and(|(at, _)| at == url);
                let response = ui.interact(row, id.with(("row", index)), Sense::click());
                // Hover from the pointer, not the row's response: the controls sit
                // on top of the row, which stops the row counting as hovered.
                let over = pointer.is_some_and(|pos| row.contains(pos));
                if over && !being_edited {
                    ui.painter()
                        .rect_filled(row, CornerRadius::same(8), palette.surface_hover);
                }
                icons::draw_icon(
                    ui.painter(),
                    Rect::from_center_size(
                        pos2(row.min.x + 14.0, row.center().y),
                        vec2(13.0, 13.0),
                    ),
                    Icon::Globe,
                    palette.text_muted,
                );

                if being_edited {
                    let mut draft = editing
                        .as_ref()
                        .map(|(_, name)| name.clone())
                        .unwrap_or_default();
                    let field = Rect::from_min_max(
                        pos2(row.min.x + 28.0, row.min.y + 2.0),
                        pos2(row.max.x - 46.0, row.max.y - 2.0),
                    );
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
                    let editor = child.add_sized(
                        field.size(),
                        TextEdit::singleline(&mut draft).font(FontId::proportional(12.5)),
                    );
                    if !editor.has_focus() && !editor.lost_focus() {
                        // Once, when the editor opens. Requesting it every frame pins the
                        // keyboard here: clicking the address bar takes focus for a single
                        // frame and loses it again, and every keystroke in the window ends up
                        // in this field until it is dismissed.
                        let focus_key = Id::new("zervo_ws_focus");
                        if ui.ctx().data(|data| data.get_temp::<usize>(focus_key)) != Some(index) {
                            editor.request_focus();
                            ui.ctx().data_mut(|data| data.insert_temp(focus_key, index));
                        }
                    }
                    let entered = child.input(|input| input.key_pressed(Key::Enter));
                    let escaped = child.input(|input| input.key_pressed(Key::Escape));

                    // An explicit tick, because committing only on Enter-and-blur
                    // meant a rename was lost far more often than it was kept.
                    let save = Rect::from_center_size(
                        pos2(row.max.x - 30.0, row.center().y),
                        vec2(20.0, 20.0),
                    );
                    icons::draw_icon(ui.painter(), save.shrink(3.0), Icon::Check, palette.accent);
                    let saved = ui
                        .interact(save, id.with(("save", index)), Sense::click())
                        .on_hover_text("Save")
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .clicked();
                    let cancel = Rect::from_center_size(
                        pos2(row.max.x - 10.0, row.center().y),
                        vec2(20.0, 20.0),
                    );
                    icons::draw_icon(
                        ui.painter(),
                        cancel.shrink(4.0),
                        Icon::Close,
                        palette.text_muted,
                    );
                    let cancelled = ui
                        .interact(cancel, id.with(("cancelrename", index)), Sense::click())
                        .on_hover_text("Discard")
                        .clicked();

                    if saved || entered {
                        actions.push(UiAction::RenameFavourite(url.clone(), draft.clone()));
                        edit = Some(None);
                    } else if cancelled || escaped {
                        edit = Some(None);
                    } else {
                        edit = Some(Some((url.clone(), draft)));
                    }
                    continue;
                }

                ui.painter().text(
                    pos2(row.min.x + 28.0, row.center().y),
                    Align2::LEFT_CENTER,
                    ellipsize(display_name(title, url), 24),
                    FontId::proportional(13.0),
                    palette.text,
                );
                if over {
                    let rename = Rect::from_center_size(
                        pos2(row.max.x - 30.0, row.center().y),
                        vec2(18.0, 18.0),
                    );
                    icons::draw_icon(
                        ui.painter(),
                        rename.shrink(3.0),
                        Icon::Sliders,
                        palette.text_muted,
                    );
                    if ui
                        .interact(rename, id.with(("edit", index)), Sense::click())
                        .on_hover_text("Rename")
                        .clicked()
                    {
                        edit = Some(Some((url.clone(), title.clone())));
                    }
                    let close = Rect::from_center_size(
                        pos2(row.max.x - 10.0, row.center().y),
                        vec2(18.0, 18.0),
                    );
                    icons::draw_icon(
                        ui.painter(),
                        close.shrink(3.0),
                        Icon::Close,
                        palette.text_muted,
                    );
                    if ui
                        .interact(close, id.with(("x", index)), Sense::click())
                        .on_hover_text("Remove")
                        .clicked()
                    {
                        actions.push(UiAction::RemoveFavourite(url.clone()));
                    }
                }
                if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    actions.push(UiAction::Navigate(url.clone()));
                }
            }
        },
    );

    if toggle_layout {
        chrome.settings.favourites_grid = !grid;
        actions.push(UiAction::PersistSettings);
    }
    if let Some(next) = edit {
        chrome.browser.favourite_edit = next;
    }
}

/// Hovering the downloads button opens the list, with the controls that
/// actually do something on each row.
///
/// Pause and resume are shown but disabled. Servo streams a download's bytes
/// to the embedder as they arrive and offers no way to ask it to stop and pick
/// up again, so there is nothing to hang them on — a button that quietly did
/// nothing would be worse than one that says why.
fn draw_downloads_card(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    anchor: Rect,
) {
    use crate::downloads::{DownloadState, format_bytes};
    const ROW: f32 = 46.0;

    let palette = chrome.palette;
    let items: Vec<(
        u64,
        String,
        String,
        Option<f32>,
        u64,
        Option<u64>,
        DownloadState,
    )> = chrome
        .downloads
        .items
        .iter()
        .rev()
        .take(8)
        .map(|item| {
            (
                item.id,
                item.filename.clone(),
                item.url.clone(),
                item.fraction(),
                item.received,
                item.total,
                item.state.clone(),
            )
        })
        .collect();
    let rows = items.len().max(1) as f32;
    let size = vec2(360.0, rows * ROW + 52.0);
    let id = Id::new("zervo_downloads_card");
    let menu_id = Id::new("zervo_download_menu");

    hover_card(
        root,
        &palette,
        "zervo_downloads_card",
        anchor,
        size,
        |ui, card, palette| {
            let ctx = ui.ctx().clone();
            let pointer = ctx.input(|input| input.pointer.latest_pos());

            let header =
                Rect::from_min_size(card.min + vec2(12.0, 8.0), vec2(card.width() - 24.0, 20.0));
            ui.painter().text(
                header.left_center(),
                Align2::LEFT_CENTER,
                "Downloads",
                FontId::proportional(11.5),
                palette.text_muted,
            );
            if !items.is_empty() {
                let clear = Rect::from_center_size(
                    pos2(header.max.x - 9.0, header.center().y),
                    vec2(18.0, 18.0),
                );
                icons::draw_icon(
                    ui.painter(),
                    clear.shrink(3.0),
                    Icon::Trash,
                    palette.text_muted,
                );
                if ui
                    .interact(clear, id.with("clear"), Sense::click())
                    .on_hover_text("Clear finished")
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .clicked()
                {
                    actions.push(UiAction::ClearDownloads);
                }
            }

            let body = Rect::from_min_max(
                pos2(card.min.x + 10.0, card.min.y + 34.0),
                card.max - vec2(10.0, 8.0),
            );
            if items.is_empty() {
                ui.painter().text(
                    body.left_top() + vec2(4.0, 12.0),
                    Align2::LEFT_TOP,
                    "Nothing downloaded yet.",
                    FontId::proportional(12.0),
                    palette.text_muted,
                );
                return;
            }

            for (index, (download, filename, _url, fraction, received, total, state)) in
                items.iter().enumerate()
            {
                let row = Rect::from_min_size(
                    pos2(body.min.x, body.min.y + index as f32 * ROW),
                    vec2(body.width(), ROW - 4.0),
                );
                if row.max.y > body.max.y {
                    break;
                }
                let over = pointer.is_some_and(|pos| row.contains(pos));
                let response = ui.interact(row, id.with(("row", index)), Sense::click_and_drag());
                if over {
                    ui.painter()
                        .rect_filled(row, CornerRadius::same(8), palette.surface_hover);
                }

                let running = *state == DownloadState::Running;
                icons::draw_icon(
                    ui.painter(),
                    Rect::from_center_size(
                        pos2(row.min.x + 16.0, row.center().y),
                        vec2(15.0, 15.0),
                    ),
                    match state {
                        DownloadState::Running => Icon::FileArrowDown,
                        DownloadState::Complete => Icon::CheckCircle,
                        _ => Icon::XCircle,
                    },
                    match state {
                        DownloadState::Complete => palette.accent,
                        DownloadState::Failed(_) | DownloadState::Cancelled => {
                            palette.text_muted.gamma_multiply(0.8)
                        },
                        DownloadState::Running => palette.text_muted,
                    },
                );
                ui.painter().text(
                    pos2(row.min.x + 34.0, row.min.y + 13.0),
                    Align2::LEFT_CENTER,
                    ellipsize(filename, 30),
                    FontId::proportional(13.0),
                    palette.text,
                );
                let detail = match state {
                    DownloadState::Running => match total {
                        Some(total) => {
                            format!("{} of {}", format_bytes(*received), format_bytes(*total))
                        },
                        None => format_bytes(*received),
                    },
                    DownloadState::Complete => format_bytes(*received),
                    DownloadState::Cancelled => "Stopped".to_owned(),
                    DownloadState::Failed(why) => ellipsize(why, 34),
                };
                ui.painter().text(
                    pos2(row.min.x + 34.0, row.min.y + 29.0),
                    Align2::LEFT_CENTER,
                    detail,
                    FontId::proportional(11.0),
                    palette.text_muted,
                );
                if let Some(fraction) = fraction
                    && running
                {
                    let track = Rect::from_min_size(
                        pos2(row.min.x + 34.0, row.max.y - 8.0),
                        vec2(row.width() - 150.0, 3.0),
                    );
                    ui.painter()
                        .rect_filled(track, CornerRadius::same(2), palette.border);
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            track.min,
                            vec2(track.width() * fraction, track.height()),
                        ),
                        CornerRadius::same(2),
                        palette.accent,
                    );
                }

                // ── Controls, on hover so a quiet list stays quiet.
                if over {
                    let mut x = row.max.x - 16.0;
                    let mut control = |ui: &mut Ui,
                                       icon: Icon,
                                       tip: &str,
                                       enabled: bool,
                                       key: &str| {
                        let hit = Rect::from_center_size(pos2(x, row.center().y), vec2(24.0, 24.0));
                        x -= 26.0;
                        let response = ui.interact(hit, id.with((key, index)), Sense::click());
                        if enabled && response.hovered() {
                            ui.painter()
                                .rect_filled(hit, CornerRadius::same(7), palette.surface);
                        }
                        icons::draw_icon(
                            ui.painter(),
                            hit.shrink(6.0),
                            icon,
                            if enabled {
                                palette.text
                            } else {
                                palette.text_muted.gamma_multiply(0.35)
                            },
                        );
                        enabled && response.on_hover_text(tip).clicked()
                    };

                    if running {
                        if control(ui, Icon::Close, "Stop", true, "stop") {
                            actions.push(UiAction::CancelDownload(*download));
                        }
                        control(
                            ui,
                            Icon::Pause,
                            "Pause — the engine cannot suspend a transfer",
                            false,
                            "pause",
                        );
                    } else {
                        if control(ui, Icon::Reload, "Start again", true, "restart") {
                            actions.push(UiAction::RestartDownload(*download));
                        }
                        if *state == DownloadState::Complete {
                            if control(ui, Icon::Folder, "Show in folder", true, "reveal") {
                                actions.push(UiAction::RevealDownload(*download));
                            }
                            if control(ui, Icon::ExternalLink, "Open", true, "open") {
                                actions.push(UiAction::OpenDownload(*download));
                            }
                        }
                    }
                }

                if response.secondary_clicked()
                    && let Some(pos) = pointer
                {
                    ctx.data_mut(|data| data.insert_temp(menu_id, (*download, pos)));
                }
                if *state == DownloadState::Complete
                    && response.on_hover_cursor(CursorIcon::PointingHand).clicked()
                {
                    actions.push(UiAction::OpenDownload(*download));
                }
            }

            // ── Right-click menu.
            if let Some((download, at)) =
                ctx.data(|data| data.get_temp::<(u64, egui::Pos2)>(menu_id))
            {
                let url = items
                    .iter()
                    .find(|(id, ..)| *id == download)
                    .map(|(_, _, url, ..)| url.clone())
                    .unwrap_or_default();
                let filename = items
                    .iter()
                    .find(|(id, ..)| *id == download)
                    .map(|(_, name, ..)| name.clone())
                    .unwrap_or_default();
                let rows = [
                    ("Copy download link".to_owned(), 0u8),
                    ("Copy file name".to_owned(), 1),
                    ("Show in folder".to_owned(), 2),
                    ("Open".to_owned(), 3),
                    ("Start again".to_owned(), 4),
                    ("Remove from list".to_owned(), 5),
                ];
                match popup_menu(&ctx, &palette, menu_id.with("area"), at, &rows) {
                    Some(choice) => {
                        ctx.data_mut(|data| data.remove::<(u64, egui::Pos2)>(menu_id));
                        match choice {
                            0 => ctx.copy_text(url),
                            1 => ctx.copy_text(filename),
                            2 => actions.push(UiAction::RevealDownload(download)),
                            3 => actions.push(UiAction::OpenDownload(download)),
                            4 => actions.push(UiAction::RestartDownload(download)),
                            _ => actions.push(UiAction::RemoveDownload(download)),
                        }
                    },
                    // A press anywhere else puts it away.
                    None if ctx.input(|input| input.pointer.any_pressed()) => {
                        ctx.data_mut(|data| data.remove::<(u64, egui::Pos2)>(menu_id));
                    },
                    None => {},
                }
            }
        },
    );
}

/// A saved page's name, falling back to its host when it has no title.
pub fn display_name<'a>(title: &'a str, url: &'a str) -> &'a str {
    if !title.is_empty() {
        return title;
    }
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
}

/// Whether a colour is light enough that a mark on it wants to be dark.
///
/// Rec. 601 luma, which is close enough for deciding between black and white
/// and is what everything else that has to make this call uses.
fn color_is_light(color: egui::Color32) -> bool {
    let luma =
        0.299 * f32::from(color.r()) + 0.587 * f32::from(color.g()) + 0.114 * f32::from(color.b());
    luma > 140.0
}

/// Stands in for a favicon, which is not stored anywhere yet.
pub fn initial(title: &str, url: &str) -> String {
    display_name(title, url)
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_owned())
}

/// Shorten to `limit` characters on a character boundary, with an ellipsis.
pub fn ellipsize(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    text.chars()
        .take(limit.saturating_sub(1))
        .collect::<String>()
        + "…"
}

/// Keep a rect inside `bounds`.
pub fn clamp_into(rect: Rect, bounds: Rect) -> Rect {
    let mut min = rect.min;
    min.x = min.x.clamp(
        bounds.min.x + 8.0,
        (bounds.max.x - rect.width() - 8.0).max(bounds.min.x + 8.0),
    );
    min.y = min.y.clamp(
        bounds.min.y + 8.0,
        (bounds.max.y - rect.height() - 8.0).max(bounds.min.y + 8.0),
    );
    Rect::from_min_size(min, rect.size())
}

// ── Navigation bar (shown when the sidebar is collapsed) ───────────────────

/// Height of the navigation bar. Tall enough for a 36pt pill with breathing
/// room above and below.
/// The row the controls themselves need: a 36pt pill with 2pt of air above and
/// below it. The bar is never shorter than this.
const NAVBAR_ROW: f32 = 40.0;
/// Default bar height: exactly the row, so nothing is wasted until someone
/// asks for the space by dragging.
pub const NAVBAR_DEFAULT_HEIGHT: f32 = NAVBAR_ROW;
/// The gap between the shelf and the content card below it.
const SHELF_BOTTOM: f32 = 6.0;
/// Tall enough for every row the shelf will offer. Derived rather than picked:
/// at 220 the shelf came to 162pt of usable height and a row costs 62, so it
/// stopped at two rows however hard it was dragged.
const NAVBAR_MAX_HEIGHT: f32 =
    NAVBAR_ROW + crate::dashboard::height_for_rows(crate::dashboard::MAX_ROWS) + SHELF_BOTTOM;
/// Room the macOS traffic lights need before the first button.
const TRAFFIC_LIGHTS: f32 = 78.0;
/// Sized against the buttons rather than the row, so the pill has about as
/// much air around it as they do.
const NAVBAR_PILL_HEIGHT: f32 = 28.0;
/// One size for every button in the bar. `icon_button` derives the whole
/// button from the glyph size, so mixing sizes gives visibly different hit
/// areas and hover shapes sitting on the same line.
const NAVBAR_ICON: f32 = 17.0;
/// Width of the centred address pill.
pub const ADDRESS_PILL_DEFAULT_WIDTH: f32 = 460.0;
const ADDRESS_PILL_MIN_WIDTH: f32 = 220.0;
const ADDRESS_PILL_MAX_WIDTH: f32 = 900.0;

/// With the sidebar collapsed, navigation moves into a bar across the top:
/// window controls and navigation snapped to the left beside the traffic
/// lights, the address pill centred on the window, and extensions, downloads
/// and settings snapped to the right.
/// Something that can sit in the navigation bar. The bar is a list of these
/// rather than a fixed sequence of calls, so it can be rearranged.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum NavItem {
    Sidebar,
    Back,
    Forward,
    Reload,
    Favourite,
    Extensions,
    History,
    Downloads,
    Settings,
}

/// Which side of the address pill an item lives on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavSide {
    Left,
    Right,
}

impl NavItem {
    pub const ALL: [NavItem; 9] = [
        NavItem::Sidebar,
        NavItem::Back,
        NavItem::Forward,
        NavItem::Reload,
        NavItem::Favourite,
        NavItem::Extensions,
        NavItem::History,
        NavItem::Downloads,
        NavItem::Settings,
    ];

    pub fn default_left() -> Vec<NavItem> {
        vec![
            NavItem::Sidebar,
            NavItem::Back,
            NavItem::Forward,
            NavItem::Reload,
            NavItem::Favourite,
        ]
    }

    pub fn default_right() -> Vec<NavItem> {
        vec![
            NavItem::Extensions,
            NavItem::History,
            NavItem::Downloads,
            NavItem::Settings,
        ]
    }

    fn icon(self) -> Icon {
        match self {
            NavItem::Sidebar => Icon::Sidebar,
            NavItem::Back => Icon::Back,
            NavItem::Forward => Icon::Forward,
            NavItem::Reload => Icon::Reload,
            NavItem::Favourite => Icon::Star,
            NavItem::Extensions => Icon::Extensions,
            NavItem::History => Icon::History,
            NavItem::Downloads => Icon::Download,
            NavItem::Settings => Icon::Gear,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NavItem::Sidebar => "Sidebar",
            NavItem::Back => "Back",
            NavItem::Forward => "Forward",
            NavItem::Reload => "Reload",
            NavItem::Favourite => "Favourites",
            NavItem::Extensions => "Extensions",
            NavItem::History => "History",
            NavItem::Downloads => "Downloads",
            NavItem::Settings => "Settings",
        }
    }
}

/// One button's worth of width in the bar.
fn nav_item_width() -> f32 {
    NAVBAR_ICON + 12.0
}

/// How far the navigation bar's shelf is uncovered, in points.
///
/// Zero unless the bar is up and has been dragged taller than its one row of
/// controls. The new tab page reads this to hold its wallpaper still while the
/// shelf takes space off the top of the page.
pub fn shelf_reveal(browser: &BrowserState, settings: &Settings) -> f32 {
    if !browser.sidebar_collapsed {
        return 0.0;
    }
    settings.navbar_height.clamp(NAVBAR_ROW, NAVBAR_MAX_HEIGHT) - NAVBAR_ROW
}

fn draw_navbar(root: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    let palette = chrome.palette;
    let ctx = root.ctx().clone();
    let window = root.ctx().content_rect();
    let height = chrome
        .settings
        .navbar_height
        .clamp(NAVBAR_ROW, NAVBAR_MAX_HEIGHT);
    let strip = Rect::from_min_size(window.min, vec2(window.width(), height));
    // The controls keep to a fixed row at the top, so dragging the bar taller
    // opens space underneath them rather than spreading them out.
    let row = Rect::from_min_size(strip.min, vec2(strip.width(), NAVBAR_ROW));

    let config = root_data_flag(&ctx, Id::new("zervo_navbar_config"));
    let shelf_open = strip.height() > NAVBAR_ROW + 10.0;

    // ── Where every item sits, both sides, worked out before anything is
    // drawn: arranging needs all of them to know where a drop lands.
    let spacing = 2.0;
    let item_size = vec2(nav_item_width(), NAVBAR_ICON + 8.0);
    let mut placed: Vec<(NavSide, usize, NavItem, Rect)> = Vec::new();
    let mut x = window.left() + TRAFFIC_LIGHTS;
    for (index, item) in chrome.settings.navbar_left.iter().enumerate() {
        let rect = Rect::from_min_size(pos2(x, row.center().y - item_size.y / 2.0), item_size);
        placed.push((NavSide::Left, index, *item, rect));
        x += nav_item_width() + spacing;
    }
    let count = chrome.settings.navbar_right.len() as f32;
    let mut x =
        window.right() - 10.0 - (count * nav_item_width() + (count - 1.0).max(0.0) * spacing);
    let right_edge_start = x;
    for (index, item) in chrome.settings.navbar_right.iter().enumerate() {
        let rect = Rect::from_min_size(pos2(x, row.center().y - item_size.y / 2.0), item_size);
        placed.push((NavSide::Right, index, *item, rect));
        x += nav_item_width() + spacing;
    }

    if config {
        draw_navbar_config(root, chrome, actions, &placed, row);
    } else {
        for (_, _, item, rect) in &placed {
            draw_nav_item(root, chrome, actions, *item, *rect);
        }
    }

    // ── Adding a widget, and arranging the bar. Both belong to what the drag
    // revealed, so they come with the shelf — except that leaving arrange mode
    // has to stay possible once it is on.
    let mut extras = Vec::new();
    if shelf_open {
        extras.push(Icon::Plus);
    }
    if shelf_open || config {
        extras.push(Icon::Arrange);
    }
    let mut add_widget = None;
    if !extras.is_empty() {
        let width = extras.len() as f32 * nav_item_width() + (extras.len() - 1) as f32 * spacing;
        let tray = Rect::from_min_size(
            pos2(
                right_edge_start - 10.0 - width,
                row.center().y - item_size.y / 2.0,
            ),
            vec2(width, item_size.y),
        );
        let mut group = root.new_child(
            egui::UiBuilder::new()
                .max_rect(tray)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        group.spacing_mut().item_spacing.x = spacing;
        for icon in extras {
            let response = icons::icon_button(&mut group, icon, NAVBAR_ICON, &palette, true);
            if icon == Icon::Plus {
                let response = response.on_hover_text("Add a widget");
                if response.clicked() {
                    let open = Id::new("zervo_navbar_add_widget");
                    let was = root_data_flag(&ctx, open);
                    ctx.data_mut(|data| data.insert_temp(open, !was));
                }
                add_widget = Some(response.rect);
            } else if response
                .on_hover_text(if config {
                    "Done arranging"
                } else {
                    "Arrange the bar"
                })
                .clicked()
            {
                let id = Id::new("zervo_navbar_config");
                ctx.data_mut(|data| data.insert_temp(id, !config));
            }
        }
    }

    // ── Centre: the address pill, centred on the window rather than on the
    // space left between the two groups, so it does not drift as they change.
    let width = navbar_pill_width(root, chrome, actions, row);
    let pill = Rect::from_center_size(
        pos2(window.center().x, row.center().y),
        vec2(width, NAVBAR_PILL_HEIGHT),
    );
    let mut host = root.new_child(egui::UiBuilder::new().max_rect(pill));
    draw_address_pill(&mut host, chrome, actions, NAVBAR_PILL_HEIGHT);

    // ── The shelf the extra height uncovers.
    if shelf_open {
        let shelf = Rect::from_min_max(
            pos2(window.left() + theme::CONTENT_MARGIN, row.max.y),
            pos2(
                window.right() - theme::CONTENT_MARGIN,
                strip.max.y - SHELF_BOTTOM,
            ),
        );
        // A recess rather than another card: the widgets are the cards, and
        // two levels of card inside each other reads as clutter.
        root.painter().rect_filled(
            shelf,
            CornerRadius::same(theme::CONTENT_RADIUS as u8),
            palette.shadow.gamma_multiply(0.10),
        );
        for change in crate::dashboard::draw(
            root,
            &palette,
            chrome.media,
            &chrome.settings.navbar_widgets,
            shelf,
        ) {
            actions.push(match change {
                crate::dashboard::Change::Remove(index) => UiAction::RemoveWidget(index),
                crate::dashboard::Change::Swap { a, b } => UiAction::SwapWidgets(a, b),
                crate::dashboard::Change::Place { index, col, row } => {
                    UiAction::PlaceWidget { index, col, row }
                },
                crate::dashboard::Change::Resize(index, size) => {
                    UiAction::ResizeWidget(index, size)
                },
                crate::dashboard::Change::Media(action) => UiAction::MediaAction(action),
            });
        }
    }

    // The menu behind the bar's add button, anchored under it.
    if let Some(anchor) = add_widget {
        let open = Id::new("zervo_navbar_add_widget");
        if root_data_flag(&ctx, open)
            && let Some(kind) = crate::dashboard::add_menu(root, &palette, anchor)
        {
            ctx.data_mut(|data| data.insert_temp(open, false));
            actions.push(UiAction::AddWidget(kind));
        }
    }

    navbar_resize(root, chrome, actions, strip);

    // Reserve the strip so the content card starts below it.
    root.allocate_rect(strip, Sense::hover());

    // Favourites hover card last, over everything else in the bar. Not while
    // arranging, when the star is something to move rather than to use.
    if !config {
        let anchor = |want: NavItem| {
            placed
                .iter()
                .find(|(_, _, item, _)| *item == want)
                .map(|(.., rect)| *rect)
        };
        if let Some(star) = anchor(NavItem::Favourite) {
            draw_favourites_card(root, chrome, actions, star);
        }
        if let Some(tray) = anchor(NavItem::Downloads) {
            draw_downloads_card(root, chrome, actions, tray);
        }
    }
}

/// Arrange mode: the bar's own buttons become things to move, not to press.
///
/// Items are dragged between and within the two groups, an insertion caret
/// shows where one would land, and the x takes it off the bar. Whatever has
/// been taken off waits in a tray underneath, where it can be put back.
fn draw_navbar_config(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    placed: &[(NavSide, usize, NavItem, Rect)],
    row: Rect,
) {
    let palette = chrome.palette;
    let ctx = root.ctx().clone();
    let drag_id = Id::new("zervo_nav_drag");
    let dragging = ctx.data(|data| data.get_temp::<NavItem>(drag_id));
    let pointer = ctx.input(|input| input.pointer.latest_pos());

    // Where a drop would insert: before or after whichever item the pointer is
    // nearest, on that item's side.
    let target = pointer.and_then(|pos| {
        placed
            .iter()
            .filter(|(.., rect)| {
                pos.y >= row.min.y && pos.y <= row.max.y && (pos.x - rect.center().x).abs() < 60.0
            })
            .min_by(|a, b| {
                let da = (pos.x - a.3.center().x).abs();
                let db = (pos.x - b.3.center().x).abs();
                da.total_cmp(&db)
            })
            .map(|(side, index, _, rect)| {
                (
                    *side,
                    if pos.x > rect.center().x {
                        index + 1
                    } else {
                        *index
                    },
                    *rect,
                    pos.x > rect.center().x,
                )
            })
    });

    // The whole row reads as editable while this is on.
    root.painter().rect_filled(
        row,
        CornerRadius::same(8),
        palette.accent.gamma_multiply(0.06),
    );

    for (slot, (_, _, item, rect)) in placed.iter().enumerate() {
        let response = root.interact(*rect, drag_id.with(slot), Sense::click_and_drag());
        let held = dragging == Some(*item);
        if response.drag_started() {
            ctx.data_mut(|data| data.insert_temp(drag_id, *item));
        }
        if held && response.drag_stopped() {
            ctx.data_mut(|data| data.remove::<NavItem>(drag_id));
            if let Some((side, index, ..)) = target {
                actions.push(UiAction::MoveNavItem {
                    item: *item,
                    side,
                    index,
                });
            }
        }
        if response.hovered() || held {
            ctx.set_cursor_icon(if held {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            });
        }

        let drawn = match (held, pointer) {
            (true, Some(pos)) => Rect::from_center_size(pos, rect.size()),
            _ => *rect,
        };
        root.painter().rect_filled(
            drawn,
            CornerRadius::same(8),
            palette.surface.gamma_multiply(if held { 1.0 } else { 0.7 }),
        );
        root.painter().rect_stroke(
            drawn,
            CornerRadius::same(8),
            Stroke::new(1.0_f32, palette.accent.gamma_multiply(0.55)),
            StrokeKind::Inside,
        );
        icons::draw_icon(
            root.painter(),
            Rect::from_center_size(drawn.center(), vec2(NAVBAR_ICON, NAVBAR_ICON)),
            item.icon(),
            palette.text,
        );

        // Taking it off the bar.
        let close =
            Rect::from_center_size(pos2(drawn.max.x - 2.0, drawn.min.y + 2.0), vec2(13.0, 13.0));
        icons::draw_icon(
            root.painter(),
            close.shrink(1.5),
            Icon::Close,
            palette.text_muted,
        );
        if !held
            && root
                .interact(close, drag_id.with(("off", slot)), Sense::click())
                .on_hover_text(format!("Remove {}", item.label()))
                .clicked()
        {
            actions.push(UiAction::RemoveNavItem(*item));
        }
    }

    // Where it would land.
    if dragging.is_some()
        && let Some((_, _, rect, after)) = target
    {
        let x = if after {
            rect.max.x + 1.0
        } else {
            rect.min.x - 1.0
        };
        root.painter().rect_filled(
            Rect::from_center_size(pos2(x, rect.center().y), vec2(2.0, rect.height() + 6.0)),
            CornerRadius::same(1),
            palette.accent,
        );
    }

    // ── The tray of everything currently off the bar.
    let hidden: Vec<NavItem> = NavItem::ALL
        .iter()
        .copied()
        .filter(|item| !placed.iter().any(|(_, _, placed, _)| placed == item))
        .collect();
    let width = (hidden.len().max(1) as f32) * (nav_item_width() + 4.0) + 150.0;
    let tray = crate::ui::clamp_into(
        Rect::from_min_size(
            pos2(row.center().x - width / 2.0, row.max.y + 8.0),
            vec2(width, 40.0),
        ),
        ctx.content_rect(),
    );
    let palette = palette.over(tray);
    egui::Area::new(Id::new("zervo_nav_tray"))
        .order(egui::Order::Foreground)
        .fixed_pos(tray.min)
        .constrain(false)
        .show(&ctx, |ui| {
            for shape in glass::shapes(tray, &palette, Glass::of(Surface::Menu).opaque(palette.bg))
            {
                ui.painter().add(shape);
            }
            ui.painter().text(
                pos2(tray.min.x + 12.0, tray.center().y),
                Align2::LEFT_CENTER,
                if hidden.is_empty() {
                    "Drag to rearrange"
                } else {
                    "Drag to rearrange, or add back:"
                },
                FontId::proportional(12.0),
                palette.text_muted,
            );
            let mut x = tray.min.x + 150.0;
            for item in &hidden {
                let slot = Rect::from_min_size(
                    pos2(x, tray.center().y - (NAVBAR_ICON + 8.0) / 2.0),
                    vec2(nav_item_width(), NAVBAR_ICON + 8.0),
                );
                let response =
                    ui.interact(slot, Id::new("zervo_nav_back").with(*item), Sense::click());
                if response.hovered() {
                    ui.painter()
                        .rect_filled(slot, CornerRadius::same(8), palette.surface_hover);
                }
                icons::draw_icon(
                    ui.painter(),
                    Rect::from_center_size(slot.center(), vec2(NAVBAR_ICON, NAVBAR_ICON)),
                    item.icon(),
                    palette.text_muted,
                );
                if response
                    .on_hover_text(item.label())
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .clicked()
                {
                    actions.push(UiAction::AddNavItem(*item));
                }
                x += nav_item_width() + 4.0;
            }
            ui.advance_cursor_after_rect(tray);
        });
}

/// One bar button, doing whatever it does.
fn draw_nav_item(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    item: NavItem,
    rect: Rect,
) {
    let palette = chrome.palette;
    let (can_go_back, can_go_forward, is_web) = chrome
        .browser
        .active_tab()
        .map(|tab| {
            (
                tab.can_go_back,
                tab.can_go_forward,
                tab.kind == TabKind::Web,
            )
        })
        .unwrap_or_default();

    let mut host = root.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );

    // The star draws itself: it has a saved state and a hover card.
    if item == NavItem::Favourite {
        draw_favourite_star(&mut host, chrome, actions);
        return;
    }

    let enabled = match item {
        NavItem::Back => can_go_back,
        NavItem::Forward => can_go_forward,
        NavItem::Reload => is_web,
        // Servo has no extension support, and a button pretending otherwise
        // would be worse than one that says so.
        NavItem::Extensions => false,
        _ => true,
    };
    let response = icons::icon_button(&mut host, item.icon(), NAVBAR_ICON, &palette, enabled);
    let active = chrome.downloads.active_count();
    let response = match item {
        NavItem::Sidebar => response.on_hover_text("Show sidebar"),
        NavItem::Reload => response.on_hover_text("Reload (⌘R)"),
        NavItem::Settings => response.on_hover_text("Settings (⌘,)"),
        NavItem::History => response.on_hover_text("History"),
        NavItem::Extensions => {
            response.on_hover_text("Extensions — not supported by the engine yet")
        },
        NavItem::Downloads => response.on_hover_text(if active > 0 {
            format!("Downloads ({active} active)")
        } else {
            "Downloads".to_owned()
        }),
        _ => response,
    };
    if response.clicked() {
        actions.push(match item {
            NavItem::Sidebar => UiAction::ToggleSidebar,
            NavItem::Back => UiAction::Back,
            NavItem::Forward => UiAction::Forward,
            NavItem::Reload => UiAction::Reload,
            NavItem::History => UiAction::OpenHistory,
            NavItem::Downloads => UiAction::OpenDownloads,
            NavItem::Settings => UiAction::OpenSettings,
            NavItem::Extensions | NavItem::Favourite => return,
        });
    }
}

/// Drag the bar's bottom edge to change its height. The room this opens up is
/// the point: widgets — a player, and whatever else earns a place — will live
/// under the controls, and there is no sense adding a menu for them before
/// there is anywhere to put them.
/// The navigation bar's height with its widget shelf fully uncovered — the
/// height dragging it open snaps to, and the one a gesture opens it to.
pub fn shelf_uncovered_height(widgets: &[crate::dashboard::Placed]) -> f32 {
    NAVBAR_ROW + crate::dashboard::open_height(widgets) + SHELF_BOTTOM
}

fn navbar_resize(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    strip: Rect,
) {
    let stored = chrome
        .settings
        .navbar_height
        .clamp(NAVBAR_ROW, NAVBAR_MAX_HEIGHT);
    let grip = Rect::from_min_max(
        pos2(strip.min.x, strip.max.y - 5.0),
        pos2(strip.max.x, strip.max.y + 1.0),
    );
    let response = root.interact(grip, Id::new("zervo_navbar_resize"), Sense::drag());
    let emphasis = glass::ease_out(root.ctx().animate_bool_with_time(
        Id::new("zervo_navbar_grabber"),
        response.hovered() || response.dragged(),
        0.12,
    ));
    if response.hovered() || response.dragged() {
        root.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
    }
    // Drawn whether or not it is hovered: an affordance nobody can see is not
    // an affordance, and there is nothing else to say the widgets are down
    // there.
    crate::dashboard::draw_grabber(root.painter(), &chrome.palette, strip, emphasis);

    // Closed, or open far enough to show a whole widget. Those are the two
    // heights worth resting at, so a release near either lands on it — the
    // shelf ends up uncovered rather than cut off mid-card. Anywhere else is
    // left alone, in case someone wants it there.
    let uncovered = shelf_uncovered_height(&chrome.settings.navbar_widgets);
    let mut height = stored;
    if response.dragged() {
        height = (height + response.drag_delta().y).clamp(NAVBAR_ROW, NAVBAR_MAX_HEIGHT);
        chrome.settings.navbar_height = height;
    }
    if response.drag_stopped() {
        for rest in [NAVBAR_ROW, uncovered] {
            if (height - rest).abs() < 18.0 {
                height = rest;
                chrome.settings.navbar_height = height;
            }
        }
    }
    // Written once the drag ends, not every frame of it.
    if !root.ctx().egui_is_using_pointer() && (height - stored).abs() > 0.5 {
        chrome.settings.navbar_height = height;
        actions.push(UiAction::PersistSettings);
    }
}

/// A remembered boolean, for the little bits of open/closed state that do not
/// deserve a field of their own.
fn root_data_flag(ctx: &egui::Context, id: Id) -> bool {
    ctx.data(|data| data.get_temp::<bool>(id)).unwrap_or(false)
}

/// The pill's width, draggable by either edge. Dragging one edge moves the
/// other by the same amount, since the pill stays centred.
fn navbar_pill_width(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    strip: Rect,
) -> f32 {
    let stored = chrome
        .settings
        .address_pill_width
        .clamp(ADDRESS_PILL_MIN_WIDTH, ADDRESS_PILL_MAX_WIDTH);
    // Never let it grow into the button groups.
    let room = (strip.width() - 2.0 * (TRAFFIC_LIGHTS + 170.0)).max(ADDRESS_PILL_MIN_WIDTH);
    let mut width = stored.min(room);

    let centre = strip.center();
    for (index, side) in [(-1.0_f32), 1.0].iter().enumerate() {
        let edge = Rect::from_center_size(
            pos2(centre.x + side * width / 2.0, centre.y),
            vec2(10.0, NAVBAR_PILL_HEIGHT),
        );
        let response = root.interact(
            edge,
            Id::new("zervo_pill_resize").with(index),
            Sense::drag(),
        );
        if response.hovered() || response.dragged() {
            root.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        if response.dragged() {
            width = (width + side * response.drag_delta().x * 2.0)
                .clamp(ADDRESS_PILL_MIN_WIDTH, ADDRESS_PILL_MAX_WIDTH.min(room));
        }
    }

    // Written once the drag ends rather than every frame of it.
    if !root.ctx().egui_is_using_pointer() && (width - stored).abs() > 0.5 {
        chrome.settings.address_pill_width = width;
        actions.push(UiAction::PersistSettings);
    }
    width
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
    let busy = state.open && (ctx.egui_is_using_pointer() || egui::Popup::is_any_open(ctx));
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
            // A peek only happens while collapsed, which is exactly when the
            // navigation bar is up.
            sidebar_body(&mut body, chrome, actions, true);
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

/// `height` because the bar and the sidebar want different ones: in the bar it
/// sits in a row of 25pt buttons, and a 36pt pill beside those reads as
/// misaligned however well its centre lines up — it has 2pt of air where they
/// have seven, so it looks like it is falling out of the top of the window.
fn draw_address_pill(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    height: f32,
) {
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

    let (pill_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());

    let radius = (height * 0.32) as u8;
    glass::paint(
        ui.painter(),
        pill_rect,
        &palette,
        Glass::new(radius).glow(focus_t),
    );
    if focus_t > 0.0 {
        ui.painter().rect_stroke(
            pill_rect,
            CornerRadius::same(radius),
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
    // What the leading icon says about the connection. While typing it goes
    // back to a magnifying glass, since it is describing the box and not a
    // page any more.
    let (icon, tint, hint) = if chrome.browser.editing_address {
        (Icon::Search, palette.text_muted, "")
    } else {
        let url = chrome.browser.active_tab().map(|tab| tab.url.as_str());
        match url.map(|url| url.split(':').next().unwrap_or("")) {
            Some("https") => (Icon::Lock, palette.text_muted, "Encrypted connection"),
            Some("http") => (
                Icon::Warning,
                theme::mix(palette.text_muted, Color32::from_rgb(220, 138, 40), 0.75),
                "Not encrypted — anyone on the network can read this",
            ),
            Some("file") => (Icon::Folder, palette.text_muted, "A file on this computer"),
            _ => (Icon::Globe, palette.text_muted, ""),
        }
    };
    let badge = Rect::from_center_size(
        pos2(inner.max_rect().min.x + 8.0, pill_rect.center().y),
        vec2(15.0, 15.0),
    );
    icons::draw_icon(inner.painter(), badge, icon, tint);
    if !hint.is_empty() {
        inner
            .interact(badge.expand(4.0), pill_id.with("security"), Sense::hover())
            .on_hover_text(hint);
    }
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
                    Glass::tier(Tier::Card)
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

/// What the user did to a workspace's name this frame.
enum WorkspaceName {
    Typing(String),
    Keep(String),
    Discard,
}

#[expect(clippy::too_many_arguments)]
fn workspace_header(
    ui: &mut Ui,
    index: usize,
    name: &str,
    tab_count: usize,
    show_count: bool,
    active: bool,
    editing: Option<&str>,
    palette: &Palette,
    actions: &mut Vec<UiAction>,
    rename: &mut Option<(usize, WorkspaceName)>,
) -> Rect {
    let desired = vec2(ui.available_width(), 28.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    if response.clicked() && editing.is_none() {
        actions.push(UiAction::SelectWorkspace(index));
    }

    let hover_t = glass::ease_out(ui.ctx().animate_bool_with_time(
        ui.id().with(("ws_header", index)),
        response.hovered(),
        0.12,
    ));
    if hover_t > 0.0 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(8),
            palette.surface_hover.gamma_multiply(hover_t),
        );
    }
    ui.painter().circle_filled(
        pos2(rect.min.x + 12.0, rect.center().y),
        4.0,
        theme::workspace_color(index),
    );

    // Named inline rather than in a dialog. A workspace made by dropping one
    // tab on another already exists by the time the user is asked what it is,
    // so a modal would be asking permission for something already done — and
    // the same tick-and-cross edit is what renaming a favourite uses.
    if let Some(draft) = editing {
        let mut text = draft.to_owned();
        let field = Rect::from_min_max(
            pos2(rect.min.x + 24.0, rect.min.y + 2.0),
            pos2(rect.max.x - 44.0, rect.max.y - 2.0),
        );
        let editor = ui.put(
            field,
            TextEdit::singleline(&mut text)
                .frame(Frame::NONE)
                .font(FontId::proportional(13.0))
                .text_color(palette.text)
                .id(ui.id().with(("ws_name", index))),
        );
        editor.request_focus();

        let control = |ui: &mut Ui, x: f32, icon: Icon, tint: Color32| -> bool {
            let hit = Rect::from_center_size(pos2(x, rect.center().y), vec2(18.0, 18.0));
            let response = ui.interact(
                hit,
                ui.id().with((icon as usize, "ws_edit", index)),
                Sense::click(),
            );
            icons::draw_icon(
                ui.painter(),
                Rect::from_center_size(hit.center(), vec2(11.0, 11.0)),
                icon,
                if response.hovered() {
                    tint
                } else {
                    palette.text_muted
                },
            );
            response.on_hover_cursor(CursorIcon::PointingHand).clicked()
        };
        let keep = control(ui, rect.max.x - 32.0, Icon::Check, palette.accent);
        let discard = control(ui, rect.max.x - 12.0, Icon::Close, palette.text);

        let entered = ui.input(|input| input.key_pressed(egui::Key::Enter));
        let escaped = ui.input(|input| input.key_pressed(egui::Key::Escape));
        // Only when something actually happened. Writing Typing(text) every
        // frame would overwrite a rename the context menu asked for on any
        // workspace drawn after this one, since there is a single slot and the
        // last writer wins — and the menu is always drawn later.
        if keep || entered {
            *rename = Some((index, WorkspaceName::Keep(text)));
        } else if discard || escaped {
            *rename = Some((index, WorkspaceName::Discard));
        } else if text != draft {
            *rename = Some((index, WorkspaceName::Typing(text)));
        }
        return rect;
    }

    ui.painter().text(
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
        ui.painter().text(
            pos2(rect.max.x - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            tab_count.to_string(),
            FontId::proportional(11.5),
            palette.text_muted,
        );
    }
    // Renaming any workspace, not only a freshly made one.
    response.clone().context_menu(|ui| {
        if ui.button("Rename").clicked() {
            *rename = Some((index, WorkspaceName::Typing(name.to_owned())));
            ui.close();
        }
    });
    response.on_hover_cursor(CursorIcon::PointingHand);
    rect
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
    /// The tab being dragged, if any — including possibly this one.
    dragging: Option<TabId>,
}

/// Where a drop on a tab row would put the dragged tab.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DropAt {
    Before,
    /// On the row itself, rather than between two: the two tabs become a
    /// workspace of their own.
    Onto,
    After,
}

/// What a tab row did this frame. More than the two booleans it used to
/// return, because the sidebar now has to resolve a drag across every row in
/// every workspace before it can act on one.
struct TabRowOut {
    clicked: bool,
    close_clicked: bool,
    drag_started: bool,
    drag_stopped: bool,
    /// Set while a drag is in flight and the pointer is over this row.
    drop_at: Option<DropAt>,
}

/// The dragged row, following the pointer.
///
/// Deliberately less than a real row — no close button, no hover state, no
/// tooltip. It is a token showing what is in hand, not a control.
fn paint_tab_ghost(
    painter: &egui::Painter,
    rect: Rect,
    title: &str,
    favicon: Option<&TextureHandle>,
    palette: &Palette,
) {
    for shape in glass::shapes(
        rect,
        palette,
        Glass::tier(Tier::Row)
            .tint(palette.active)
            // It travels over the web page, so nothing shows through it.
            .opaque(palette.bg),
    ) {
        painter.add(shape);
    }
    let icon_center = pos2(rect.min.x + 19.0, rect.center().y);
    if let Some(texture) = favicon {
        painter.image(
            texture.id(),
            Rect::from_center_size(icon_center, vec2(15.0, 15.0)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        icons::draw_icon(
            painter,
            Rect::from_center_size(icon_center, vec2(14.0, 14.0)),
            Icon::Globe,
            palette.text_muted,
        );
    }
    painter.text(
        pos2(rect.min.x + 34.0, rect.center().y),
        Align2::LEFT_CENTER,
        ellipsize(title, 24),
        FontId::proportional(13.0),
        palette.text,
    );
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
) -> TabRowOut {
    let row_height = if style.compact { 28.0 } else { 34.0 };
    let desired = vec2(ui.available_width(), row_height);
    let (_, rect) = ui.allocate_space(desired);
    // Sensing drag as well as click is the whole feature. egui decides between
    // the two itself — a press only becomes a drag past 6 points of travel or
    // 0.8 seconds held — so an ordinary click still reports as a click and no
    // threshold of our own is needed. The id is keyed by tab identity rather
    // than layout position for the same reason the animations below are:
    // closing a tab above must not hand this one's interaction state over.
    let response = ui.interact(
        rect,
        ui.id().with(("tab_row", style.tab_id)),
        Sense::click_and_drag(),
    );
    let pointer = ui.ctx().input(|input| input.pointer.latest_pos());

    if style.dragging == Some(style.tab_id) {
        // The row being dragged is painted last, at the pointer, by the
        // sidebar — so it passes over the rows it is travelling across rather
        // than under them. What is left here is the gap it came out of, which
        // is the clearest drop affordance there is.
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        return TabRowOut {
            clicked: false,
            close_clicked: false,
            drag_started: false,
            drag_stopped: response.drag_stopped(),
            drop_at: None,
        };
    }

    // Three bands: the outer quarters insert above or below, the middle half
    // groups. Half the row is a large enough target to hit deliberately, and
    // the quarters are still 8.5 points — or 7 when the sidebar is compact.
    let drop_at = style
        .dragging
        .and(pointer)
        .filter(|pos| rect.contains(*pos))
        .map(|pos| {
            let t = (pos.y - rect.min.y) / rect.height();
            if t < 0.25 {
                DropAt::Before
            } else if t > 0.75 {
                DropAt::After
            } else {
                DropAt::Onto
            }
        });

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
            Glass::tier(Tier::Row)
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

    // The drop affordance, over everything else the row drew.
    match drop_at {
        Some(DropAt::Onto) => {
            glass::paint(
                ui.painter(),
                rect,
                palette,
                Glass::tier(Tier::Row)
                    .tint(palette.accent)
                    .strength(0.55)
                    .no_shadow(),
            );
        },
        Some(edge) => {
            let y = if edge == DropAt::Before {
                rect.min.y
            } else {
                rect.max.y
            };
            ui.painter().rect_filled(
                Rect::from_min_max(
                    pos2(rect.min.x + 6.0, y - 1.0),
                    pos2(rect.max.x - 6.0, y + 1.0),
                ),
                CornerRadius::same(1),
                palette.accent,
            );
        },
        None => {},
    }

    let row_clicked = response.clicked() && !close_response.clicked();
    let close_clicked = close_response.clicked();
    let drag_started = response.drag_started();
    let drag_stopped = response.drag_stopped();
    if style.dragging.is_none() {
        response
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(style.url);
    }
    TabRowOut {
        clicked: row_clicked,
        close_clicked,
        drag_started,
        drag_stopped,
        drop_at,
    }
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
pub fn draw_zervo_mark(painter: &egui::Painter, center: egui::Pos2, height: f32, color: Color32) {
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
pub fn paint_newtab_background(
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

        // Reached only while a photograph is still being fetched, or when the
        // fetch failed: a fade is a kinder thing to wait on than a flat void.
        NewTabBackground::Photo | NewTabBackground::Gradient => {
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
                            Glass::tier(Tier::Card).strength(0.7).no_shadow(),
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
    // The page as a menu opened over it will see it: the base and the
    // navigation column, and none of the controls that are about to be drawn
    // on them.
    if let Some(capture) = &chrome.capture {
        crate::backdrop::capture_into(&painter, content_rect, capture);
    }

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
                        crate::state::SettingsSection::Layout => {
                            settings_layout(ui, chrome, &palette, actions)
                        },
                        crate::state::SettingsSection::Passwords => {
                            settings_passwords(ui, chrome, actions)
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

    settings_section(ui, palette, "Accent colour", |ui| {
        // A swatch, drawn the same way whether it is a preset or the reader's
        // own: one row of circles, one of which happens to open a picker.
        let swatch = |ui: &mut Ui, color: egui::Color32, selected: bool, custom: bool| {
            let (rect, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
            let centre = rect.center();
            let t = ui.ctx().animate_bool(response.id, selected);
            ui.painter().circle_filled(centre, 10.0 + 2.0 * t, color);
            if custom {
                // A ring, so the one that opens a picker does not read as
                // simply another colour to choose.
                ui.painter().circle_stroke(
                    centre,
                    10.0 + 2.0 * t,
                    Stroke::new(1.5_f32, palette.bg),
                );
                icons::draw_icon(
                    ui.painter(),
                    Rect::from_center_size(centre, vec2(11.0, 11.0)),
                    Icon::Pencil,
                    if color_is_light(color) {
                        egui::Color32::from_black_alpha(150)
                    } else {
                        egui::Color32::from_white_alpha(200)
                    },
                );
            }
            if t > 0.0 {
                ui.painter()
                    .circle_stroke(centre, 14.0, Stroke::new(1.5 + 0.5 * t, palette.text));
            }
            response
        };

        ui.horizontal(|ui| {
            // ── The reader's own, first.
            let custom_selected = matches!(chrome.settings.accent, AccentColor::Custom(..));
            let mut rgb = chrome.settings.accent.rgb(palette.dark);
            let response = swatch(
                ui,
                egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
                custom_selected,
                true,
            );
            let picker = Id::new("zervo_accent_picker");
            if response
                .clone()
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text("Mix your own")
                .clicked()
            {
                // Opening it also selects it, seeded from whatever is in force
                // — so the picker opens on the colour being replaced rather
                // than on an arbitrary one.
                chrome.settings.accent = AccentColor::Custom(rgb[0], rgb[1], rgb[2]);
                actions.push(UiAction::SettingsChanged);
                let open = ui.ctx().data(|data| data.get_temp::<bool>(picker)) == Some(true);
                ui.ctx().data_mut(|data| data.insert_temp(picker, !open));
            }
            if ui.ctx().data(|data| data.get_temp::<bool>(picker)) == Some(true) {
                let mut colour = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                egui::Area::new(picker.with("area"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(response.rect.left_bottom() + vec2(0.0, 6.0))
                    .constrain(true)
                    .show(ui.ctx(), |ui| {
                        Frame::popup(ui.style()).show(ui, |ui| {
                            if egui::color_picker::color_picker_color32(
                                ui,
                                &mut colour,
                                egui::color_picker::Alpha::Opaque,
                            ) {
                                rgb = [colour.r(), colour.g(), colour.b()];
                                chrome.settings.accent =
                                    AccentColor::Custom(rgb[0], rgb[1], rgb[2]);
                                actions.push(UiAction::SettingsChanged);
                            }
                            if ui.button("Done").clicked() {
                                ui.ctx().data_mut(|data| data.insert_temp(picker, false));
                            }
                        });
                    });
            }

            // ── The presets.
            for accent in AccentColor::PRESETS {
                let selected = chrome.settings.accent == accent;
                if swatch(ui, accent.color(palette.dark), selected, false)
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
            RichText::new("Material")
                .size(12.0)
                .color(palette.text_muted),
        );
        let levels = crate::theme::Translucency::ALL;
        let labels: Vec<&str> = levels.iter().map(|level| level.label()).collect();
        let current = levels
            .iter()
            .position(|level| *level == chrome.settings.translucency)
            .unwrap_or(0);
        if let Some(index) = widgets::segmented(ui, current, &labels, palette) {
            chrome.settings.translucency = levels[index];
            actions.push(UiAction::SettingsChanged);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(chrome.settings.translucency.note())
                .size(11.5)
                .color(palette.text_muted),
        );
        ui.add_space(2.0);
        ui.label(
            RichText::new(
                "Everything the material draws — the window's own chrome, the cards, \
                 the menus, the shelf, the new tab page.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );

        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Frosted asks the system for the backdrop behind the window and tints \
                 it. Solid asks for none and paints over it.",
            )
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

    settings_section(ui, palette, "Downloads", |ui| {
        if widgets::toggle(
            ui,
            &mut chrome.settings.downloads_auto,
            "Save files without asking where",
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "Saved to {}",
                crate::downloads::downloads_dir().display()
            ))
            .size(11.5)
            .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, "Compatibility", |ui| {
        if widgets::toggle(
            ui,
            &mut chrome.settings.user_agent_compat,
            "Present as plain Firefox",
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Servo's own user agent already claims Firefox, but keeps a Servo token \
                 and omits Gecko — and enough sites match on exactly those to turn you \
                 away. Takes effect on the next launch.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );
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
}

/// One swipe direction and what it does.
fn gesture_row(
    ui: &mut Ui,
    label: &str,
    hint: &str,
    slot: &mut crate::gestures::GestureAction,
    palette: &Palette,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(13.0).color(palette.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::ComboBox::from_id_salt(("gesture", label))
                .selected_text(slot.label())
                .show_ui(ui, |ui| {
                    for option in crate::gestures::GestureAction::ALL {
                        if ui
                            .selectable_label(*slot == option, option.label())
                            .clicked()
                        {
                            *slot = option;
                            changed = true;
                        }
                    }
                });
        });
    });
    if !hint.is_empty() {
        ui.label(RichText::new(hint).size(11.0).color(palette.text_muted));
    }
    ui.add_space(6.0);
    changed
}

fn settings_layout(
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
    settings_section(ui, palette, "New tab page", |ui| {
        ui.label(
            RichText::new(
                "The cards are arranged on the page itself — press Customise there to \
                 move, resize and remove them.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );
        ui.add_space(10.0);
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
            actions.push(UiAction::PersistSettings);
        }
    });

    settings_section(ui, palette, "World clocks", |ui| {
        let mut remove = None;
        for (index, zone) in chrome.settings.newtab_world_clocks.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&zone.label).size(13.0).color(palette.text));
                ui.label(
                    RichText::new(&zone.name)
                        .size(11.5)
                        .color(palette.text_muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::icon_button(ui, Icon::Close, 13.0, palette, true)
                        .on_hover_text("Take this city off")
                        .clicked()
                    {
                        remove = Some(index);
                    }
                });
            });
        }
        if let Some(index) = remove {
            chrome.settings.newtab_world_clocks.remove(index);
            actions.push(UiAction::PersistSettings);
        }
        if chrome.settings.newtab_world_clocks.is_empty() {
            ui.label(
                RichText::new("No cities — the card says so rather than showing nothing.")
                    .size(11.5)
                    .color(palette.text_muted),
            );
        }
        ui.add_space(6.0);
        egui::ComboBox::from_id_salt("zervo_world_clock_add")
            .selected_text("Add a city…")
            .width(220.0)
            .show_ui(ui, |ui| {
                for (label, name) in crate::newtab::Zone::CATALOGUE {
                    let already = chrome
                        .settings
                        .newtab_world_clocks
                        .iter()
                        .any(|zone| zone.name == name);
                    if already {
                        continue;
                    }
                    if ui.selectable_label(false, label).clicked() {
                        chrome
                            .settings
                            .newtab_world_clocks
                            .push(crate::newtab::Zone {
                                label: label.to_owned(),
                                name: name.to_owned(),
                            });
                        actions.push(UiAction::PersistSettings);
                    }
                }
            });
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Each clock reads the zone from the compiled-in IANA table, so daylight \
                 saving is right without anyone maintaining it.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );
    });

    settings_section(ui, palette, "Wallpaper", |ui| {
        use crate::wallpaper::{Cadence, Source, Subject};
        let photo = chrome.settings.new_tab_background == NewTabBackground::Photo;
        let mut wants_photo = photo;
        if widgets::toggle(ui, &mut wants_photo, "Show a photograph", palette) {
            chrome.settings.new_tab_background = if wants_photo {
                NewTabBackground::Photo
            } else {
                NewTabBackground::Aurora
            };
            actions.push(UiAction::PersistSettings);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Pictures come from Wikimedia Commons and Openverse, which publish under \
                 licences that allow this. Neither needs an account. The credit line under \
                 the page is part of the licence, so it is always drawn.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );

        ui.add_space(10.0);
        ui.label(RichText::new("Source").size(12.0).color(palette.text_muted));
        let mut sources: Vec<(String, Source)> =
            vec![("Commons picture of the day".to_owned(), Source::Commons)];
        sources.extend(Subject::ALL.iter().map(|subject| {
            (
                format!("Openverse — {}", subject.label().to_lowercase()),
                Source::Openverse(*subject),
            )
        }));
        let selected = sources
            .iter()
            .find(|(_, source)| *source == chrome.settings.wallpaper_source)
            .map(|(label, _)| label.clone())
            .unwrap_or_else(|| chrome.settings.wallpaper_source.label());
        egui::ComboBox::from_id_salt("zervo_wallpaper_source")
            .selected_text(selected)
            .width(260.0)
            .show_ui(ui, |ui| {
                for (label, source) in sources {
                    if ui
                        .selectable_label(chrome.settings.wallpaper_source == source, label)
                        .clicked()
                    {
                        chrome.settings.wallpaper_source = source;
                        actions.push(UiAction::ShuffleWallpaper);
                    }
                }
            });

        ui.add_space(10.0);
        ui.label(
            RichText::new("Change it")
                .size(12.0)
                .color(palette.text_muted),
        );
        let cadences = Cadence::ALL;
        let labels: Vec<&str> = cadences.iter().map(|cadence| cadence.label()).collect();
        let current = cadences
            .iter()
            .position(|cadence| *cadence == chrome.settings.wallpaper_cadence)
            .unwrap_or(0);
        if let Some(index) = widgets::segmented(ui, current, &labels, palette) {
            chrome.settings.wallpaper_cadence = cadences[index];
            actions.push(UiAction::PersistSettings);
        }

        ui.add_space(10.0);
        ui.label(RichText::new("Veil").size(12.0).color(palette.text_muted));
        if widgets::slider(ui, &mut chrome.settings.wallpaper_dim, 0.15..=0.9, palette) {
            actions.push(UiAction::PersistSettings);
        }
        ui.label(
            RichText::new(format!(
                "{:.0}% — how far the picture is dimmed so the cards stay readable on it.",
                chrome.settings.wallpaper_dim * 100.0
            ))
            .size(11.5)
            .color(palette.text_muted),
        );

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Another picture").clicked() {
                actions.push(UiAction::ShuffleWallpaper);
            }
            if ui.button("Choose a file…").clicked() {
                actions.push(UiAction::PickWallpaper);
            }
        });
        ui.add_space(4.0);
        let credit = chrome.wallpaper.credit;
        let note = if let Some(why) = chrome.wallpaper.error {
            format!("The last attempt failed: {why}")
        } else if chrome.wallpaper.loading {
            "Fetching one…".to_owned()
        } else if chrome.wallpaper.texture.is_some() {
            format!("Showing {} — from {}.", credit.line(), credit.source)
        } else {
            "Nothing fetched yet.".to_owned()
        };
        ui.label(RichText::new(note).size(11.5).color(palette.text_muted));
    });

    settings_section(ui, palette, "Trackpad", |ui| {
        if widgets::toggle(
            ui,
            &mut chrome.settings.gestures.enabled,
            "Two-finger swipes",
            palette,
        ) {
            actions.push(UiAction::SettingsChanged);
        }
        ui.label(
            RichText::new(
                "A quick, straight flick. A slow or wandering one is a scroll and is \
                 left alone.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );
        if chrome.settings.gestures.enabled {
            ui.add_space(10.0);
            let mut changed = false;
            changed |= gesture_row(
                ui,
                "Swipe right",
                "Anywhere in the window.",
                &mut chrome.settings.gestures.right,
                palette,
            );
            changed |= gesture_row(
                ui,
                "Swipe left",
                "",
                &mut chrome.settings.gestures.left,
                palette,
            );
            changed |= gesture_row(
                ui,
                "Swipe down",
                "Over the bar above the page — everywhere else this scrolls.",
                &mut chrome.settings.gestures.down,
                palette,
            );
            changed |= gesture_row(
                ui,
                "Swipe up",
                "",
                &mut chrome.settings.gestures.up,
                palette,
            );
            if changed {
                actions.push(UiAction::SettingsChanged);
            }
        }
    });

    settings_section(ui, palette, "Arrangement", |ui| {
        ui.label(
            RichText::new(
                "The navigation bar, its widgets, and the widths of the sidebar and \
                 address bar are all arranged by dragging them rather than set here.",
            )
            .size(11.5)
            .color(palette.text_muted),
        );
        ui.add_space(8.0);
        if ui
            .button("Reset to defaults")
            .on_hover_text("Puts every bar button, widget and width back")
            .clicked()
        {
            actions.push(UiAction::ResetLayout);
        }
    });
}

/// Saved logins.
///
/// Deliberately plain about what this can and cannot do: Servo gives the
/// embedder no way to see a submitted form or write into a page, so there is no
/// autofill to offer and pretending otherwise would be worse than saying so.
fn settings_passwords(ui: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    let palette = chrome.palette;

    settings_section(ui, &palette, "Saved logins", |ui| {
        ui.label(
            RichText::new(
                "Passwords are kept in your system keychain, never in Zervo's own files. \
                 Zervo cannot fill them into web forms — the engine provides no way to \
                 do that — but it does use them when a site asks for HTTP authentication.",
            )
            .size(12.0)
            .color(palette.text_muted),
        );
        ui.add_space(10.0);

        if chrome.vault.is_empty() {
            ui.label(
                RichText::new("Nothing saved yet.")
                    .size(13.0)
                    .color(palette.text_muted),
            );
        }
        let logins: Vec<(String, String)> = chrome
            .vault
            .logins()
            .iter()
            .map(|login| (login.site.clone(), login.username.clone()))
            .collect();
        for (site, username) in logins {
            let (row, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
            if response.hovered() {
                ui.painter()
                    .rect_filled(row, CornerRadius::same(7), palette.surface_hover);
            }
            ui.painter().text(
                pos2(row.min.x + 6.0, row.center().y),
                Align2::LEFT_CENTER,
                &site,
                FontId::proportional(13.0),
                palette.text,
            );
            ui.painter().text(
                pos2(row.min.x + 6.0 + 170.0, row.center().y),
                Align2::LEFT_CENTER,
                &username,
                FontId::proportional(12.5),
                palette.text_muted,
            );
            let remove =
                Rect::from_center_size(pos2(row.max.x - 14.0, row.center().y), vec2(18.0, 18.0));
            icons::draw_icon(
                ui.painter(),
                remove.shrink(4.0),
                Icon::Trash,
                palette.text_muted,
            );
            if ui
                .interact(
                    remove,
                    Id::new("zervo_pw_remove").with((&site, &username)),
                    Sense::click(),
                )
                .on_hover_text("Forget this login")
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                actions.push(UiAction::RemovePassword(site.clone(), username.clone()));
            }
        }
    });

    ui.add_space(14.0);
    settings_section(ui, &palette, "Add a login", |ui| {
        let field = |ui: &mut Ui, label: &str, value: &mut String, secret: bool| {
            ui.horizontal(|ui| {
                ui.allocate_ui(vec2(90.0, 26.0), |ui| {
                    ui.label(RichText::new(label).size(12.5).color(palette.text_muted));
                });
                ui.add(
                    TextEdit::singleline(value)
                        .password(secret)
                        .font(FontId::proportional(13.0))
                        .desired_width(ui.available_width().min(260.0)),
                );
            });
        };
        let draft = &mut chrome.browser.password_draft;
        field(ui, "Site", &mut draft.0, false);
        field(ui, "Username", &mut draft.1, false);
        field(ui, "Password", &mut draft.2, true);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Save login").clicked() {
                actions.push(UiAction::SavePassword);
            }
            ui.add_space(6.0);
            if ui.button("Import…").clicked() {
                actions.push(UiAction::ImportPasswords);
            }
            if ui
                .button("Export…")
                .on_hover_text("Writes every password to a plain, unencrypted file")
                .clicked()
            {
                actions.push(UiAction::ExportPasswords);
            }
        });
        if !chrome.browser.password_notice.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(chrome.browser.password_notice.clone())
                    .size(12.0)
                    .color(palette.text_muted),
            );
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
        ui.label(
            RichText::new(format!(
                "Material: {} — corner radii, fills, edges and shadows all come from it.",
                palette.material.name
            ))
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
                Glass::tier(Tier::Card).strength(0.8),
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
        || input.starts_with("file://")
        || input.starts_with("about:")
        || input.starts_with("zervo://")
    {
        input.to_owned()
    } else if input.starts_with('/') || input.starts_with("~/") {
        // An absolute path is a local file, not a search. Without this a
        // `file://` URL fell through to the branch below and became
        // `https://file:///...`, so local files could not be opened at all.
        let path = if let Some(rest) = input.strip_prefix("~/") {
            match std::env::var("HOME") {
                Ok(home) => format!("{home}/{rest}"),
                Err(_) => input.to_owned(),
            }
        } else {
            input.to_owned()
        };
        format!("file://{path}")
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
