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
use crate::settings::Settings;
use crate::state::{BrowserState, TabId, TabKind};
use crate::theme::{self, Palette, Surface, Tier};

pub(crate) mod backdrops;
mod frame;
mod settings_page;
mod setup;
mod text;

// The chrome stays one namespace, to itself and to everything outside it:
// `ui::ellipsize`, `ui::finish_content_frame`, `ui::paint_newtab_background`
// and the rest are where they always were, whichever file they now live in.
// The globs are for this module's own use; the named re-exports are the ones
// the rest of the tree asks for by name.
use frame::*;
use settings_page::*;
use text::*;

pub use backdrops::{draw_zervo_mark, paint_newtab_background};
pub use frame::{CardFrame, Corners, PANEL_REACH, finish_content_frame, vertical_gradient};
pub use text::{
    DISPLAY_FAMILY, clamp_into, display_font, display_name, ellipsize, host_of, initial,
    normalize_url,
};

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
    /// Dismiss the notification with this id.
    DismissNotification(u64),
    /// Open or shut the notification tray behind the bell.
    ToggleNotifications,
    /// Throw away every notification being kept.
    ClearNotifications,
    /// Fill the saved login for the page in the active tab.
    ///
    /// Only ever raised by a deliberate keypress — see
    /// `AppState::fill_saved_login`.
    FillSavedLogin,
    OpenDownloads,
    OpenHistory,
    /// Add or remove the active page from favourites.
    ToggleFavourite,
    /// Pin a site the reader picked from a list, rather than the page they
    /// happen to be on. Same favourites as the sidebar's star keeps — one
    /// list, reachable from the new tab page's chip row as well.
    PinSite(String, String),
    /// Open one of the four pages a page that will not load lands on. The
    /// string is the whole address, query and all: what is known about the
    /// failure travels in it rather than in a field beside it.
    OpenTrouble(crate::trouble::Trouble, String),
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
    /// Walk all three layouts. The glyph is a two-way switch and should stay
    /// one; this is the shortcut that can reach full-page mode.
    CycleLayout,
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
    /// The surface the page is painted on.
    ///
    /// The same rectangle, except at the last step of the seam, where the
    /// chrome is a tint laid on the page and the page reaches back underneath
    /// it. What every surface frosts against is a copy of *this*, so the two
    /// have to be told apart: sampling the card's rectangle for a sidebar that
    /// is sitting outside it finds nothing at all.
    pub page_rect: egui::Rect,
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
    /// The page that is on its way out, if one is. See [`crate::transition`].
    pub crossing: Option<&'a crate::transition::Crossing>,
    /// What the page says the pointer is over — a link's target, mostly.
    pub status: Option<&'a str>,
    /// Notifications raised by pages.
    pub notifications: crate::notifications::View<'a>,
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

    // First run takes the whole frame, chrome and page both. There is nothing
    // behind it worth clicking on by accident, and a half-configured browser
    // showing through a setup screen would be answering questions it has not
    // asked yet.
    if setup::draw(root, chrome, &mut actions) {
        let window = root.ctx().content_rect();
        return UiOutput {
            actions,
            content_rect: window,
            page_rect: window,
            settings_open: true,
            controls_open: false,
            ambient: false,
        };
    }

    // Autohide reveal: decided before the sidebar is drawn, but painted last
    // as a floating overlay. The collapsed handle stays allocated underneath
    // either way, so revealing never reflows the content card — and so never
    // resizes the webview, which would relayout the page.
    let peek = sidebar_peek(root, chrome);
    // Where the window begins, before the sidebar has taken its share of it.
    // The page reaches back to here once the chrome is floating over it.
    let window = root.available_rect_before_wrap();
    // One clock for every morph.
    //
    // The material has owned this number all along and the chrome spent it on
    // hovers only; the layout takes the same one, and the change from one
    // arrangement to another stops being a jump cut. Four properties move —
    // width, height, radius and opacity — and position is not among them: a
    // tab that slid across the window as the panel narrowed would read as
    // flying off the edge, and there is nowhere for it to fly to.
    //
    // Two values rather than one, because there are three layouts. `present`
    // is how much chrome there is at all — one at either docked layout, zero
    // in full-page mode. `toward_bar` says which of the two docked ones it is.
    // Between them the six transitions are all continuous and none of them
    // needs a case of its own.
    let layout = chrome.settings.layout;
    let motion = chrome.palette.material.animation;
    let mut floating_sidebar: Option<Rect> = None;
    let present = glass::ease_out(root.ctx().animate_bool_with_time(
        Id::new("zervo_chrome_present"),
        layout != crate::settings::Layout::FullPage,
        motion,
    ));
    let toward_bar = glass::ease_out(root.ctx().animate_bool_with_time(
        Id::new("zervo_layout_morph"),
        layout == crate::settings::Layout::Bar,
        motion,
    ));
    // Whether the sidebar will be a tint laid on the page rather than a panel
    // beside it. Decided before it is drawn, because a floating sidebar
    // reserves its space here and paints its contents later, above the page.
    let active_kind = chrome.browser.active_tab().map(|tab| tab.kind);
    let will_float = layout.is_sidebar()
        && chrome.palette.appearance.seam.chrome_floats()
        && peek.is_none()
        && matches!(active_kind, Some(TabKind::NewTab));

    // One at a time, and never both. The address pill is a single control with
    // a single id, so two of it in one frame is two of it fighting over the
    // same focus and the same anchor. So the sidebar narrows to nothing over
    // the first half of the clock and the bar grows out of the window's top
    // edge over the second — one thing becoming another rather than two things
    // overlapping.
    // Whether `sidebar_body` is being drawn by the docked panel this frame.
    // The reveal draws the same body into another layer, and one widget id in
    // two layers is a hard assert in egui — so exactly one of them may run.
    let sidebar_docked = present > 0.0 && toward_bar < 0.5;
    if present > 0.0 {
        if toward_bar < 0.5 {
            floating_sidebar = draw_sidebar(
                root,
                chrome,
                &mut actions,
                present * (1.0 - toward_bar * 2.0),
                will_float,
            );
        } else {
            // Navigation moves into a bar across the top, so the sidebar has
            // nothing to hold but tabs.
            let reveal = present * (toward_bar * 2.0 - 1.0);
            if reveal >= 1.0 {
                draw_navbar(root, chrome, &mut actions, reveal);
            } else {
                root.scope(|ui| {
                    ui.multiply_opacity(reveal);
                    // Halfway through arriving is not something to click on.
                    ui.disable();
                    draw_navbar(ui, chrome, &mut actions, reveal);
                });
            }
        }
    }

    let outer = root.available_rect_before_wrap();
    // With the bar up, the card starts where the bar ends: the gap under the
    // pill is then the bar's own, and matches the gap above it.
    let top_margin = if layout.is_sidebar() {
        theme::content_margin(&chrome.palette)
    } else {
        0.0
    };
    let content_rect = paint_content_backdrop(root, outer, &chrome.palette, top_margin);

    // The last step of the seam: the chrome stops being a panel beside the
    // page and becomes a tint laid on it. The sidebar keeps its own layout
    // space — it is still a `Panel`, so it still resizes and still remembers
    // its width — but the page is painted back underneath it, and the sidebar
    // frosts against that rather than against grey.
    //
    // The new tab page only. A web page arrives as a blit into the rectangle
    // the engine was given and there is no way to ask it to leave a margin
    // down its left side; the other internal pages are cards in their own
    // right and fill the rectangle they are handed. In every one of those
    // cases the sidebar sits beside the page exactly as it does one step
    // earlier — and, crucially, in the same place, so nothing moves when the
    // kind of page changes.
    let page_rect = if will_float {
        Rect::from_min_max(pos2(window.min.x, content_rect.min.y), content_rect.max)
    } else {
        content_rect
    };
    let mut ambient = false;
    match active_kind {
        Some(TabKind::Settings) => {
            draw_settings_page(root, chrome, content_rect, &mut actions);
        },
        Some(TabKind::NewTab) => {
            ambient = crate::newtab::draw(root, chrome, page_rect, content_rect, &mut actions);
        },
        Some(TabKind::Downloads) => {
            draw_downloads_page(root, chrome, content_rect, &mut actions);
        },
        Some(TabKind::History) => {
            draw_history_page(root, chrome, content_rect, &mut actions);
        },
        Some(TabKind::Trouble(trouble)) => {
            crate::trouble::draw(root, chrome, trouble, content_rect, &mut actions);
        },
        _ => {},
    }
    // ── The page that is leaving, over the one that has arrived.
    //
    // In an `Area` at `Order::Background`, which is above the background layer
    // the page is painted into and below the `Order::Middle` the floating
    // sidebar uses — so under the seam that floats the chrome, the old page
    // travels *under* the glass, which is the whole of what "pass under"
    // means. Clipped to where the page was, so a frame sliding out disappears
    // at the page's own edge rather than over the sidebar beside it.
    if let Some(crossing) = chrome.crossing {
        let ctx = root.ctx().clone();
        let clip = crossing.clip();
        egui::Area::new(Id::new("zervo_crossing"))
            .order(egui::Order::Background)
            .fixed_pos(clip.min)
            .constrain(false)
            // An `Area` fades itself in over the style's animation time, which
            // here is the material's settle — 0.14s of a 0.26s crossing. So the
            // departing page started *invisible* and was still ramping up as it
            // was meant to be fading out: two opposed curves, and the visible
            // result was a faint smear rather than a page leaving.
            .fade_in(false)
            .show(&ctx, |ui| {
                ui.set_clip_rect(clip);
                crossing.draw(ui.painter(), &chrome.palette);
            });
        ctx.request_repaint_after(crossing.again());
    }

    let settings_open = matches!(
        active_kind,
        Some(
            TabKind::Settings
                | TabKind::NewTab
                | TabKind::Downloads
                | TabKind::History
                | TabKind::Trouble(_)
        )
    );

    // The sidebar's own material, laid on the page it is floating over.
    //
    // Painted after the page and into the same layer, so it covers it — while
    // the sidebar's rows and text went into the panel's layer earlier and stay
    // on top of it. `reaching` is what makes the frost work: the blurred copy
    // is of the page, the sidebar is beside the page rather than on it, and
    // the sampler clamps, so what the sidebar gets is the page's own left edge
    // carried out to it.
    //
    // No hairline down the join. The hairline *is* the seam, and this is the
    // step that gets rid of it.
    //
    // Drawn into an `Area` above the background layer rather than into the
    // background layer itself. That was the bug that made the whole sidebar a
    // ghost: `Panel::show_inside(root, …)` puts the sidebar's rows and text
    // into *root's* layer, which is the background layer, so a tint added to
    // that layer after the page was also added after the sidebar's own
    // contents — and painted over them. Shapes within a layer are drawn in the
    // order they were added, and there is no order in one layer that puts the
    // tint above the page and below the sidebar. It needs a layer of its own,
    // which is what "the sidebar floats over the content" meant all along.
    if let Some(over) = floating_sidebar {
        let ctx = root.ctx().clone();
        let clip = ctx.content_rect();
        egui::Area::new(Id::new("zervo_sidebar_float"))
            .order(egui::Order::Middle)
            .fixed_pos(over.min)
            .constrain(false)
            .show(&ctx, |ui| {
                ui.set_clip_rect(clip);
                ui.painter().extend(glass::shapes(
                    over,
                    &chrome.palette.reaching(PANEL_REACH),
                    Glass::of(Surface::Menu)
                        .radius_exact(0)
                        .no_shadow()
                        .tint(chrome.palette.pane_tint()),
                ));
                paint_edge_shadow(
                    ui.painter(),
                    Rect::from_min_max(
                        pos2(over.max.x, over.min.y),
                        pos2(over.max.x + 12.0, over.max.y),
                    ),
                    chrome.palette.shadow,
                );
                let inner = Rect::from_min_max(
                    over.min
                        + vec2(
                            f32::from(SIDEBAR_MARGIN.left),
                            f32::from(SIDEBAR_MARGIN.top),
                        ),
                    over.max
                        - vec2(
                            f32::from(SIDEBAR_MARGIN.right),
                            f32::from(SIDEBAR_MARGIN.bottom),
                        ),
                );
                let mut body = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(inner)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                sidebar_body(&mut body, chrome, &mut actions, SidebarHost::DOCKED);
                ui.advance_cursor_after_rect(over);
            });
    }

    // What the hidden sidebar leaves behind, drawn over the page for the same
    // reason the chrome's own tint is: it belongs to the window rather than to
    // whatever is open in it.
    // `sidebar_peek` returns `Some` whenever peeking is *available*, not only
    // while it is open — so testing `is_none()` here meant the spine was drawn
    // exactly when the sidebar was docked, which is the one case `draw_spine`
    // itself refuses. The two conditions were complements and the spine was
    // never once drawn. What is wanted is: whenever the reveal is not covering
    // the edge it would sit on.
    if !peek.is_some_and(|(_, open)| open) {
        draw_spine(root, chrome);
    }

    // The reveal, before anything that has to know whether it is on screen.
    //
    // Suppressed entirely while the docked sidebar is drawing its own body,
    // which includes the frames where it is morphing away. The rule
    // is the one the morph above already states — "one at a time, and never
    // both. The address pill is a single control with a single id, so two of it
    // in one frame is two of it fighting over the same focus and the same
    // anchor" — and it holds just as hard here: the reveal draws the same
    // sidebar body, into a *different layer*, and egui asserts outright when
    // one widget id is registered in two layers in one frame. Which is to say
    // it does not merely look wrong, it takes the process down.
    //
    // The window it was reachable in was small — the pointer had to already be
    // at the window's edge when the layout changed — which is exactly why it
    // presented as "crashes sometimes when opening the sidebar".
    let revealed = (!sidebar_docked)
        .then(|| draw_sidebar_peek(root, chrome, &mut actions, peek))
        .flatten();

    // Full-page mode's address bar, for when nothing is revealed.
    //
    // Only then: the reveal in full-page mode is the whole chrome and carries a
    // pill of its own, and two live `zervo_address_pill`s in one frame is one
    // egui id used twice.
    //
    // Gated on whether the reveal is *drawing*, not on whether it is open. It
    // keeps drawing while it slides away, so gating on `open` left both pills
    // on screen for the length of the slide — in two different layers, which
    // is the assert above rather than merely the wrong focus.
    if layout == crate::settings::Layout::FullPage && revealed.is_none() {
        draw_floating_pill(root, chrome, &mut actions);
    }

    // Whether it is showing no longer needs reporting: egui is asked directly
    // which layer the pointer is over.
    let _ = revealed;

    // Page-initiated UI last, over everything else it might overlap.
    let origin = chrome
        .browser
        .active_tab()
        .and_then(|tab| url::Url::parse(&tab.url).ok())
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "This page".to_owned());
    let scale = root.pixels_per_point();
    chrome.controls.draw(
        root,
        &chrome.palette,
        content_rect,
        scale,
        &origin,
        chrome.vault,
    );
    if let Some(status) = chrome.status {
        draw_status_text(root, &chrome.palette, content_rect, status);
    }
    draw_toasts(
        root,
        &chrome.palette,
        content_rect,
        &chrome.notifications,
        &mut actions,
    );
    let controls_open = !chrome.controls.is_empty();

    UiOutput {
        actions,
        content_rect,
        page_rect,
        settings_open,
        controls_open,
        ambient,
    }
}

/// Snap a rect in points to the physical pixel grid, so shapes drawn in
/// points and the GL blit (computed in pixels) agree exactly.
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

/// Where the sidebar's nav row has to start for its centre to land on the
/// macOS traffic lights'.
///
/// The lights are 12pt tall at 14pt in, so their centre is at 20.
/// `icons::icon_button` is `size + 8` tall, which for a 17pt glyph is 25, so
/// the row has to start at 7.5 for the two centres to meet. The frame's own
/// top margin is 8 — half a point out, which is a near-miss, and a near-miss
/// reads worse than an obvious offset does.
///
/// The half point is taken off the top and given straight back to the gap
/// below, so the address pill does not move when this is turned on.
const NAV_ROW_TOP: f32 = 7.5;
/// The gap between the nav row and the address pill under it.
const NAV_ROW_GAP: f32 = 10.0;

/// While the sidebar is narrowing out of existence its width belongs to the
/// animation rather than to the reader, so it is a different panel with a
/// different id.
///
/// The alternative is letting the morph write its own intermediate widths into
/// the real panel's stored state, which ends with a reader who set the sidebar
/// to 320 getting 200 back the next time they open it.
const SIDEBAR_MORPH_ID: &str = "zervo_sidebar_morph";

/// The sidebar panel.
///
/// When `floating` it reserves its space and paints nothing: its contents
/// belong above the page, and the caller draws them there once the page is
/// down. Returns the rect they go in.
fn draw_sidebar(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    reveal: f32,
    floating: bool,
) -> Option<Rect> {
    let stored = chrome
        .settings
        .sidebar_width
        .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
    let settled = reveal >= 1.0;
    let mut reserved = None;
    let panel = if settled {
        Panel::left(SIDEBAR_ID)
            .resizable(true)
            .default_size(stored)
            .size_range(SIDEBAR_MIN_WIDTH..=SIDEBAR_MAX_WIDTH)
    } else {
        Panel::left(SIDEBAR_MORPH_ID)
            .resizable(false)
            .exact_size(stored * reveal.max(0.0))
    };
    panel
        // Transparent: the window-wide chrome base paints behind it, so the
        // top gradient continues across the sidebar boundary unbroken.
        .frame(Frame::NONE.inner_margin(SIDEBAR_MARGIN))
        .show_separator_line(false)
        .show_inside(root, |ui| {
            // Fill the width the panel was given.
            //
            // egui stores a panel's remembered width from the rect its
            // *contents* used, floored at the size range's minimum — not from
            // the width the panel was laid out at. Nothing in the sidebar
            // stretches, so every second frame it remembered 200 rather than
            // the 248 it had just been drawn at, wrote that back to the
            // settings, and the next launch opened at 200 again. The default
            // width had not been reachable since the panel was written, and
            // dragging the sidebar wider snapped it back on release.
            ui.set_min_width(ui.available_width());
            if floating {
                // Space only. What goes in it is painted above the page.
                reserved = Some(Rect::from_min_max(
                    ui.max_rect().min
                        - vec2(
                            f32::from(SIDEBAR_MARGIN.left),
                            f32::from(SIDEBAR_MARGIN.top),
                        ),
                    ui.max_rect().max
                        + vec2(
                            f32::from(SIDEBAR_MARGIN.right),
                            f32::from(SIDEBAR_MARGIN.bottom),
                        ),
                ));
                return;
            }
            if settled {
                sidebar_body(ui, chrome, actions, SidebarHost::DOCKED);
                return;
            }
            // Laid out at the width it will have when it arrives, and clipped
            // to the width it has now — the panel's own clip rect does the
            // cutting. Reflowing the body into a narrowing column instead
            // would move every row in it, and moving is the one thing this is
            // not allowed to do.
            let full = Rect::from_min_size(
                ui.max_rect().min,
                vec2(
                    stored - f32::from(SIDEBAR_MARGIN.left + SIDEBAR_MARGIN.right),
                    ui.max_rect().height(),
                ),
            );
            let layout = *ui.layout();
            let mut body = ui.new_child(egui::UiBuilder::new().max_rect(full).layout(layout));
            body.multiply_opacity(reveal);
            // Halfway through leaving is not something to click on.
            body.disable();
            sidebar_body(&mut body, chrome, actions, SidebarHost::DOCKED);
            // Catching the light on the way out. The phase runs with the
            // panel's own width, so the light crosses it exactly once over the
            // morph rather than on a clock of its own.
            paint_specular(ui.painter(), ui.max_rect(), &chrome.palette, 1.0 - reveal);
        });

    // Remember a manual resize, so the width outlives the session — and so an
    // autohide reveal comes back the width the user left it. Written once the
    // drag ends rather than every frame of it, which would be a disk write per
    // frame — and never at all while the morph owns the width.
    if !settled {
        return reserved;
    }
    let width = sidebar_width(root.ctx(), chrome.settings);
    if !root.ctx().egui_is_using_pointer() && (width - stored).abs() > 0.5 {
        chrome.settings.sidebar_width = width;
        actions.push(UiAction::PersistSettings);
    }
    reserved
}

/// The sidebar's contents, drawn into whatever `ui` it is handed: the docked
/// panel, or the floating overlay an autohide reveal puts over the content.
/// `compact` drops everything the navigation bar is already showing, leaving
/// the sidebar to do the one thing the bar cannot: tabs and workspaces.
/// Which of the sidebar's two jobs this copy of it is doing.
///
/// `compact` drops what the navigation bar is already showing — the nav row
/// and the address pill — leaving the sidebar to do the one thing the bar
/// cannot: tabs and workspaces. `utilities` is a separate question, because
/// full-page mode has no bar either: there, the revealed panel is the only
/// chrome there is and it has to carry everything.
#[derive(Clone, Copy)]
struct SidebarHost {
    compact: bool,
    utilities: bool,
}

impl SidebarHost {
    /// The docked panel: it has the window to itself.
    const DOCKED: Self = Self {
        compact: false,
        utilities: true,
    };

    /// A reveal over the page. What it carries depends on what the layout it
    /// was revealed from is already showing.
    ///
    /// Full-page mode is showing nothing: there is no bar and no docked
    /// sidebar, so the reveal is not a *supplement* to some other chrome, it is
    /// the whole of it. Compact there meant the one panel a full-page reader
    /// can open had no nav row, no address pill and no shelf — three quarters
    /// of the browser reachable only by leaving the mode.
    fn peeked(layout: crate::settings::Layout) -> Self {
        let alone = layout == crate::settings::Layout::FullPage;
        Self {
            compact: !alone,
            utilities: alone,
        }
    }
}

fn sidebar_body(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    host: SidebarHost,
) {
    let SidebarHost { compact, utilities } = host;
    let palette = chrome.palette;
    let drag = ui.interact(ui.max_rect(), ui.id().with("sidebar_drag"), Sense::drag());
    if drag.drag_started() {
        actions.push(UiAction::DragWindow);
    }

    // Bottom utility bar first, so the scroll area gets the rest; then the
    // shelf above it, so the widgets sit between the tabs and the tools.
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
                // The bar's own right-hand group, in the bar's own order —
                // Extensions and History used to be reachable only when the
                // sidebar was collapsed. They sit in the navigation bar when
                // it is up, so the sidebar does not show them twice; in
                // full-page mode nothing else is showing them at all.
                if utilities {
                    let active = chrome.downloads.active_count();
                    let mut tray = None;
                    for item in [
                        NavItem::Extensions,
                        NavItem::History,
                        NavItem::Downloads,
                        NavItem::Settings,
                    ] {
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(nav_item_width(), NAVBAR_ICON + 8.0),
                            Sense::hover(),
                        );
                        draw_nav_item(ui, chrome, actions, item, rect);
                        if item == NavItem::Downloads {
                            tray = Some(rect);
                            if active > 0 {
                                // A small accent dot marks downloads in
                                // flight. The dot does not move; what changes
                                // beside it is a byte count.
                                ui.painter().circle_filled(
                                    pos2(rect.max.x - 4.0, rect.min.y + 4.0),
                                    3.5,
                                    palette.accent,
                                );
                                ui.ctx()
                                    .request_repaint_after(std::time::Duration::from_millis(250));
                            }
                        }
                    }
                    if let Some(tray) = tray {
                        draw_downloads_card(ui, chrome, actions, tray);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::icon_button(ui, Icon::Plus, 15.0, &palette, true)
                        .on_hover_text("New workspace")
                        .clicked()
                    {
                        actions.push(UiAction::NewWorkspace);
                    }
                    // The way out of full-page mode. It is the one tile of
                    // turn 4b's quick row that has somewhere real to go: the
                    // others open a tab overview and a page-tools panel that
                    // do not exist yet, and a button that does nothing is
                    // worse than one that is not there.
                    if chrome.settings.layout == crate::settings::Layout::FullPage
                        && icons::icon_button(ui, Icon::Browser, 16.0, &palette, true)
                            .on_hover_text("Leave full-page mode (⌘S)")
                            .clicked()
                    {
                        actions.push(UiAction::ToggleSidebar);
                    }
                });
            });
        });

    // ── The widget shelf, above the utility bar and below the tabs.
    //
    // Not in a compact reveal over the page: what a peek is for is finding a
    // tab, and a shelf of widgets in a panel that is about to slide away again
    // is furniture in a doorway.
    //
    // `utilities` overrides the reader's `ShelfHome`, and only ever upward. A
    // panel carrying the utilities is the only chrome there is, and "Bar only"
    // in a layout with no bar is not a placement, it is the shelf switched off
    // by a control that does not say so.
    if !compact && (chrome.palette.appearance.shelf.in_sidebar() || utilities) {
        draw_sidebar_shelf(ui, chrome, actions);
    }

    // ── Top row: nav icons, right of the macOS traffic lights. All of these
    // are in the navigation bar when the sidebar is collapsed.
    if !compact {
        // Half a point up, and the same half point back into the gap under the
        // row — see `NAV_ROW_TOP`. Only where there are window controls to
        // line up with.
        let align = cfg!(target_os = "macos") && chrome.settings.appearance.align_nav;
        let nudge = if align {
            NAV_ROW_TOP - f32::from(SIDEBAR_MARGIN.top)
        } else {
            0.0
        };
        ui.add_space(nudge);
        let star;
        // The nav row gets a rect of its own rather than a `horizontal`.
        //
        // egui grows a `Ui`'s *max* rect to include anything a child overflowed
        // into, and the overflow is silent. At the sidebar's minimum width the
        // macOS inset plus five nav icons is 188 points inside a 178-point
        // column, so the row widened the whole body by 61 — and the address
        // pill under it, which takes `available_width`, was drawn hanging fifty
        // points out over the page. Nothing in the sidebar had asked to be
        // wider; one row had simply not fitted.
        //
        // Boxed and clipped, the miss stays local: the last icon is cut off,
        // which is visible and true, instead of every widget below it moving.
        let row = Rect::from_min_size(
            pos2(ui.max_rect().min.x, ui.cursor().min.y),
            vec2(ui.max_rect().width(), NAVBAR_ICON + 8.0),
        );
        let mut nav = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        nav.set_clip_rect(row.intersect(ui.clip_rect()));
        // The study's nav icons sit shoulder to shoulder — five 26pt boxes in a
        // row, no gap. egui's own 8pt item spacing put 32 points between them,
        // which is what pushed the fifth off the end of a 226-point column.
        nav.spacing_mut().item_spacing.x = 0.0;
        {
            let ui = &mut nav;
            #[cfg(target_os = "macos")]
            ui.add_space(58.0);

            let (can_go_back, can_go_forward, is_settings) = chrome
                .browser
                .active_tab()
                .map(|tab| {
                    (
                        tab.can_go_back,
                        tab.can_go_forward,
                        // Reload is offered on a web page and on the four
                        // pages that stand in for one. `zervo://offline` is
                        // the case that makes it necessary: 7b gives it no
                        // button of its own — "it retries on its own when the
                        // interface comes back. No button to press" — and
                        // Zervo cannot yet watch the interface, so a greyed
                        // toolbar Reload left the page with no way to try
                        // again at all.
                        !reloadable(tab.kind),
                    )
                })
                .unwrap_or_default();

            // Drawn rather than lettered: the divider rotates in place, so
            // the button shows the layout it is moving to and shows it
            // moving. A glyph swap here is one frame with no continuity, and
            // at seventeen points the reader mostly does not see it happen.
            if icons::morph_button(
                ui,
                17.0,
                &palette,
                true,
                &crate::morphicons::SIDEBAR,
                &crate::morphicons::BAR,
                !chrome.settings.layout.is_sidebar(),
            )
            .on_hover_text(if chrome.settings.layout.is_sidebar() {
                "Hide sidebar (⌘S)"
            } else {
                "Show sidebar (⌘S)"
            })
            .clicked()
            {
                actions.push(UiAction::ToggleSidebar);
            }

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
            // Favourites, where the bar puts it: last in the left-hand group.
            // It used to exist only in the collapsed bar, which meant that
            // *hiding* the sidebar added a feature. Collapsing should change
            // the shape and never the vocabulary.
            let (rect, _) =
                ui.allocate_exact_size(vec2(nav_item_width(), NAVBAR_ICON + 8.0), Sense::hover());
            draw_nav_item(ui, chrome, actions, NavItem::Favourite, rect);
            star = rect;
        }
        ui.advance_cursor_after_rect(row);
        ui.add_space(NAV_ROW_GAP - nudge);

        // ── Search / address pill.
        draw_address_pill(ui, chrome, actions, 36.0);
        ui.add_space(12.0);
        draw_favourites_card(ui, chrome, actions, star);
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
            // One highlight that travels and stretches between rows, rather
            // than two rows half-lit for the settle time. Reserved here and
            // filled in after the rows are laid out, because until they are
            // nobody knows where it is going.
            let liquid = chrome.palette.appearance.liquid;
            let highlight = liquid.then(|| ui.painter().add(Shape::Noop));
            let mut selected_rect: Option<Rect> = None;

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
                    chrome.settings.space_colour,
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
                            glyph: internal_glyph(tab.kind),
                            always_show_close: always_close,
                            compact: chrome.settings.compact_sidebar,
                            liquid,
                            dragging,
                        },
                        &palette,
                        chrome.favicons.get(&tab.id),
                        actions,
                    );
                    if selected {
                        selected_rect = Some(out.rect);
                    }
                    if out.drag_started {
                        ctx.data_mut(|data| data.insert_temp(drag_id, tab.id));
                    }
                    released |= out.drag_stopped;
                    match out.drop_at {
                        Some(DropAt::Before) => target = Some((workspace_index, tab_index, None)),
                        Some(DropAt::After) => {
                            target = Some((workspace_index, tab_index + 1, None));
                        },
                        Some(DropAt::Onto) => {
                            target = Some((workspace_index, tab_index, Some(tab.id)));
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
                        palette.corner(Tier::Row),
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

            // ── The selection, as one thing that moves.
            //
            // Two rows cross-fading is two answers to "which tab am I on" for
            // the length of the settle time. One highlight that travels from
            // the old row to the new one is a single answer that happens to be
            // in transit, which is both truer and easier to follow with the
            // eye.
            //
            // Filled into the slot reserved before the rows were drawn, so it
            // lands underneath them and their text stays on top of it.
            if let (Some(slot), Some(to)) = (highlight, selected_rect) {
                let id = Id::new("zervo_liquid_selection");
                let now = ctx.input(|input| input.time);
                let motion = f64::from(palette.material.animation);
                let mut state = ctx
                    .data(|data| data.get_temp::<Liquid>(id))
                    .unwrap_or(Liquid {
                        from: to,
                        to,
                        started: now - motion,
                    });
                let progress = if motion <= 0.0 {
                    1.0
                } else {
                    ((now - state.started) / motion).clamp(0.0, 1.0) as f32
                };
                let at = lerp_rect(state.from, state.to, glass::ease_out(progress));
                if state.to != to {
                    // A new leg, starting from wherever it had got to — so
                    // changing tabs twice quickly carries on from the middle
                    // rather than snapping back to the row it left.
                    state = Liquid {
                        from: at,
                        to,
                        started: now,
                    };
                    ctx.data_mut(|data| data.insert_temp(id, state));
                }
                if progress < 1.0 {
                    ctx.request_repaint();
                } else if state.from != state.to {
                    // Arrived. Collapse the leg so the next one starts from a
                    // rectangle rather than from an interpolation of one.
                    ctx.data_mut(|data| {
                        data.insert_temp(id, Liquid { from: to, ..state });
                    });
                }
                let mut shapes = glass::shapes(at, &palette, selection_glass(&palette, 1.0));
                ui.painter().set(
                    slot,
                    if shapes.len() == 1 {
                        shapes.remove(0)
                    } else {
                        Shape::Vec(shapes)
                    },
                );
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
        .rect_filled(content_rect, theme::content_corners(&palette), palette.bg);

    let mut ui = root.new_child(
        egui::UiBuilder::new()
            .max_rect(
                content_rect
                    .shrink(28.0)
                    .with_min_y(content_rect.min.y + 28.0 + window_controls_room(chrome.settings)),
            )
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
                        ui.painter().rect_filled(
                            row,
                            palette.corner(Tier::Row),
                            palette.surface_hover,
                        );
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
    let lit = glass::ease_out(ui.ctx().animate_bool_with_time(
        ui.id().with("favourite_lit"),
        saved,
        palette.material.animation,
    ));
    if lit > 0.0 {
        // The material has declared a `glow` and a `glow_reach` all along, and
        // the starred state was the one place that said "saved" with a hue and
        // nothing else. A hue is something to read; a light coming on is
        // something you feel change, which is what a state you just toggled
        // should be.
        ui.painter().add(glass::shadow(
            rect,
            f32::from(palette.radius(Tier::Control)),
            palette.accent.gamma_multiply(palette.material.glow * lit),
            palette.material.glow_reach,
            glass::Inner::Under,
        ));
        // No filled star in the vendored pack, so the colour still has to do
        // the other half of the work.
        icons::draw_icon(
            ui.painter(),
            Rect::from_center_size(rect.center(), vec2(NAVBAR_ICON, NAVBAR_ICON)),
            Icon::Star,
            palette.accent.gamma_multiply(lit),
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
                &palette.reaching(PANEL_REACH),
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
            add(ui, card, palette.reaching(PANEL_REACH).over(drawn));
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
    let palette = &palette.reaching(PANEL_REACH).over(rect);
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
                    ui.painter().rect_filled(
                        row,
                        palette.corner(Tier::Control),
                        palette.surface_hover,
                    );
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
                            palette.corner(Tier::Card),
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
                        palette.corner(Tier::Control),
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
                        .rect_filled(row, palette.corner(Tier::Row), palette.surface_hover);
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
    let items: Vec<crate::downloads::RowView> = chrome
        .downloads
        .items
        .iter()
        .rev()
        .take(8)
        .map(crate::downloads::Download::row_view)
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

            for (index, entry) in items.iter().enumerate() {
                let crate::downloads::RowView {
                    id: download,
                    filename,
                    url: _,
                    fraction,
                    received,
                    total,
                    state,
                } = entry;
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
                        .rect_filled(row, palette.corner(Tier::Row), palette.surface_hover);
                }

                let running = *state == DownloadState::Running;
                let glyph = Rect::from_center_size(
                    pos2(row.min.x + 16.0, row.center().y),
                    vec2(15.0, 15.0),
                );
                match state {
                    // The one state change in this row somebody is waiting
                    // for, so it is drawn rather than lettered: the shaft
                    // retracts into its own base while the chevron
                    // straightens into a tick, and the colour crosses with
                    // it. A glyph swap here happens at the exact moment
                    // nobody is looking at the row any more.
                    DownloadState::Running | DownloadState::Complete => {
                        let done = *state == DownloadState::Complete;
                        let cross = glass::ease_out(ui.ctx().animate_bool_with_time(
                            ui.id().with(("download_done", index)),
                            done,
                            palette.material.animation,
                        ));
                        crate::morphicons::draw(
                            ui.painter(),
                            glyph,
                            &crate::morphicons::DOWNLOADING,
                            &crate::morphicons::DONE,
                            cross,
                            theme::mix(palette.text_muted, palette.accent, cross),
                        );
                    },
                    _ => icons::draw_icon(
                        ui.painter(),
                        glyph,
                        Icon::XCircle,
                        palette.text_muted.gamma_multiply(0.8),
                    ),
                }
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
                        .rect_filled(track, palette.corner(Tier::Hairline), palette.border);
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            track.min,
                            vec2(track.width() * fraction, track.height()),
                        ),
                        palette.corner(Tier::Hairline),
                        palette.accent,
                    );
                }

                // ── Controls, on hover so a quiet list stays quiet.
                if over {
                    let mut x = row.max.x - 16.0;
                    let mut control =
                        |ui: &mut Ui, icon: Icon, tip: &str, enabled: bool, key: &str| {
                            let hit =
                                Rect::from_center_size(pos2(x, row.center().y), vec2(24.0, 24.0));
                            x -= 26.0;
                            let response = ui.interact(hit, id.with((key, index)), Sense::click());
                            if enabled && response.hovered() {
                                ui.painter().rect_filled(
                                    hit,
                                    palette.corner(Tier::Control),
                                    palette.surface,
                                );
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
                let chosen = items.iter().find(|entry| entry.id == download);
                let url = chosen.map(|entry| entry.url.clone()).unwrap_or_default();
                let filename = chosen
                    .map(|entry| entry.filename.clone())
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
pub fn shelf_reveal(settings: &Settings) -> f32 {
    if !matches!(settings.layout, crate::settings::Layout::Bar) {
        return 0.0;
    }
    settings.navbar_height.clamp(NAVBAR_ROW, NAVBAR_MAX_HEIGHT) - NAVBAR_ROW
}

fn draw_navbar(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    reveal: f32,
) {
    let palette = chrome.palette;
    let ctx = root.ctx().clone();
    let window = root.ctx().content_rect();
    let height = chrome
        .settings
        .navbar_height
        .clamp(NAVBAR_ROW, NAVBAR_MAX_HEIGHT);
    // Only the reserved height moves while the bar is arriving, so the page
    // below it slides up and down and the controls above it stay exactly where
    // they will be when it has finished.
    let strip = Rect::from_min_size(window.min, vec2(window.width(), height * reveal.max(0.0)));
    // The controls keep to a fixed row at the top, so dragging the bar taller
    // opens space underneath them rather than spreading them out.
    let row = Rect::from_min_size(window.min, vec2(window.width(), NAVBAR_ROW));

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
        let margin = theme::content_margin(&palette);
        let shelf = Rect::from_min_max(
            pos2(window.left() + margin, row.max.y),
            pos2(window.right() - margin, strip.max.y - SHELF_BOTTOM),
        );
        // A recess rather than another card: the widgets are the cards, and
        // two levels of card inside each other reads as clutter.
        root.painter().rect_filled(
            shelf,
            theme::content_corners(&palette),
            palette.shadow.gamma_multiply(0.10),
        );
        // The shelf is the surface that changes size most often — every drag
        // of the bar's lower edge resizes it — so it is the one that most
        // wants to look like glass while it does.
        paint_specular(
            root.painter(),
            shelf,
            &palette,
            (shelf.height() / crate::dashboard::height_for_rows(crate::dashboard::MAX_ROWS))
                .clamp(0.0, 0.999),
        );
        for change in crate::dashboard::draw(
            root,
            &palette,
            chrome.media,
            &chrome.settings.navbar_widgets,
            shelf,
            crate::dashboard::COLUMNS,
        ) {
            actions.push(widget_action(change));
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
        palette.corner(Tier::Row),
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
            palette.corner(Tier::Row),
            palette.surface.gamma_multiply(if held { 1.0 } else { 0.7 }),
        );
        root.painter().rect_stroke(
            drawn,
            palette.corner(Tier::Row),
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
            // A two-point bar with rounded ends. Half its own width, not a
            // tier: this is a caret, and a square one reads as a glitch.
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
    let palette = palette.reaching(PANEL_REACH).over(tray);
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
                    ui.painter().rect_filled(
                        slot,
                        palette.corner(Tier::Row),
                        palette.surface_hover,
                    );
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
        .map(|tab| (tab.can_go_back, tab.can_go_forward, reloadable(tab.kind)))
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
    // Click to toggle, drag to size — the same gesture the shelf has always
    // had, plus the one everybody tries first. Dragging the bar taller is not
    // something anybody does by accident, so on its own it was a secret.
    let response = root.interact(
        grip,
        Id::new("zervo_navbar_resize"),
        Sense::click_and_drag(),
    );
    let emphasis = glass::ease_out(root.ctx().animate_bool_with_time(
        Id::new("zervo_navbar_grabber"),
        response.hovered() || response.dragged(),
        0.12,
    ));
    if response.hovered() || response.dragged() {
        root.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
    }
    response
        .clone()
        .on_hover_text("Widgets — click to open, drag to size");
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
    if response.clicked() {
        // Between the two heights worth resting at, and towards whichever one
        // it is not already nearer to.
        height = if height > NAVBAR_ROW + 1.0 {
            NAVBAR_ROW
        } else {
            uncovered
        };
        chrome.settings.navbar_height = height;
        actions.push(UiAction::PersistSettings);
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
/// The spine's own metrics: how wide a tick is, how long, how long the active
/// one is, and how far in from the window's edge they sit.
///
/// All of it inside [`PEEK_TRIGGER`], because the whole argument for a spine
/// is that the hot edge should be visible: a strip of ticks that stuck out
/// past the region it advertises would be pointing at the wrong place.
const SPINE_TICK: f32 = 3.0;
const SPINE_LENGTH: f32 = 13.0;
const SPINE_GAP: f32 = 5.0;
const SPINE_INSET: f32 = 4.0;
/// A favicon is a picture and has to be big enough to recognise, so this one
/// costs more width than the ticks do — which is the trade the setting is
/// offering.
const SPINE_FAVICON: f32 = 14.0;

/// How far the spine reaches in from the window's edge.
///
/// The reveal trigger is never narrower than this. A strip you can see is a
/// strip you will aim at, and one that stuck out past the region it advertises
/// would be pointing at the wrong place.
fn spine_width(appearance: &crate::theme::Appearance) -> f32 {
    match appearance.spine {
        crate::theme::Spine::Nothing => 0.0,
        crate::theme::Spine::TabTicks => SPINE_INSET * 2.0 + SPINE_TICK,
        crate::theme::Spine::Favicons => SPINE_INSET * 2.0 + SPINE_FAVICON,
    }
}

/// One tick per tab down the window's left edge, coloured by its workspace,
/// with the active one twice as long.
///
/// A hidden sidebar leaves nothing today, so the only way to learn the edge is
/// hot is to find out by accident. This is the smallest thing that is a target
/// rather than a rumour — and it says how many tabs are open without opening
/// anything.
fn draw_spine(root: &Ui, chrome: &ChromeContext) {
    let appearance = chrome.palette.appearance;
    if appearance.spine == crate::theme::Spine::Nothing
        || chrome.settings.layout.is_sidebar()
        || !peek_available(chrome.settings)
    {
        return;
    }
    let active = chrome.browser.active_tab().map(|tab| tab.id);
    let first = chrome.settings.space_colour;
    let ticks: Vec<(Color32, TabId, bool)> = chrome
        .browser
        .workspaces
        .iter()
        .enumerate()
        .flat_map(|(index, workspace)| {
            workspace.tabs.iter().map(move |tab| {
                (
                    theme::workspace_color_from(index, first),
                    tab.id,
                    Some(tab.id) == active,
                )
            })
        })
        .collect();
    if ticks.is_empty() {
        return;
    }

    let window = root.ctx().content_rect();
    let painter = root.ctx().layer_painter(egui::LayerId::background());
    let favicons = appearance.spine == crate::theme::Spine::Favicons;
    let step = |on: bool| {
        if favicons {
            SPINE_FAVICON
        } else if on {
            SPINE_LENGTH * 2.0
        } else {
            SPINE_LENGTH
        }
    };
    let total: f32 = ticks.iter().map(|(_, _, on)| step(*on)).sum::<f32>()
        + SPINE_GAP * (ticks.len() - 1) as f32;
    // Centred on the window rather than stacked from the top: it is a legend
    // for the edge, not a list, and a list would want to be read.
    let mut y = window.center().y - total * 0.5;
    let radius = CornerRadius::same(chrome.palette.radius(Tier::Hairline));
    for (colour, tab, on) in ticks {
        let length = step(on);
        let at = pos2(window.left() + SPINE_INSET, y);
        if favicons {
            let rect = Rect::from_min_size(at, vec2(SPINE_FAVICON, SPINE_FAVICON));
            match chrome.favicons.get(&tab) {
                Some(texture) => {
                    painter.image(
                        texture.id(),
                        rect,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        // Faded the same way the ticks are: the tab you are on
                        // is the one meant to stand out.
                        Color32::WHITE.gamma_multiply(if on { 1.0 } else { 0.55 }),
                    );
                },
                // No favicon yet, and a blank gap would lose the count. The
                // workspace's own colour stands in for it.
                None => {
                    painter.rect_filled(
                        rect.shrink(3.0),
                        CornerRadius::same(chrome.palette.radius(Tier::Control)),
                        colour.gamma_multiply(if on { 0.95 } else { 0.4 }),
                    );
                },
            }
        } else {
            painter.rect_filled(
                Rect::from_min_size(at, vec2(SPINE_TICK, length)),
                radius,
                // The one you are on is its workspace's colour; the rest are a
                // suggestion of it. Ten equally lit ticks would be a barcode.
                colour.gamma_multiply(if on { 0.95 } else { 0.4 }),
            );
        }
        y += length + SPINE_GAP;
    }
}

/// Whether brushing the window's left edge reveals the sidebar.
///
/// The reader's setting, except in full-page mode, where it is not offered:
/// there is no other chrome there, so an edge that did nothing would be a
/// browser with no way back to its own tabs.
fn peek_available(settings: &Settings) -> bool {
    settings.sidebar_autohide || matches!(settings.layout, crate::settings::Layout::FullPage)
}

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
    if chrome.settings.layout.is_sidebar() || !peek_available(chrome.settings) {
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

    // Never narrower than what the spine has drawn there: a strip you can see
    // is a strip you will aim at.
    let trigger = PEEK_TRIGGER.max(spine_width(&chrome.palette.appearance));
    let in_trigger = pointer.is_some_and(|pos| {
        (window.left()..=window.left() + trigger).contains(&pos.x)
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
    //
    // Except in full-page mode, where the pill is floating over the page and
    // does not need the panel opened to be typed into. Revealing there would
    // be the sidebar barging in on a shortcut that has nothing to do with it.
    let owns_address = chrome.settings.layout != crate::settings::Layout::FullPage;
    let wanted = crate::shot::peek_forced(ctx.cumulative_pass_nr())
        || in_trigger
        || in_panel
        || busy
        || (owns_address && (chrome.browser.editing_address || chrome.browser.focus_address));

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
            // This used to be opaque, on the grounds that it floats over the
            // page and any translucency would let the page's own text read
            // through the tabs. That was a fair call when there was nothing to
            // frost against — but `backdrop.rs` already hands the palette a
            // blurred copy of exactly the page this is floating over, and
            // `tint_over` already thickens a surface as far as it must to hold
            // its theme against whatever is behind it. Frost against that and
            // the panel belongs to the window it is over rather than covering
            // it up, with the same guarantee about legibility that every other
            // menu in the application has.
            //
            // Solid keeps the old behaviour, because under Solid there is no
            // blurred copy and opaque is the whole point of the setting.
            if chrome.palette.translucency == theme::Translucency::Frosted {
                // The chrome's own tint first, so the panel is the chrome
                // rather than a menu that happens to be sidebar-shaped, and
                // the glass on top of it.
                paint_chrome_fill(
                    ui.painter(),
                    drawn,
                    ctx.content_rect().top(),
                    &chrome.palette,
                    chrome.settings.top_glow,
                    chrome.palette.chrome_tint(),
                );
                ui.painter().extend(glass::shapes(
                    drawn,
                    &chrome.palette.reaching(PANEL_REACH),
                    Glass::of(Surface::Menu).radius_exact(0).no_shadow(),
                ));
            } else {
                paint_chrome_fill(
                    ui.painter(),
                    drawn,
                    ctx.content_rect().top(),
                    &chrome.palette,
                    chrome.settings.top_glow,
                    1.0,
                );
            }
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
            //
            // The palette is handed down `over` the panel, the same as every
            // other floating surface: now that this is glass rather than an
            // opaque slab, its ink has to follow whatever it is floating over
            // rather than the theme. A pale label on a dark panel opened over
            // a white page is the exact case that rule exists for.
            let restore = chrome.palette;
            chrome.palette = chrome.palette.reaching(PANEL_REACH).over(drawn);
            sidebar_body(
                &mut body,
                chrome,
                actions,
                SidebarHost::peeked(chrome.settings.layout),
            );
            chrome.palette = restore;
            // Claim the whole overlay, so the pointer over it resolves to this
            // layer rather than the chrome underneath.
            ui.advance_cursor_after_rect(drawn);
        });
    Some(drawn)
}

/// What the shelf reports, whichever host it was drawn in.
///
/// One mapping rather than one per host: the sidebar's shelf and the bar's are
/// the same shelf, and `Change::{Resize, Swap, Place, Media}` mean the same
/// thing in both. A second copy of this is a second place for them to stop
/// meaning it.
fn widget_action(change: crate::dashboard::Change) -> UiAction {
    match change {
        crate::dashboard::Change::Remove(index) => UiAction::RemoveWidget(index),
        crate::dashboard::Change::Swap { a, b } => UiAction::SwapWidgets(a, b),
        crate::dashboard::Change::Place { index, col, row } => {
            UiAction::PlaceWidget { index, col, row }
        },
        crate::dashboard::Change::Resize(index, size) => UiAction::ResizeWidget(index, size),
        crate::dashboard::Change::Media(action) => UiAction::MediaAction(action),
    }
}

/// The floating address pill in full-page mode: how tall, and how far off the
/// bottom edge it sits.
///
/// Public because a page drawn under it has to know: the pill floats, so
/// nothing reserves the strip for it, and the new tab page's own bottom row
/// was landing underneath it.
pub const FLOATING_PILL_HEIGHT: f32 = 34.0;
pub const FLOATING_PILL_FLOAT: f32 = 18.0;

/// The room a page has to leave at the top for the window's own controls.
///
/// The sidebar and the navigation bar each hold that strip themselves — the
/// sidebar with its nav row, the bar with `TRAFFIC_LIGHTS` — so this is only
/// ever non-zero in full-page mode, where there is no chrome at all and an
/// internal page's own heading was drawn straight under the close, minimise
/// and zoom buttons.
///
/// A *web* page is deliberately not inset. It fills the window and the
/// controls float over it, which is the whole point of a fullsize content
/// view; this is for the pages Zervo draws itself, which have a heading in
/// exactly that corner.
pub fn window_controls_room(settings: &Settings) -> f32 {
    #[cfg(target_os = "macos")]
    {
        if settings.layout == crate::settings::Layout::FullPage {
            // The lights are 12pt tall at 14pt in, so their bottom edge is at
            // 26. The sidebar reserves 28 for the same reason.
            28.0
        } else {
            0.0
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = settings;
        0.0
    }
}

/// The room a page should leave at the bottom for chrome that floats over it.
pub fn floating_chrome_room(settings: &Settings) -> f32 {
    if settings.layout == crate::settings::Layout::FullPage {
        FLOATING_PILL_HEIGHT + FLOATING_PILL_FLOAT * 2.0
    } else {
        0.0
    }
}

/// How much room the sidebar's shelf grip takes when the shelf is shut.
const SIDEBAR_GRIP: f32 = 10.0;

/// The widget shelf, at the foot of the sidebar.
///
/// The same widgets, the same 52pt cell, the same grip and the same `Change`
/// events — one column instead of twelve, because a 226pt sidebar cannot hold
/// a twelve-column grid and does not need to. Shipping it in the bar alone is
/// why full-page mode could not reach the shelf at all.
fn draw_sidebar_shelf(ui: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    let max = crate::dashboard::height_for_rows(crate::dashboard::MAX_ROWS);
    let open = chrome.settings.sidebar_shelf_height.clamp(0.0, max);
    let uncovered = crate::dashboard::open_height(&chrome.settings.navbar_widgets).min(max);

    Panel::bottom("zervo_sidebar_shelf")
        .resizable(false)
        .exact_size(SIDEBAR_GRIP + open)
        .frame(Frame::NONE)
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            let area = ui.max_rect();
            // The grip is at the top edge here rather than the bottom: the
            // shelf grows upward out of the foot of the sidebar, so that is
            // the edge you pull.
            let grip = Rect::from_min_max(area.min, pos2(area.max.x, area.min.y + SIDEBAR_GRIP));
            let response = ui.interact(
                grip,
                Id::new("zervo_sidebar_shelf_grip"),
                Sense::click_and_drag(),
            );
            let emphasis = glass::ease_out(ui.ctx().animate_bool_with_time(
                Id::new("zervo_sidebar_shelf_grabber"),
                response.hovered() || response.dragged(),
                0.12,
            ));
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
            }
            crate::dashboard::draw_grabber(ui.painter(), &chrome.palette, grip, emphasis);
            response
                .clone()
                .on_hover_text("Widgets — click to open, drag to size");

            let mut height = open;
            if response.dragged() {
                // Upward is taller, which is the opposite of the bar's grip
                // and the same gesture: both drag away from the chrome they
                // belong to.
                height = (height - response.drag_delta().y).clamp(0.0, max);
                chrome.settings.sidebar_shelf_height = height;
            }
            if response.drag_stopped() {
                for rest in [0.0, uncovered] {
                    if (height - rest).abs() < 18.0 {
                        height = rest;
                        chrome.settings.sidebar_shelf_height = height;
                    }
                }
                actions.push(UiAction::PersistSettings);
            }
            if response.clicked() {
                height = if height > 1.0 { 0.0 } else { uncovered };
                chrome.settings.sidebar_shelf_height = height;
                actions.push(UiAction::PersistSettings);
            }
            if height <= 0.0 {
                return;
            }

            let shelf = Rect::from_min_max(pos2(area.min.x, grip.max.y), area.max);
            let palette = chrome.palette;
            for change in crate::dashboard::draw(
                ui,
                &palette,
                chrome.media,
                &chrome.settings.navbar_widgets,
                shelf,
                crate::dashboard::SIDEBAR_COLUMNS,
            ) {
                actions.push(widget_action(change));
            }
        });
}

/// Full-page mode's one piece of permanent chrome: where you are.
///
/// Anchored to the bottom of the window rather than the top. The top is where
/// the chrome that has just gone used to be, and a pill sitting up there would
/// read as a bar that failed to leave; at the bottom it reads as the page's
/// own, which is what it is.
fn draw_floating_pill(root: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    const HEIGHT: f32 = FLOATING_PILL_HEIGHT;
    const FLOAT: f32 = FLOATING_PILL_FLOAT;
    let ctx = root.ctx().clone();
    let window = ctx.content_rect();
    let width = (window.width() * 0.3).clamp(ADDRESS_PILL_MIN_WIDTH, 520.0);
    let rect = Rect::from_min_size(
        pos2(
            window.center().x - width * 0.5,
            window.max.y - HEIGHT - FLOAT,
        ),
        vec2(width, HEIGHT),
    );
    egui::Area::new(Id::new("zervo_floating_pill"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(&ctx, |ui| {
            ui.set_clip_rect(window.expand(glass::room(chrome.palette.radius(Tier::Pill))));
            let mut host = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            // Over the page, like every other floating surface: its ink has to
            // follow whatever it is sitting on rather than the theme.
            let restore = chrome.palette;
            chrome.palette = chrome.palette.reaching(PANEL_REACH).over(rect);
            draw_address_pill(&mut host, chrome, actions, HEIGHT);
            chrome.palette = restore;
            ui.advance_cursor_after_rect(rect);
        });
}

/// How long the light takes to cross the address pill once.
///
/// Slower than a spinner turns, because it is saying the same thing over a
/// longer object: a light that crossed a 460pt pill three times a second would
/// read as an alarm rather than as progress.
const SWEEP_PERIOD: f64 = 1.5;

/// One pass of light across a surface that is changing size.
///
/// Real glass catches the light when it moves, and this is the cheapest way to
/// say a thing is glass and not a rectangle: no shader, one gradient, and it
/// rides the clock the change is already on rather than introducing a second.
/// `phase` is that clock, 0 to 1; outside it nothing is drawn.
fn paint_specular(painter: &egui::Painter, rect: Rect, palette: &Palette, phase: f32) {
    if !palette.appearance.sweep || !(0.0..1.0).contains(&phase) || !rect.is_positive() {
        return;
    }
    // Narrow, and travelling further than the surface is wide so it enters and
    // leaves rather than appearing in the middle of it.
    let width = (rect.width() * 0.35).max(24.0);
    let x = rect.min.x - width + (rect.width() + width * 2.0) * phase;
    paint_sweep(
        &painter.with_clip_rect(rect),
        Rect::from_min_size(pos2(x, rect.min.y), vec2(width, rect.height())),
        // White rather than the accent: this is a highlight on the material,
        // not a thing the material is saying.
        Color32::WHITE.gamma_multiply(0.10),
    );
}

/// A band of light, brightest in the middle and gone at both ends.
fn paint_sweep(painter: &egui::Painter, rect: Rect, colour: Color32) {
    const STEPS: usize = 16;
    let mut mesh = Mesh::default();
    for step in 0..=STEPS {
        let t = step as f32 / STEPS as f32;
        // A raised cosine: zero at both ends, one in the middle, and no corner
        // in the curve where a triangle would have one.
        let shade = colour.gamma_multiply(0.5 - 0.5 * (t * std::f32::consts::TAU).cos());
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
/// Where the notification bell sits, stashed by the address bar for the toast
/// stack to grow out of. The two are drawn at opposite ends of the frame.
const BELL_ANCHOR: &str = "zervo_bell_anchor";

fn draw_address_pill(
    ui: &mut Ui,
    chrome: &mut ChromeContext,
    actions: &mut Vec<UiAction>,
    height: f32,
) {
    let palette = chrome.palette;
    let ctx = ui.ctx().clone();
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

    // The study's pill is 12 points at 36 tall and 9 at 26 — a third of its
    // height, which is `Tier::Panel` at the size the pill actually is. Taking
    // the tier rather than the fraction is what makes it answer to the corner
    // scale: at Candy's 1.35 every surface around it got rounder and the pill
    // stayed at eleven points, because a fraction of a height knows nothing
    // about the ladder.
    let radius = f32::from(palette.radius(Tier::Panel)).min(height * 0.5) as u8;
    glass::paint(
        ui.painter(),
        pill_rect,
        &palette,
        Glass::new(radius)
            // 5a's address pill is lit whether or not it is focused: `0 0 0 1px
            // accent(.28), 0 0 20px accent(.3)`. Focus adds to that rather than
            // being the only moment the accent appears — on an arrangement that
            // lights nothing, both of these come out at nothing and the focus
            // ring below is the whole of it, as it always was.
            .ring(palette.accent_ring(0.28 + 0.5 * focus_t))
            .glow((0.55 + 0.45 * focus_t).min(1.0)),
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
        let tab = chrome.browser.active_tab();
        // A trouble page says what it is, in its own colour. The badge is the
        // one part of the chrome that reports on the page rather than on the
        // browser, and a globe beside `zervo://certificate` is it reporting
        // the opposite of the truth.
        if let Some(crate::state::TabKind::Trouble(trouble)) = tab.map(|tab| tab.kind) {
            let hint: &str = match trouble {
                crate::trouble::Trouble::Unsupported => "The engine cannot render this page",
                crate::trouble::Trouble::Offline => "No network",
                crate::trouble::Trouble::Certificate => "The certificate does not match this site",
                crate::trouble::Trouble::NotFound => "Nothing answered at this address",
                crate::trouble::Trouble::Crashed => "The engine fell over on this page",
            };
            (trouble.icon(), trouble.badge_tint(&palette), hint)
        } else {
            let url = tab.map(|tab| tab.url.as_str());
            match url.map(|url| url.split(':').next().unwrap_or("")) {
                Some("https") => (Icon::Lock, palette.text_muted, "Encrypted connection"),
                Some("http") => (
                    Icon::Warning,
                    theme::mix(palette.text_muted, palette.warning, 0.75),
                    "Not encrypted — anyone on the network can read this",
                ),
                Some("file") => (Icon::Folder, palette.text_muted, "A file on this computer"),
                _ => (Icon::Globe, palette.text_muted, ""),
            }
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

    // ── The bell, beside the security badge, and only once a page has
    // actually raised something. A permanently dead bell is furniture.
    //
    // A laid-out widget rather than a painted one, so the address field's
    // available width already accounts for it. That is what keeps it inside
    // the glass: at the other end it had to share the spinner's reserved slot
    // and hung 21pt outside the pill whenever a page was loading.
    if chrome.notifications.count > 0 {
        let count = chrome.notifications.count;
        let response = icons::icon_button(
            &mut inner,
            if chrome.notifications.open {
                Icon::BellRinging
            } else {
                Icon::Bell
            },
            15.0,
            &palette,
            true,
        );
        // Where the toasts grow from. Stashed rather than returned because the
        // stack is drawn much later, over the content, by which point this Ui
        // is long gone.
        ctx.data_mut(|data| data.insert_temp(Id::new(BELL_ANCHOR), response.rect));

        if count > 1 {
            // A count, sat on the bell's shoulder.
            let badge = Rect::from_center_size(
                pos2(
                    response.rect.center().x + 6.0,
                    response.rect.center().y - 5.0,
                ),
                vec2(13.0, 13.0),
            );
            inner
                .painter()
                .circle_filled(badge.center(), badge.width() / 2.0, palette.accent);
            inner.painter().text(
                badge.center(),
                Align2::CENTER_CENTER,
                if count > 9 {
                    "9+".to_owned()
                } else {
                    count.to_string()
                },
                FontId::proportional(8.5),
                palette.bg,
            );
        }

        if response
            .on_hover_text(if chrome.notifications.open {
                "Hide notifications"
            } else if count == 1 {
                "1 notification"
            } else {
                "Notifications"
            })
            .clicked()
        {
            actions.push(UiAction::ToggleNotifications);
        }
    } else {
        // Nothing to anchor to any more, so the next toast starts from wherever
        // the bell will actually be rather than where it last was.
        ctx.data_mut(|data| data.remove::<Rect>(Id::new(BELL_ANCHOR)));
    }

    let hint = format!("Search with {}…", chrome.settings.search_engine.label());
    // The spinner slot is always reserved so text never jumps when a load
    // starts or finishes.
    let editor = TextEdit::singleline(&mut chrome.browser.address_bar)
        .frame(Frame::NONE)
        .font(FontId::proportional(14.0))
        .text_color(palette.text)
        .vertical_align(egui::Align::Center)
        .hint_text(RichText::new(hint).color(palette.text_muted))
        // 24pt reserves the spinner's slot, so the text never jumps when a
        // load starts or finishes — unless the light is in the glass instead,
        // in which case there is no slot and the text gets the width back.
        .desired_width(
            inner.available_width()
                - if chrome.settings.appearance.pill_progress {
                    0.0
                } else {
                    24.0
                },
        );
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
        if chrome.settings.appearance.pill_progress {
            // The address bar reserves a slot for a spinner that only ever
            // appears at the end of the field. Put the light in the glass
            // itself: it travels the pill once every `SWEEP_PERIOD`, the
            // reserved slot goes back to the text, and progress ends up where
            // the eye already is.
            //
            // Kept inside the pill's straight run — the clip is a rectangle
            // and the pill is not, so a band allowed to reach the rounded ends
            // would smear accent into the corner notches.
            let inset = f32::from(radius);
            let track = pill_rect.shrink2(vec2(inset, 1.0));
            if track.width() > 0.0 {
                let width = track.width() * 0.28;
                let phase = (ctx.input(|input| input.time) % SWEEP_PERIOD) / SWEEP_PERIOD;
                let x = track.min.x - width + (track.width() + width) * phase as f32;
                paint_sweep(
                    &ui.painter().with_clip_rect(track),
                    Rect::from_min_size(pos2(x, track.min.y), vec2(width, track.height())),
                    palette.accent.gamma_multiply(0.55),
                );
                // Nothing else is going to wake us to move it.
                ctx.request_repaint();
            }
        } else {
            inner.add(egui::Spinner::new().size(14.0).color(palette.accent));
        }
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
        /// What stands in for the favicon an internal page will never have.
        ///
        /// The tab row grew one of these and this grid did not, so the same
        /// pinned `zervo://certificate` showed a padlock in the list and a
        /// globe in the grid two inches above it.
        glyph: Option<Icon>,
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
                    glyph: internal_glyph(tab.kind),
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
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(33));
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
                    icons::draw_icon(
                        ui.painter(),
                        icon_rect,
                        essential.glyph.unwrap_or(Icon::Globe),
                        palette.text_muted,
                    );
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
    // Which of `WORKSPACE_COLORS` the first space took, so the ring is rotated
    // to start where the reader put it.
    first: usize,
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
            palette.corner(Tier::Row),
            palette.surface_hover.gamma_multiply(hover_t),
        );
    }
    let colour = theme::workspace_color_from(index, first);
    // 5a's active space is a lit row, not a bolder label: `rgba(w,.26)` behind
    // it, a white inset along the top and `0 0 16px rgba(w,.32)` around it.
    // Which space you are in is the one thing that artboard is about, and a
    // dot four points across was the whole of how it was said.
    if active {
        glass::paint(
            ui.painter(),
            rect,
            palette,
            Glass::tier(Tier::Row)
                .tint(colour)
                .strength(0.55)
                .glow(0.6)
                .no_shadow(),
        );
    }
    let dot = pos2(rect.min.x + 12.0, rect.center().y);
    // `0 0 10px rgba(w,.9)`, as two soft rings rather than a blur — the dot is
    // seven points across and a real blur of it would cost a texture.
    if active && let Some(halo) = palette.accent_ring(1.0) {
        for (radius, alpha) in [(9.0_f32, 0.10_f32), (6.5, 0.16)] {
            ui.painter().circle_filled(
                dot,
                radius,
                colour.gamma_multiply(alpha * halo.a() as f32 / 255.0),
            );
        }
    }
    ui.painter().circle_filled(dot, 4.0, colour);

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
    /// What to draw in the favicon's place on an internal page, which has no
    /// favicon and never will. `None` falls back to the globe.
    glyph: Option<Icon>,
    always_show_close: bool,
    compact: bool,
    /// Whether the caller is painting the selection itself, as one highlight
    /// travelling between rows. When it is, the row must not also paint its
    /// own, or there are two of them again — which is the thing that was
    /// being fixed.
    liquid: bool,
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
    /// Where the row was drawn, so a caller painting the selection itself
    /// knows where the selection has to end up.
    rect: Rect,
    /// Set while a drag is in flight and the pointer is over this row.
    drop_at: Option<DropAt>,
}

/// Something the engine cannot do, said where the feature is.
///
/// These live in the README, where nobody using the browser will read them. A
/// reader who meets a black video player and no explanation concludes the
/// browser is broken; a reader who was told concludes the engine is young.
/// Same fact, opposite outcome — so the place to say it is beside the thing it
/// is about, phrased as what the engine cannot do rather than as what Zervo
/// forgot.
pub(crate) fn limitation(ui: &mut Ui, palette: &Palette, icon: Icon, text: &str) {
    let inset = 12.0;
    let width = ui.available_width();
    let galley = ui.painter().layout(
        text.to_owned(),
        FontId::proportional(11.5),
        palette.text_muted,
        width - inset * 2.0 - 22.0,
    );
    let (rect, _) =
        ui.allocate_exact_size(vec2(width, galley.size().y + inset * 2.0), Sense::hover());
    glass::paint(
        ui.painter(),
        rect,
        palette,
        Glass::tier(Tier::Card).strength(0.5).no_shadow(),
    );
    icons::draw_icon(
        ui.painter(),
        Rect::from_center_size(
            pos2(rect.min.x + inset + 6.0, rect.min.y + inset + 6.0),
            vec2(13.0, 13.0),
        ),
        icon,
        palette.text_muted,
    );
    ui.painter().galley(
        pos2(rect.min.x + inset + 22.0, rect.min.y + inset),
        galley,
        palette.text_muted,
    );
}

/// The accent-tinted glass that says which row you are on.
///
/// One recipe, because there are two callers now: the row painting its own,
/// and the single highlight that travels between rows instead. Two copies of
/// it would be two selections that could stop looking alike.
fn selection_glass(palette: &Palette, strength: f32) -> Glass {
    Glass::tier(Tier::Row)
        // 5a's active tab is `accent(.34)` with `0 0 22px accent(.42)` around
        // it — a lit row, not a slightly darker one. At 0.8 of a 0.42 fill the
        // accent arrived at about a seventh of what the study asks for, and the
        // selected tab read as the same glass as the address pill above it.
        .strength(strength * 0.95)
        .glow(strength * 0.75)
        .tint(palette.active)
        .no_shadow()
}

/// What stands in for the favicon a page with no webview will never have.
///
/// One answer, because two grew apart: the tab row learned this and the
/// essentials grid did not, so a pinned trouble page had two different icons
/// in the same sidebar.
fn internal_glyph(kind: TabKind) -> Option<Icon> {
    match kind {
        TabKind::Settings => Some(Icon::Gear),
        TabKind::History => Some(Icon::History),
        TabKind::Downloads => Some(Icon::Download),
        TabKind::Trouble(trouble) => Some(trouble.icon()),
        TabKind::Web | TabKind::NewTab => None,
    }
}

/// Whether reloading this kind of tab means anything.
///
/// A web page, obviously — and a trouble page, which stands in for one that
/// would not load and whose whole purpose is to be tried again. The other
/// internal pages are drawn from state that is already live; reloading one
/// would be a button that redraws what is already on the screen.
fn reloadable(kind: TabKind) -> bool {
    matches!(kind, TabKind::Web | TabKind::Trouble(_))
}

/// The ink for a row that is `select_t` of the way into being selected.
///
/// The selected row is the one surface in the chrome whose colour the reader
/// picked, so it is the one whose ink cannot be a constant: at the ratio turn 5
/// asks for, a pale accent under the dark theme's own pale ink is a row you
/// cannot read. [`Palette::ink_on`] answers for the settled colour; this
/// crossfades to it on the row's own clock, so the text does not change colour
/// in a step halfway through the row arriving.
fn row_ink(palette: &Palette, select_t: f32) -> (Color32, Color32) {
    if select_t <= 0.0 {
        return (palette.text, palette.text_muted);
    }
    let (text, muted) = palette.ink_on(palette.active);
    (
        theme::mix(palette.text, text, select_t),
        theme::mix(palette.text_muted, muted, select_t),
    )
}

/// Where the travelling selection is, and where it is going.
#[derive(Clone, Copy)]
struct Liquid {
    from: Rect,
    to: Rect,
    started: f64,
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
            rect,
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
    // The row's own ink, before anything is drawn with it: at full candy the
    // selected row is half its accent, and the theme's ink is not guaranteed to
    // read on that.
    let (ink, ink_muted) = row_ink(palette, select_t);
    let painter = ui.painter();

    // Animated fill: transparent -> hover surface -> accent-tinted glass.
    if hover_t > 0.0 {
        painter.rect_filled(
            rect,
            palette.corner(Tier::Row),
            palette.surface_hover.gamma_multiply(hover_t),
        );
    }
    if select_t > 0.0 && !style.liquid {
        glass::paint(painter, rect, palette, selection_glass(palette, select_t));
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
        // Thirty a second, not the display's maximum. A spinner reads the same
        // either way, and this one can run indefinitely: opening Zervo with a
        // `zervo://` address puts the homepage behind the internal page, and a
        // background tab is throttled, so the engine may never call its load
        // complete. That left this arc turning at a hundred and twelve frames a
        // second, forever, for a tab nobody is looking at.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    } else if let Some(glyph) = style.glyph {
        let icon_rect = Rect::from_center_size(icon_center, vec2(14.0, 14.0));
        icons::draw_icon(painter, icon_rect, glyph, ink_muted);
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
        icons::draw_icon(painter, icon_rect, Icon::Globe, ink_muted);
    }

    let text_color = if style.selected || hovered {
        ink
    } else {
        ink_muted
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
            painter.rect_filled(
                close_rect,
                palette.corner(Tier::Control),
                palette.surface_hover,
            );
        }
        let glyph_rect = Rect::from_center_size(close_rect.center(), vec2(11.0, 11.0));
        icons::draw_icon(
            painter,
            glyph_rect,
            Icon::Close,
            if close_response.hovered() {
                ink
            } else {
                ink_muted
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
                // The same caret, lying down.
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
        rect,
        drop_at,
    }
}

/// A soft radial light blob: a triangle-fan mesh fading from `color` at the
/// center to transparent at the edge — the ambient light of the aurora page.
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
            theme::content_corners(&palette),
            palette.surface,
        );

    let mut pane = root.new_child(
        egui::UiBuilder::new()
            .max_rect(
                content_rect
                    .shrink2(vec2(6.0, 10.0))
                    .with_min_y(content_rect.min.y + 10.0 + window_controls_room(chrome.settings)),
            )
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

                    if !cfg!(feature = "engine-downloads") {
                        limitation(
                            ui,
                            &palette,
                            Icon::Info,
                            "Built without engine-downloads. Responses the engine cannot \
                             render — an archive, an installer — are not offered for saving, \
                             so they will not appear here.",
                        );
                        ui.add_space(14.0);
                    }

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
                            DownloadState::Complete => (Icon::CheckCircle, palette.success),
                            DownloadState::Cancelled => (Icon::XCircle, palette.text_muted),
                            DownloadState::Failed(_) => (Icon::Warning, palette.warning),
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
                                palette.corner(Tier::Hairline),
                                palette.surface_hover,
                            );
                            if let Some(fraction) = item.fraction() {
                                let filled = Rect::from_min_size(
                                    track.min,
                                    vec2(track.width() * fraction, track.height()),
                                );
                                ui.painter().rect_filled(
                                    filled,
                                    palette.corner(Tier::Hairline),
                                    palette.accent,
                                );
                            }
                            ui.ctx()
                                .request_repaint_after(std::time::Duration::from_millis(33));
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

/// The link target, in the corner of the page.
///
/// Every browser has this and it is not decoration: it is how somebody checks
/// where a link actually goes before clicking it, which is the whole defence
/// against a link that says one thing and points at another.
///
/// Bottom-left, hugging the content card's corner, and clipped to the card so
/// it cannot spill into the chrome. It is drawn over the page rather than
/// reserving space, so nothing reflows when it appears.
/// Interpolate a rectangle, corner for corner.
fn lerp_rect(from: Rect, to: Rect, t: f32) -> Rect {
    Rect::from_min_max(
        pos2(
            from.min.x + (to.min.x - from.min.x) * t,
            from.min.y + (to.min.y - from.min.y) * t,
        ),
        pos2(
            from.max.x + (to.max.x - from.max.x) * t,
            from.max.y + (to.max.y - from.max.y) * t,
        ),
    )
}

/// Notifications raised by pages, stacked down the top-right of the content.
///
/// Returns the id of one the reader dismissed, if any. Clicking anywhere on a
/// toast dismisses it: there is nothing else to do with one, so a separate
/// close target would be a smaller version of the same thing.
fn draw_toasts(
    root: &Ui,
    palette: &Palette,
    content_rect: Rect,
    notifications: &crate::notifications::View,
    actions: &mut Vec<UiAction>,
) {
    let toasts = notifications.visible;
    if toasts.is_empty() {
        return;
    }

    const MARGIN: f32 = 12.0;
    const GAP: f32 = 8.0;
    const PAD: f32 = 10.0;

    let width = 320.0_f32.min(content_rect.width() - MARGIN * 2.0);
    if width < 120.0 {
        // A window too narrow to read one in is a window too narrow to cover
        // with one.
        return;
    }

    // Where the bell is, if the address bar drew one this frame.
    let anchor: Option<Rect> = root
        .ctx()
        .data(|data| data.get_temp::<Rect>(Id::new(BELL_ANCHOR)));

    let mut dismissed = None;
    let mut clear_all = false;
    let mut top = content_rect.top() + MARGIN;

    egui::Area::new(Id::new("zervo_toasts"))
        .order(egui::Order::Foreground)
        .fixed_pos(content_rect.min)
        .show(root.ctx(), |ui| {
            let painter = ui.painter().with_clip_rect(content_rect);
            for toast in toasts {
                let wrap = width - PAD * 2.0;
                let title = painter.layout(
                    ellipsize(&toast.title, 120),
                    FontId::proportional(12.5),
                    palette.text,
                    wrap,
                );
                let body = (!toast.body.trim().is_empty()).then(|| {
                    painter.layout(
                        ellipsize(&toast.body, 300),
                        FontId::proportional(11.5),
                        palette.text_muted,
                        wrap,
                    )
                });

                let title_height = title.size().y;
                let mut height = PAD * 2.0 + title_height;
                if let Some(body) = &body {
                    height += 3.0 + body.size().y;
                }

                let rect = Rect::from_min_size(
                    pos2(content_rect.right() - MARGIN - width, top),
                    vec2(width, height),
                );
                if rect.bottom() > content_rect.bottom() {
                    break;
                }

                // Grow out of the bell rather than simply appear: a
                // notification with no visible cause reads as the window doing
                // something, not the page.
                //
                // Driven by the toast's own age, not `animate_bool_with_time`.
                // Asked to animate a brand-new id towards `true`, egui seeds
                // the entry with the target and hands it straight back, so the
                // ramp never runs and every toast snaps to its resting place
                // fully drawn. The id here is new every time by construction,
                // so that is the only thing that would ever have happened.
                let grow = glass::ease_out(
                    (toast.age.as_secs_f32() / crate::notifications::MORPH.as_secs_f32())
                        .clamp(0.0, 1.0),
                );
                let rect = anchor.map_or(rect, |from| lerp_rect(from, rect, grow));

                // Claim the space. An `Area` whose contents are only painted
                // never grows its own `min_rect`, so egui stores it as
                // zero-sized, `layer_id_at` never resolves the pointer to it,
                // and `is_pointer_over_egui` stays false — which is exactly
                // what the event loop uses to decide a click belongs to the
                // page. The toast would show a pointing-hand cursor, refuse to
                // be dismissed, and pass the click through to the page under it.
                ui.advance_cursor_after_rect(rect);

                // Keyed on the toast's own id rather than its position, so
                // dismissing the one above does not hand its hover state to
                // the next.
                let response =
                    ui.interact(rect, Id::new("zervo_toast").with(toast.id), Sense::click());
                if response.clicked() {
                    dismissed = Some(toast.id);
                }
                if response.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }

                // The same glass every other floating panel is made of, so a
                // theme that changes what a menu looks like changes this too.
                for shape in glass::shapes(
                    rect,
                    palette,
                    Glass::of(Surface::Menu)
                        .radius(Tier::Panel)
                        .opaque(palette.bg)
                        .border(palette.border),
                ) {
                    painter.add(shape);
                }

                // Text only once there is somewhere to put it — at the start of
                // the morph the surface is bell-sized, and anything drawn into
                // it would spill out and then snap into place.
                let ink = ((grow - 0.45) / 0.55).clamp(0.0, 1.0);
                if ink > 0.0 {
                    let text_left = rect.left() + PAD;
                    let text_top = rect.top() + PAD;
                    painter.galley(
                        pos2(text_left, text_top),
                        title,
                        palette.text.gamma_multiply(ink),
                    );
                    if let Some(body) = body {
                        painter.galley(
                            pos2(text_left, text_top + title_height + 3.0),
                            body,
                            palette.text_muted.gamma_multiply(ink),
                        );
                    }
                }

                top = rect.bottom() + GAP;
            }

            // ── Clearing the lot. Only worth offering once the tray is open
            // and there is more than one thing in it — with a single
            // notification, clicking it is already the shorter way.
            if notifications.open && toasts.len() > 1 {
                let rect = Rect::from_min_size(
                    pos2(content_rect.right() - MARGIN - width, top),
                    vec2(width, 26.0),
                );
                if rect.bottom() <= content_rect.bottom() {
                    ui.advance_cursor_after_rect(rect);
                    let response = ui.interact(rect, Id::new("zervo_toasts_clear"), Sense::click());
                    if response.clicked() {
                        clear_all = true;
                    }
                    if response.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    let lit = glass::ease_out(ui.ctx().animate_bool_with_time(
                        Id::new("zervo_toasts_clear_hover"),
                        response.hovered(),
                        0.12,
                    ));
                    for shape in glass::shapes(
                        rect,
                        palette,
                        Glass::of(Surface::Menu)
                            .radius(Tier::Panel)
                            .opaque(palette.bg)
                            .border(palette.border),
                    ) {
                        painter.add(shape);
                    }
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "Clear all",
                        FontId::proportional(11.5),
                        theme::mix(palette.text_muted, palette.text, lit),
                    );
                }
            }
        });

    if let Some(id) = dismissed {
        actions.push(UiAction::DismissNotification(id));
    }
    if clear_all {
        actions.push(UiAction::ClearNotifications);
    }
}

fn draw_status_text(root: &Ui, palette: &Palette, content_rect: Rect, status: &str) {
    if status.trim().is_empty() {
        return;
    }
    let painter = root.ctx().layer_painter(egui::LayerId::background());
    let font = FontId::proportional(11.5);
    // Long URLs are the common case, so give it most of the card and trim the
    // middle rather than the end — the host is the part worth reading, and so
    // is the end of the path.
    let text = ellipsize(status, 128);
    let galley = painter.layout_no_wrap(text, font, palette.text_muted);
    let pad = vec2(8.0, 4.0);
    let size = galley.size() + pad * 2.0;
    let rect = Rect::from_min_size(
        pos2(content_rect.left(), content_rect.bottom() - size.y),
        size,
    )
    .intersect(content_rect);
    let painter = painter.with_clip_rect(content_rect);
    for shape in glass::shapes(
        rect,
        palette,
        Glass::of(Surface::Menu)
            .opaque(palette.bg)
            // Square where it meets the card's own edges, rounded only on the
            // corner that faces into the page.
            .radius_exact(0)
            .border(palette.border),
    ) {
        painter.add(shape);
    }
    painter.galley(rect.min + pad, galley, palette.text_muted);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Ui::set_min_width` carries `debug_assert!(0.0 <= width)`, and the
    /// sidebar hands it `ui.available_width()` — from a panel that is
    /// `exact_size(stored * reveal)` and therefore sweeps through every width
    /// between nothing and its own twenty-two points of margin every time the
    /// layout changes.
    ///
    /// egui clamps that at zero rather than going negative, so the call is
    /// safe. This pins that: if a future egui stops clamping, the sidebar
    /// starts panicking on the first frame of every morph, in every build with
    /// debug assertions on — which is the profile this is developed in.
    #[test]
    fn a_panel_narrower_than_its_margins_reports_no_width_rather_than_negative() {
        let margins = f32::from(SIDEBAR_MARGIN.left + SIDEBAR_MARGIN.right);
        let ctx = egui::Context::default();
        let mut seen: Vec<f32> = Vec::new();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            for width in [0.0_f32, 2.0, 12.0, margins, 248.0] {
                Panel::left(Id::new("t").with(width.to_bits()))
                    .resizable(false)
                    .exact_size(width)
                    .frame(Frame::NONE.inner_margin(SIDEBAR_MARGIN))
                    .show_inside(ui, |ui| {
                        seen.push(ui.available_width());
                        // The call the sidebar makes. Without the clamp this
                        // line is the crash.
                        ui.set_min_width(ui.available_width().max(0.0));
                    });
            }
        });
        assert_eq!(seen.len(), 5, "every panel ran");
        assert!(
            seen.iter().all(|width| *width >= 0.0),
            "egui has stopped clamping, and `set_min_width` will now assert: {seen:?}"
        );
        assert!(
            seen.last().is_some_and(|width| *width > 0.0),
            "and a panel wider than its margins still has room in it: {seen:?}"
        );
    }
}
