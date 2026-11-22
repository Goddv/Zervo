//! The Zervo chrome, drawn with egui. Everything lives in the left
//! sidebar — nav icons beside the traffic lights, a large search pill,
//! a grid of pinned "essential" tabs, workspaces with tab rows, and a
//! bottom utility bar — around a floating rounded card holding the web
//! content (or the zervo://settings tab). Pure UI — emits `UiAction`s.

use crate::l10n::text as tr;
use crate::overlay_glass::ComboBoxGlassExt;
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
mod command;
mod frame;
mod layout_editor;
mod settings_page;
mod setup;
mod spaces;
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
pub use command::{CommandBar, CommandMode};
pub use frame::{CardFrame, Corners, PANEL_REACH, finish_content_frame, vertical_gradient};
pub use text::{
    anchored_popup, clamp_into, display_name, ellipsize, host_of, initial, normalize_url,
};

#[derive(Debug)]
pub enum UiAction {
    OpenCommand(CommandMode),
    OpenAddress(String),
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
    RemoveWorkspace(usize),
    SelectWorkspace(usize),
    OpenSettings,
    CustomizeChrome,
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
    PlaceWidget {
        index: usize,
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
    ChangeLanguage,
    /// A setting changed that needs nothing re-applied — just write it out.
    PersistSettings,
}

/// AppKit surfaces positioned beneath the browser chrome.
#[derive(Default)]
pub struct NativeGlass {
    pub sidebar_available: bool,
    pub glass_available: bool,
    pub sidebar: Option<Rect>,
    pub address: Option<(Rect, f32)>,
}

pub struct UiOutput {
    pub actions: Vec<UiAction>,
    #[cfg(target_os = "macos")]
    pub native_glass: NativeGlass,
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
    /// Visible portion of the revealed sidebar, including its non-widget space.
    pub sidebar_overlay: Option<Rect>,
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
    pub window_controls: Option<Rect>,
    pub device_pixels_per_css_pixel: f32,
    pub native_glass: NativeGlass,
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

impl ChromeContext<'_> {
    fn uses_native_sidebar(&self) -> bool {
        self.native_glass.sidebar_available
            && self.palette.translucency.is_translucent()
            && self.settings.layout.is_sidebar()
    }

    pub(crate) fn window_controls_room(&self) -> f32 {
        if self.settings.layout.is_sidebar() {
            0.0
        } else {
            self.window_controls
                .map_or(0.0, |controls| controls.bottom() + 8.0)
        }
    }
}

/// Draw the chrome into the root `Ui` handed to us by `EguiGlow::run`. The
/// caller registers the webview blit at `content_rect` (unless an internal
/// page is open), then calls [`finish_content_frame`].
fn language_picker(ui: &mut Ui, settings: &mut Settings, actions: &mut Vec<UiAction>) {
    let languages = crate::l10n::languages();
    let selected = languages
        .iter()
        .find(|(locale, _)| *locale == settings.language)
        .map_or_else(|| tr("language-system"), |(_, name)| *name);
    let mut chosen = None;
    let label = ui.label(tr("ui-language"));
    let picker = egui::ComboBox::from_id_salt("interface-language")
        .selected_text(selected)
        .show_glass(ui, |ui| {
            if ui
                .selectable_label(settings.language.is_empty(), tr("language-system"))
                .clicked()
            {
                chosen = Some("");
            }
            for (locale, name) in languages {
                if ui
                    .selectable_label(settings.language == locale, name)
                    .clicked()
                {
                    chosen = Some(locale);
                }
            }
        });
    picker.response.labelled_by(label.id);
    if let Some(locale) = chosen
        && locale != settings.language
    {
        settings.language = locale.to_owned();
        actions.push(UiAction::ChangeLanguage);
    }
}

pub fn draw(root: &mut Ui, chrome: &mut ChromeContext) -> UiOutput {
    let mut actions = Vec::new();

    // First run takes the whole frame, chrome and page both. There is nothing
    // behind it worth clicking on by accident, and a half-configured browser
    // showing through a setup screen would be answering questions it has not
    // asked yet.
    if setup::draw(root, chrome, &mut actions) {
        let window = root.ctx().content_rect();
        return UiOutput {
            actions,
            #[cfg(target_os = "macos")]
            native_glass: std::mem::take(&mut chrome.native_glass),
            content_rect: window,
            page_rect: window,
            settings_open: true,
            sidebar_overlay: None,
            controls_open: false,
            ambient: false,
        };
    }

    if chrome.controls.is_empty() {
        for (key, mode) in [
            (Key::L, CommandMode::CurrentTab),
            (Key::T, CommandMode::NewTab),
        ] {
            if root.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, key)) {
                chrome.browser.command_bar =
                    Some(CommandBar::new(mode, &chrome.browser.address_bar));
            }
        }
    }

    // Autohide reveal: decided before the sidebar is drawn, but painted last
    // as a floating overlay. The collapsed handle stays allocated underneath
    // either way, so revealing never reflows the content card — and so never
    // resizes the webview, which would relayout the page.
    let peek = sidebar_peek(root, chrome);
    // Where the window begins, before the sidebar has taken its share of it.
    // The page reaches back to here once the chrome is floating over it.
    let window = root.available_rect_before_wrap();
    let layout = chrome.settings.layout;
    let present = glass::ease_out(root.ctx().animate_bool_with_time(
        Id::unique("zervo_chrome_present"),
        layout.is_sidebar(),
        chrome.palette.material.animation,
    ));
    let active_kind = chrome.browser.active_tab().map(|tab| tab.kind);
    let will_float = layout.is_sidebar()
        && !chrome.uses_native_sidebar()
        && chrome.palette.appearance.seam.chrome_floats()
        && matches!(active_kind, Some(TabKind::NewTab));
    let native_sidebar = chrome.uses_native_sidebar() && present >= 1.0;
    if !native_sidebar {
        paint_chrome_base(
            root,
            &chrome.palette,
            chrome.settings.top_glow,
            chrome.palette.chrome_tint(),
            None,
        );
    }
    // The docked and revealed sidebar share widget IDs. Draw only one per frame.
    let sidebar_docked = present > 0.0;
    let floating_sidebar = if sidebar_docked {
        draw_sidebar(root, chrome, &mut actions, present, will_float)
    } else {
        None
    };

    let outer = root.available_rect_before_wrap();
    if native_sidebar {
        paint_chrome_base(
            root,
            &chrome.palette,
            chrome.settings.top_glow,
            chrome.palette.chrome_tint(),
            chrome.native_glass.sidebar.map(|sidebar| sidebar.right()),
        );
    }
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
        egui::Area::new(Id::unique("zervo_crossing"))
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
        ctx.request_repaint();
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
        egui::Area::new(Id::unique("zervo_sidebar_float"))
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
                sidebar_body(&mut body, chrome, &mut actions);
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

    let sidebar_overlay = if sidebar_docked {
        None
    } else {
        draw_sidebar_peek(root, chrome, &mut actions, peek)
    };

    if let Some(status) = chrome.status
        && chrome.controls.is_empty()
        && chrome.browser.command_bar.is_none()
    {
        draw_status_text(root, &chrome.palette, content_rect, status);
    }

    // Page-initiated UI last, over everything else it might overlap.
    let origin = chrome
        .browser
        .active_tab()
        .and_then(|tab| url::Url::parse(&tab.url).ok())
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| tr("chrome-this-page").to_owned());
    let scale = root.pixels_per_point();
    chrome.controls.draw(
        root,
        &chrome.palette,
        crate::controls::PageCoordinates {
            bounds: content_rect,
            points_per_device_pixel: scale.recip(),
            points_per_css_pixel: chrome.device_pixels_per_css_pixel / scale,
        },
        &origin,
        chrome.vault,
    );
    draw_toasts(
        root,
        &chrome.palette,
        content_rect,
        &chrome.notifications,
        &mut actions,
    );
    let controls_open = !chrome.controls.is_empty() || command::draw(root, chrome, &mut actions);

    UiOutput {
        actions,
        #[cfg(target_os = "macos")]
        native_glass: std::mem::take(&mut chrome.native_glass),
        content_rect,
        page_rect,
        settings_open,
        sidebar_overlay,
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
        // The window chrome or AppKit paints the background beneath the panel.
        .frame(Frame::NONE.inner_margin(SIDEBAR_MARGIN))
        .show_separator_line(false)
        .show(root, |ui| {
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
                if chrome.uses_native_sidebar() {
                    let bounds = ui.ctx().content_rect();
                    chrome.native_glass.sidebar = Some(Rect::from_min_max(
                        bounds.min,
                        pos2(
                            (ui.max_rect().right() + f32::from(SIDEBAR_MARGIN.right))
                                .min(bounds.right()),
                            bounds.bottom(),
                        ),
                    ));
                }
                sidebar_body(ui, chrome, actions);
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
            sidebar_body(&mut body, chrome, actions);
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

fn sidebar_nav_offset(
    controls: Rect,
    sidebar_left: f32,
    sidebar_width: f32,
    align: bool,
    button_count: usize,
) -> egui::Vec2 {
    let row_height = SIDEBAR_ICON + 8.0;
    let inset = (controls.right() + 8.0 - sidebar_left).max(0.0);
    // icon_button adds six points on either side of the glyph.
    if align && inset + button_count as f32 * (SIDEBAR_ICON + 12.0) <= sidebar_width {
        vec2(inset, controls.center().y - row_height * 0.5)
    } else {
        vec2(0.0, controls.bottom() + 8.0)
    }
}

/// Draw the sidebar in its docked panel or its Full Page overlay.
fn sidebar_body(ui: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    let palette = chrome.palette;
    let drag = ui.interact(
        ui.max_rect(),
        ui.scope_id().with("sidebar_drag"),
        Sense::drag(),
    );
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
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if chrome.settings.show_sidebar_tools {
                    draw_sidebar_tools(ui, chrome, actions);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::icon_button(ui, Icon::Sliders, 16.0, &palette, true)
                        .on_hover_text(tr("layout-customize"))
                        .clicked()
                    {
                        actions.push(UiAction::CustomizeChrome);
                    }
                });
            });
            ui.add_space(4.0);
            ui.separator();
            spaces::switcher(ui, chrome, actions);
        });

    if chrome.settings.show_sidebar_widgets {
        draw_sidebar_shelf(ui, chrome, actions);
    }

    // Navigation sits clear of the macOS window controls.
    {
        let nav_height = SIDEBAR_ICON + 8.0;
        let row_top = ui.cursor().top();
        // The overlay moves its whole body. Use the settled origin to choose
        // a row and inset so the controls travel with it from the first frame.
        let sidebar_left = ui.ctx().content_rect().left() + f32::from(SIDEBAR_MARGIN.left);
        let button_count = 3
            + usize::from(chrome.settings.show_forward_button)
            + usize::from(chrome.settings.show_reload_button);
        let offset = chrome
            .window_controls
            .map(|controls| {
                sidebar_nav_offset(
                    controls,
                    sidebar_left,
                    ui.max_rect().width(),
                    chrome.settings.appearance.align_nav,
                    button_count,
                )
            })
            .unwrap_or_default();
        ui.add_space((offset.y - row_top).max(0.0));
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
        // The width check above places the whole row below the traffic lights
        // when the sidebar is too narrow for both to fit on one line.
        let row = Rect::from_min_size(
            pos2(ui.max_rect().min.x, ui.cursor().min.y),
            vec2(ui.max_rect().width(), nav_height),
        );
        let mut nav = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let mut clip = row.intersect(ui.clip_rect());
        if offset.x > 0.0
            && let Some(controls) = chrome.window_controls
        {
            // During the slide keep the moving icons clear of the fixed
            // traffic lights until they have reached their own column.
            clip.min.x = clip.min.x.max(controls.right() + 8.0);
        }
        nav.set_clip_rect(clip);
        // The study's nav icons sit shoulder to shoulder — five 26pt boxes in a
        // row, no gap. egui's own 8pt item spacing put 32 points between them,
        // which is what pushed the fifth off the end of a 226-point column.
        nav.spacing_mut().item_spacing.x = 0.0;
        {
            let ui = &mut nav;
            if offset.x > 0.0 {
                ui.add_space(offset.x);
            }

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

            let sidebar_label = if chrome.settings.layout.is_sidebar() {
                tr("chrome-hide-sidebar-s")
            } else {
                tr("chrome-show-sidebar")
            };
            if icons::icon_button(ui, Icon::Sidebar, SIDEBAR_ICON, &palette, true)
                .on_hover_text(sidebar_label)
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
                    .on_hover_text(tr("chrome-reload-r"))
                    .clicked()
            {
                actions.push(UiAction::Reload);
            }
            star = draw_favourite_star(ui, chrome, actions);
        }
        ui.advance_cursor_after_rect(row);
        ui.add_space(NAV_ROW_GAP);
        draw_address_pill(ui, chrome, actions, 36.0);
        ui.add_space(12.0);
        draw_favourites_card(ui, chrome, actions, star);
    }

    spaces::body(ui, chrome, actions);
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
                    .with_min_y(content_rect.min.y + 28.0 + chrome.window_controls_room()),
            )
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr("common-history"))
                .size(24.0)
                .color(palette.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !chrome.library.history.is_empty()
                && icons::icon_button(ui, Icon::Trash, 15.0, &palette, true)
                    .on_hover_text(tr("chrome-clear-all-history"))
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
            .hint_text(RichText::new(tr("chrome-search-history")).color(palette.text_muted))
            .desired_width(editor.available_width()),
    );
    ui.advance_cursor_after_rect(field);
    ui.add_space(14.0);

    let now = crate::app::now();
    let groups = chrome.library.browse(&chrome.browser.history_query, now);
    if groups.is_empty() {
        ui.label(
            RichText::new(if chrome.library.history.is_empty() {
                tr("chrome-nothing-here-yet")
            } else {
                tr("chrome-no-pages-match-that-search")
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
                        visit.local_time().strftime("%H:%M").to_string(),
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
                        .interact(
                            forget,
                            Id::unique("zervo_forget").with(index),
                            Sense::click(),
                        )
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

    let response = icons::icon_button(ui, Icon::Star, SIDEBAR_ICON, &palette, !url.is_empty());
    let label = if saved {
        tr("chrome-remove-from-favourites")
    } else {
        tr("chrome-add-to-favourites")
    };
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            response.enabled() && !url.is_empty(),
            label,
        )
    });
    let rect = response.rect;
    let lit = glass::ease_out(ui.ctx().animate_bool_with_time(
        ui.scope_id().with("favourite_lit"),
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
            Rect::from_center_size(rect.center(), vec2(SIDEBAR_ICON, SIDEBAR_ICON)),
            Icon::Star,
            palette.accent.gamma_multiply(lit),
        );
    }
    if response.on_hover_text(label).clicked() {
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
    let id = Id::unique(key);
    let pointer = ctx.input(|input| input.pointer.latest_pos());
    let was_open = ctx.data(|data| data.get_temp::<bool>(id)).unwrap_or(false);

    let card = anchored_popup(anchor, size, ctx.content_rect(), true);
    // Cover the gap in either direction when the panel flips.
    let (top, bottom) = if card.top() >= anchor.bottom() {
        (anchor.bottom(), card.top())
    } else {
        (card.bottom(), anchor.top())
    };
    let bridge = Rect::from_min_max(
        pos2(anchor.left().min(card.left()), top),
        pos2(anchor.right().max(card.right()), bottom),
    );
    let open = pointer.is_some_and(|pos| {
        anchor.contains(pos)
            || (was_open && (card.expand(6.0).contains(pos) || bridge.contains(pos)))
    });
    ctx.data_mut(|data| data.insert_temp(id, open));

    let grow = glass::ease_out(ctx.animate_bool_with_time(
        id.with("grow"),
        open,
        palette.material.animation.min(0.13),
    ));
    if grow <= 0.0 {
        return None;
    }
    let height = (card.height() * grow).max(1.0).min(card.height());
    let top = if card.bottom() <= anchor.top() {
        card.bottom() - height
    } else {
        card.top()
    };
    let drawn = Rect::from_min_size(pos2(card.left(), top), vec2(card.width(), height));

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
            let surface = crate::overlay_glass::Surface::begin(ui);
            if !surface.is_native() {
                glass::paint(
                    painter,
                    drawn,
                    &palette.reaching(PANEL_REACH),
                    Glass::of(Surface::Menu)
                        .radius(Tier::Panel)
                        .opaque(palette.bg)
                        .border(palette.border),
                );
            }
            // The contents keep the tight clip, so a half-grown card shows the
            // top of a whole one rather than a squashed one — and so its
            // widgets are not interactive before they are visible.
            ui.set_clip_rect(drawn);
            // The card knows where it landed; its contents do not. Handing the
            // palette down rather than letting the closure capture the outer
            // one is what lets pale text turn dark when this card opens over a
            // white page in dark mode.
            let ink = if surface.is_native() {
                *palette
            } else {
                palette.reaching(PANEL_REACH).over(drawn)
            };
            add(ui, card, ink);
            surface.finish(ui, drawn, crate::overlay_glass::Role::Panel);
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
    let mut chosen = None;
    egui::Area::new(key)
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(ctx, |ui| {
            let painter = ui.painter();
            let surface = crate::overlay_glass::Surface::begin(ui);
            let fallback = palette.reaching(PANEL_REACH).over(rect);
            let palette = if surface.is_native() {
                palette
            } else {
                &fallback
            };
            if !surface.is_native() {
                glass::paint(
                    painter,
                    rect,
                    palette,
                    Glass::of(Surface::Menu)
                        .opaque(palette.bg)
                        .border(palette.border),
                );
            }
            crate::overlay_glass::scroll_body(ui, rect.shrink(6.0), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (label, value) in rows {
                    let (row, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                    if response.hovered() {
                        ui.painter().rect_filled(
                            row,
                            palette.corner(Tier::Control),
                            palette.surface_hover,
                        );
                    }
                    crate::overlay_glass::row_label(
                        ui,
                        row.shrink2(vec2(8.0, 0.0)),
                        label,
                        FontId::proportional(13.0),
                        palette.text,
                    );
                    if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                        chosen = Some(value.clone());
                    }
                }
            });
            surface.finish(ui, rect, crate::overlay_glass::Role::Menu);
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

    let favourites = &chrome.library.favourites;
    let editing = chrome.browser.favourite_edit.clone();
    let id = Id::unique("zervo_favourites_card");

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
                tr("common-favourites"),
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
                    tr("chrome-show-as-a-list")
                } else {
                    tr("chrome-show-as-tiles")
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
                    tr("chrome-nothing-saved-yet-the-star-adds-this"),
                    FontId::proportional(12.0),
                    palette.text_muted,
                );
                return;
            }

            crate::overlay_glass::scroll_body(ui, body, |ui| {
                let body = ui.max_rect();
                if grid {
                    for (index, entry) in favourites.iter().enumerate() {
                        let (url, title) = (&entry.url, &entry.title);
                        let tile = Rect::from_min_size(
                            body.min + vec2((index % 4) as f32 * TILE, (index / 4) as f32 * TILE),
                            vec2(TILE - 6.0, TILE - 6.0),
                        );
                        ui.advance_cursor_after_rect(tile);
                        if !ui.is_rect_visible(tile) {
                            continue;
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

                for (index, entry) in favourites.iter().enumerate() {
                    let (url, title) = (&entry.url, &entry.title);
                    let row = Rect::from_min_size(
                        pos2(body.min.x, body.min.y + index as f32 * ROW),
                        vec2(body.width(), ROW - 2.0),
                    );
                    ui.advance_cursor_after_rect(row);
                    if !ui.is_rect_visible(row) {
                        continue;
                    }
                    let being_edited = editing.as_ref().is_some_and(|(at, _)| at == url);
                    let response = ui.interact(row, id.with(("row", index)), Sense::click());
                    // Hover from the pointer, not the row's response: the controls sit
                    // on top of the row, which stops the row counting as hovered.
                    let over = pointer.is_some_and(|pos| row.contains(pos));
                    if over && !being_edited {
                        ui.painter().rect_filled(
                            row,
                            palette.corner(Tier::Row),
                            palette.surface_hover,
                        );
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
                            let focus_key = Id::unique("zervo_ws_focus");
                            if ui.ctx().data(|data| data.get_temp::<usize>(focus_key))
                                != Some(index)
                            {
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
                        icons::draw_icon(
                            ui.painter(),
                            save.shrink(3.0),
                            Icon::Check,
                            palette.accent,
                        );
                        let saved = ui
                            .interact(save, id.with(("save", index)), Sense::click())
                            .on_hover_text(tr("common-save"))
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
                            .on_hover_text(tr("chrome-discard"))
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

                    crate::overlay_glass::row_label(
                        ui,
                        row.with_min_x(row.left() + 28.0)
                            .with_max_x(row.right() - 46.0),
                        display_name(title, url),
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
                            .on_hover_text(tr("common-rename"))
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
                            .on_hover_text(tr("common-remove"))
                            .clicked()
                        {
                            actions.push(UiAction::RemoveFavourite(url.clone()));
                        }
                    }
                    if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                        actions.push(UiAction::Navigate(url.clone()));
                    }
                }
            });
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
    let rows = chrome.downloads.items.len().clamp(1, 8) as f32;
    let size = vec2(360.0, rows * ROW + 52.0);
    let id = Id::unique("zervo_downloads_card");
    let menu_id = Id::unique("zervo_download_menu");

    hover_card(
        root,
        &palette,
        "zervo_downloads_card",
        anchor,
        size,
        |ui, card, palette| {
            let ctx = ui.ctx().clone();
            let items: Vec<_> = chrome
                .downloads
                .items
                .iter()
                .rev()
                .take(8)
                .map(crate::downloads::Download::row_view)
                .collect();
            let pointer = ctx.input(|input| input.pointer.latest_pos());

            let header =
                Rect::from_min_size(card.min + vec2(12.0, 8.0), vec2(card.width() - 24.0, 20.0));
            ui.painter().text(
                header.left_center(),
                Align2::LEFT_CENTER,
                tr("common-downloads"),
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
                    .on_hover_text(tr("chrome-clear-finished"))
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
                    tr("common-no-downloads"),
                    FontId::proportional(12.0),
                    palette.text_muted,
                );
                return;
            }

            crate::overlay_glass::scroll_body(ui, body, |ui| {
                let body = ui.max_rect();
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
                    ui.advance_cursor_after_rect(row);
                    if !ui.is_rect_visible(row) {
                        continue;
                    }
                    let over = pointer.is_some_and(|pos| row.contains(pos));
                    let response =
                        ui.interact(row, id.with(("row", index)), Sense::click_and_drag());
                    if over {
                        ui.painter().rect_filled(
                            row,
                            palette.corner(Tier::Row),
                            palette.surface_hover,
                        );
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
                                ui.scope_id().with(("download_done", index)),
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
                        DownloadState::Cancelled => tr("chrome-stopped").to_owned(),
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
                        ui.painter().rect_filled(
                            track,
                            palette.corner(Tier::Hairline),
                            palette.border,
                        );
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
                        let mut control = |ui: &mut Ui,
                                           icon: Icon,
                                           tip: &str,
                                           enabled: bool,
                                           key: &str| {
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
                            if control(ui, Icon::Close, tr("chrome-stop"), true, "stop") {
                                actions.push(UiAction::CancelDownload(*download));
                            }
                            control(
                                ui,
                                Icon::Pause,
                                tr("chrome-pause-the-engine-cannot-suspend-a-transfer"),
                                false,
                                "pause",
                            );
                        } else {
                            if control(ui, Icon::Reload, tr("chrome-start-again"), true, "restart")
                            {
                                actions.push(UiAction::RestartDownload(*download));
                            }
                            if *state == DownloadState::Complete {
                                if control(
                                    ui,
                                    Icon::Folder,
                                    tr("chrome-show-in-folder"),
                                    true,
                                    "reveal",
                                ) {
                                    actions.push(UiAction::RevealDownload(*download));
                                }
                                if control(ui, Icon::ExternalLink, tr("common-open"), true, "open")
                                {
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
            });

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
                    (tr("chrome-copy-download-link").to_owned(), 0u8),
                    (tr("chrome-copy-file-name").to_owned(), 1),
                    (tr("chrome-show-in-folder").to_owned(), 2),
                    (tr("common-open").to_owned(), 3),
                    (tr("chrome-start-again").to_owned(), 4),
                    (tr("chrome-remove-from-list").to_owned(), 5),
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

const SIDEBAR_ICON: f32 = 17.0;

fn draw_sidebar_tools(ui: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    let palette = &chrome.palette;
    if icons::icon_button(ui, Icon::History, SIDEBAR_ICON, palette, true)
        .on_hover_text(tr("common-history"))
        .clicked()
    {
        actions.push(UiAction::OpenHistory);
    }
    let active = chrome.downloads.active_count();
    let download = icons::icon_button(ui, Icon::Download, SIDEBAR_ICON, palette, true);
    let anchor = download.rect;
    let hint = if active > 0 {
        ui.painter().circle_filled(
            pos2(anchor.right() - 4.0, anchor.top() + 4.0),
            3.5,
            palette.accent,
        );
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
        crate::l10n::message("chrome-downloads-active", [("count", active.into())])
    } else {
        tr("common-downloads").to_owned()
    };
    if download.on_hover_text(hint).clicked() {
        actions.push(UiAction::OpenDownloads);
    }
    if icons::icon_button(ui, Icon::Gear, SIDEBAR_ICON, palette, true)
        .on_hover_text(tr("common-settings"))
        .clicked()
    {
        actions.push(UiAction::OpenSettings);
    }
    draw_downloads_card(ui, chrome, actions, anchor);
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
    if appearance.spine == crate::theme::Spine::Nothing || chrome.settings.layout.is_sidebar() {
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
    egui::PanelState::load(ctx, Id::unique(SIDEBAR_ID))
        .map(|state| state.outer_rect.width())
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
    let id = Id::unique("zervo_sidebar_peek");
    if chrome.settings.layout.is_sidebar() {
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
    let wanted =
        crate::shot::peek_forced(ctx.cumulative_pass_nr()) || in_trigger || in_panel || busy;

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
    let slide = Id::unique("zervo_sidebar_peek_slide");
    let Some((rect, open)) = peek else {
        // Snap shut, or the next reveal starts half open.
        ctx.animate_value_with_time(slide, 0.0, 0.0);
        return None;
    };
    let t = glass::ease_out(ctx.animate_value_with_time(
        slide,
        if open { 1.0 } else { 0.0 },
        chrome.palette.material.animation.min(PEEK_SLIDE),
    ));
    if t <= 0.0 {
        return None;
    }

    let drawn = rect.translate(vec2((t - 1.0) * rect.width(), 0.0));
    egui::Area::new(Id::unique("zervo_sidebar_peek_area"))
        .order(egui::Order::Foreground)
        .fixed_pos(drawn.min)
        .constrain(false)
        .fade_in(false)
        .show(&ctx, |ui| {
            ui.set_clip_rect(ctx.content_rect());
            let surface = crate::overlay_glass::Surface::begin(ui);
            if !surface.is_native() {
                paint_chrome_fill(
                    ui.painter(),
                    drawn,
                    ctx.content_rect().top(),
                    &chrome.palette,
                    chrome.settings.top_glow,
                    chrome.palette.chrome_tint(),
                );
                if chrome.palette.translucency.is_translucent() {
                    glass::paint(
                        ui.painter(),
                        drawn,
                        &chrome.palette.reaching(PANEL_REACH),
                        Glass::of(Surface::Menu).radius_exact(0).no_shadow(),
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
            }

            let inner = Rect::from_min_max(
                drawn.min + vec2(SIDEBAR_MARGIN.left as f32, SIDEBAR_MARGIN.top as f32),
                drawn.max - vec2(SIDEBAR_MARGIN.right as f32, SIDEBAR_MARGIN.bottom as f32),
            );
            let mut body = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            let restore = chrome.palette;
            if !surface.is_native() {
                chrome.palette = chrome.palette.reaching(PANEL_REACH).over(drawn);
            }
            sidebar_body(&mut body, chrome, actions);
            chrome.palette = restore;
            surface.finish(ui, drawn, crate::overlay_glass::Role::Sidebar);
            // Claim the whole overlay, so the pointer over it resolves to this
            // layer rather than the chrome underneath.
            ui.advance_cursor_after_rect(drawn);
        });
    Some(drawn.intersect(ctx.content_rect()))
}

fn widget_action(change: crate::dashboard::Change) -> UiAction {
    match change {
        crate::dashboard::Change::Add(kind) => UiAction::AddWidget(kind),
        crate::dashboard::Change::Remove(index) => UiAction::RemoveWidget(index),
        crate::dashboard::Change::Swap { a, b } => UiAction::SwapWidgets(a, b),
        crate::dashboard::Change::Place { index, row } => UiAction::PlaceWidget { index, row },
        crate::dashboard::Change::Resize(index, size) => UiAction::ResizeWidget(index, size),
        crate::dashboard::Change::Media(action) => UiAction::MediaAction(action),
    }
}

/// How much room the sidebar's shelf grip takes when the shelf is shut.
const SIDEBAR_GRIP: f32 = 10.0;

fn draw_sidebar_shelf(ui: &mut Ui, chrome: &mut ChromeContext, actions: &mut Vec<UiAction>) {
    let max = crate::dashboard::MAX_HEIGHT;
    let open = chrome.settings.sidebar_shelf_height.clamp(0.0, max);
    let uncovered = crate::dashboard::open_height(&chrome.settings.sidebar_widgets).min(max);

    Panel::bottom("zervo_sidebar_shelf")
        .resizable(false)
        .exact_size(SIDEBAR_GRIP + open)
        .frame(Frame::NONE)
        .show_separator_line(false)
        .show(ui, |ui| {
            let area = ui.max_rect();
            // The grip is at the top edge here rather than the bottom: the
            // shelf grows upward out of the foot of the sidebar, so that is
            // the edge you pull.
            let grip = Rect::from_min_max(area.min, pos2(area.max.x, area.min.y + SIDEBAR_GRIP));
            let response = ui.interact(
                grip,
                Id::unique("zervo_sidebar_shelf_grip"),
                Sense::click_and_drag(),
            );
            let emphasis = glass::ease_out(ui.ctx().animate_bool_with_time(
                Id::unique("zervo_sidebar_shelf_grabber"),
                response.hovered() || response.dragged(),
                0.12,
            ));
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
            }
            crate::dashboard::draw_grabber(ui.painter(), &chrome.palette, grip, emphasis);
            response
                .clone()
                .on_hover_text(tr("chrome-widgets-click-to-open-drag-to-size"));

            crate::overlay_glass::context_menu(&response, |ui| {
                if let Some(kind) =
                    crate::dashboard::widget_menu(ui, &chrome.settings.sidebar_widgets)
                {
                    actions.push(UiAction::AddWidget(kind));
                }
            });

            let mut height = open;
            if response.dragged() {
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
                &chrome.settings.sidebar_widgets,
                shelf,
            ) {
                actions.push(widget_action(change));
            }
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

    let pill_id = ui.scope_id().with("zervo_address_pill");

    let (pill_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());

    let radius = f32::from(palette.radius(Tier::Panel)).min(height * 0.5);
    if chrome.native_glass.sidebar.is_some()
        && chrome.native_glass.glass_available
        && palette.translucency == theme::Translucency::LiquidGlass
    {
        chrome.native_glass.address = Some((pill_rect, radius));
    } else {
        ui.painter().rect_filled(pill_rect, radius, palette.surface);
        ui.painter().rect_stroke(
            pill_rect,
            radius,
            Stroke::new(1.0_f32, palette.border),
            StrokeKind::Inside,
        );
    }
    let radius = radius as u8;

    let mut inner = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(pill_rect.shrink2(vec2(12.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    // What the leading icon says about the connection. While typing it goes
    // back to a magnifying glass, since it is describing the box and not a
    // page any more.
    let (icon, tint, hint) = if chrome.browser.address_bar.is_empty() {
        (Icon::Search, palette.text_muted, "")
    } else {
        let tab = chrome.browser.active_tab();
        // A trouble page says what it is, in its own colour. The badge is the
        // one part of the chrome that reports on the page rather than on the
        // browser, and a globe beside `zervo://certificate` is it reporting
        // the opposite of the truth.
        if let Some(crate::state::TabKind::Trouble(trouble)) = tab.map(|tab| tab.kind) {
            let hint: &str = match trouble {
                crate::trouble::Trouble::Unsupported => {
                    tr("chrome-the-engine-cannot-render-this-page")
                },
                crate::trouble::Trouble::Offline => tr("common-offline"),
                crate::trouble::Trouble::Certificate => {
                    tr("chrome-the-certificate-does-not-match-this-site")
                },
                crate::trouble::Trouble::NotFound => tr("chrome-nothing-answered-at-this-address"),
                crate::trouble::Trouble::Crashed => tr("chrome-the-engine-fell-over-on-this-page"),
            };
            (trouble.icon(), trouble.badge_tint(&palette), hint)
        } else {
            let url = tab.map(|tab| tab.url.as_str());
            match url.map(|url| url.split(':').next().unwrap_or("")) {
                Some("https") => (
                    Icon::Lock,
                    palette.text_muted,
                    tr("chrome-encrypted-connection"),
                ),
                Some("http") => (
                    Icon::Warning,
                    theme::mix(palette.text_muted, palette.warning, 0.75),
                    tr("chrome-not-encrypted-anyone-on-the-network-can"),
                ),
                Some("file") => (
                    Icon::Folder,
                    palette.text_muted,
                    tr("chrome-a-file-on-this-computer"),
                ),
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
        let label = if chrome.notifications.open {
            tr("chrome-hide-notifications").to_owned()
        } else {
            crate::l10n::message("chrome-notification-count", [("count", count.into())])
        };
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, response.enabled(), &label)
        });
        // Where the toasts grow from. Stashed rather than returned because the
        // stack is drawn much later, over the content, by which point this Ui
        // is long gone.
        ctx.data_mut(|data| data.insert_temp(Id::unique(BELL_ANCHOR), response.rect));

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

        if response.on_hover_text(label).clicked() {
            actions.push(UiAction::ToggleNotifications);
        }
    } else {
        // Nothing to anchor to any more, so the next toast starts from wherever
        // the bell will actually be rather than where it last was.
        ctx.data_mut(|data| data.remove::<Rect>(Id::unique(BELL_ANCHOR)));
    }

    let right_inset = if loading && !chrome.settings.appearance.pill_progress {
        24.0
    } else {
        0.0
    };
    let width = (inner.available_width() - right_inset).max(1.0);
    let address = &chrome.browser.address_bar;
    let label = if address.is_empty() {
        tr("chrome-search-or-url")
    } else {
        address.as_str()
    };
    let result = inner.add_sized(
        vec2(width, height - 4.0),
        egui::Button::new(RichText::new(label).size(13.0).color(palette.text))
            .truncate()
            .frame(false),
    );
    let tooltip = if address.is_empty() {
        tr("chrome-search-or-enter-an-address")
    } else {
        address
    };
    if result.on_hover_text(tooltip).clicked() {
        actions.push(UiAction::OpenCommand(CommandMode::CurrentTab));
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
    rename: &mut Option<(usize, WorkspaceName)>,
) -> Rect {
    let desired = vec2(ui.available_width(), 34.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    if response.double_clicked() && editing.is_none() {
        *rename = Some((index, WorkspaceName::Typing(name.to_owned())));
    }

    let hover_t = glass::ease_out(ui.ctx().animate_bool_with_time(
        ui.scope_id().with(("ws_header", index)),
        response.hovered(),
        palette.material.animation,
    ));
    if hover_t > 0.0 {
        ui.painter().rect_filled(
            rect,
            palette.corner(Tier::Row),
            palette.surface_hover.gamma_multiply(hover_t),
        );
    }
    let colour = theme::workspace_color_from(index, first);
    if active {
        ui.painter().rect_filled(rect, 8, palette.surface);
        ui.painter().rect_stroke(
            rect,
            8,
            Stroke::new(1.0_f32, palette.border),
            StrokeKind::Inside,
        );
    }
    let dot = pos2(rect.min.x + 12.0, rect.center().y);
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
                .id(ui.scope_id().with(("ws_name", index))),
        );
        editor.request_focus();

        let control = |ui: &mut Ui, x: f32, icon: Icon, tint: Color32| -> bool {
            let hit = Rect::from_center_size(pos2(x, rect.center().y), vec2(18.0, 18.0));
            let response = ui.interact(
                hit,
                ui.scope_id().with((icon as usize, "ws_edit", index)),
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

    let mut right = rect.right() - 10.0;
    if show_count {
        let count = ui.painter().text(
            pos2(right, rect.center().y),
            Align2::RIGHT_CENTER,
            tab_count.to_string(),
            FontId::proportional(11.5),
            palette.text_muted,
        );
        right = count.left() - 8.0;
    }
    let left = rect.left() + 26.0;
    let ink = if active {
        palette.text
    } else {
        palette.text_muted
    };
    let mut label =
        egui::text::LayoutJob::simple_singleline(name.to_owned(), FontId::proportional(13.0), ink);
    label.wrap = egui::text::TextWrapping::truncate_at_width((right - left).max(0.0));
    let label = ui.painter().layout_job(label);
    ui.painter().galley(
        pos2(left, rect.center().y - label.size().y / 2.0),
        label,
        ink,
    );
    // Renaming any workspace, not only a freshly made one.
    crate::overlay_glass::context_menu(&response, |ui| {
        if ui.button(tr("common-rename")).clicked() {
            *rename = Some((index, WorkspaceName::Typing(name.to_owned())));
            ui.close();
        }
    });
    if response.has_focus() {
        icons::paint_focus(ui, palette, rect, Tier::Row);
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Tab, response.enabled(), active, name)
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
    pinnable: bool,
    pinned: bool,
    workspace: usize,
    spaces: &'a [crate::state::Workspace],
    /// What to draw in the favicon's place on an internal page, which has no
    /// favicon and never will. `None` falls back to the globe.
    glyph: Option<Icon>,
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

/// Share the tab selection between the static row and the travelling highlight.
fn selection_shapes(rect: Rect, palette: &Palette, strength: f32) -> Vec<Shape> {
    if palette.translucency == theme::Translucency::LiquidGlass {
        let marker =
            Rect::from_center_size(pos2(rect.left() + 4.0, rect.center().y), vec2(2.0, 14.0));
        return vec![
            Shape::rect_filled(
                rect,
                palette.corner(Tier::Row),
                palette.text.gamma_multiply(strength * 0.16),
            ),
            Shape::rect_filled(marker, 1, palette.accent.gamma_multiply(strength)),
        ];
    }
    glass::shapes(
        rect,
        palette,
        Glass::tier(Tier::Row)
            .strength(strength * 0.95)
            .glow(strength * 0.75)
            .tint(palette.active)
            .no_shadow(),
    )
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
    if select_t <= 0.0 || palette.translucency == theme::Translucency::LiquidGlass {
        return (palette.text, palette.text_muted);
    }
    let (text, muted) = palette.ink_on(palette.active);
    (
        theme::mix(palette.text, text, select_t),
        theme::mix(palette.text_muted, muted, select_t),
    )
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
        ui.scope_id().with(("tab_row", style.tab_id)),
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
        ui.scope_id().with(("tab_close", style.tab_id)),
        Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::Role::Tab,
            response.enabled(),
            style.selected,
            style.title,
        )
    });
    close_response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            close_response.enabled(),
            crate::l10n::message(
                if style.pinned {
                    "sidebar-unload-pin-title"
                } else {
                    "chrome-close-tab"
                },
                [("title", style.title.into())],
            ),
        )
    });

    let hovered = response.hovered() || close_response.hovered();
    // Keyed by tab identity, not layout position, so closing a tab doesn't
    // shift animation state onto its neighbor.
    let hover_t = glass::ease_out(ui.ctx().animate_bool_with_time(
        ui.scope_id().with(("tab_hover", style.tab_id)),
        hovered && !style.selected,
        palette.material.animation,
    ));
    let select_t = glass::ease_out(ui.ctx().animate_bool_with_time(
        ui.scope_id().with(("tab_sel", style.tab_id)),
        style.selected,
        palette.material.animation,
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
    if select_t > 0.0 {
        painter.extend(selection_shapes(rect, palette, select_t));
        painter.rect_filled(
            Rect::from_center_size(pos2(rect.left() + 2.0, rect.center().y), vec2(3.0, 16.0)),
            1,
            palette.accent.gamma_multiply(select_t),
        );
    }
    if response.has_focus() {
        icons::paint_focus(ui, palette, rect, Tier::Row);
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
    let title_x = rect.min.x + 34.0;
    let title_width = (close_rect.left() - title_x - 4.0).max(0.0);
    let mut title = egui::text::LayoutJob::simple_singleline(
        style.title.to_owned(),
        FontId::proportional(13.5),
        text_color,
    );
    title.wrap = egui::text::TextWrapping::truncate_at_width(title_width);
    let title = painter.layout_job(title);
    painter.galley(
        pos2(title_x, rect.center().y - title.size().y / 2.0),
        title,
        text_color,
    );

    if hovered || style.selected || style.always_show_close || close_response.has_focus() {
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
        if close_response.has_focus() {
            icons::paint_focus(ui, palette, close_rect, Tier::Control);
        }
    }

    crate::overlay_glass::context_menu(&response, |ui| {
        if style.pinnable
            && ui
                .button(if style.pinned {
                    tr("chrome-unpin")
                } else {
                    tr("sidebar-pin-tab")
                })
                .clicked()
        {
            actions.push(UiAction::TogglePin(style.tab_id));
            ui.close();
        }
        if style.spaces.len() > 1 {
            crate::overlay_glass::submenu(ui, tr("sidebar-move-to-space"), |ui| {
                for (index, space) in style
                    .spaces
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| *index != style.workspace)
                {
                    if ui.button(&space.name).clicked() {
                        actions.push(UiAction::MoveTab {
                            tab: style.tab_id,
                            workspace: index,
                            index: space.tabs.len(),
                        });
                        ui.close();
                    }
                }
            });
        }
        let close_label = if style.pinned {
            tr("sidebar-unload-pin")
        } else {
            tr("common-close")
        };
        if ui.button(close_label).clicked() {
            actions.push(UiAction::CloseTab(style.tab_id));
            ui.close();
        }
    });

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
                    .with_min_y(content_rect.min.y + 10.0 + chrome.window_controls_room()),
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
                            RichText::new(tr("common-downloads"))
                                .size(21.0)
                                .strong()
                                .color(palette.text),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !chrome.downloads.items.is_empty()
                                && icons::icon_button(ui, Icon::Trash, 15.0, &palette, true)
                                    .on_hover_text(tr("chrome-clear-finished"))
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
                            tr("chrome-built-without-engine-downloads-responses-the-engine"),
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
                                RichText::new(tr("chrome-no-downloads-yet"))
                                    .size(14.0)
                                    .color(palette.text_muted),
                            );
                            ui.label(
                                RichText::new(tr("chrome-files-you-download-will-appear-here"))
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
                            DownloadState::Cancelled => tr("chrome-cancelled").to_owned(),
                            DownloadState::Failed(error) => crate::l10n::message(
                                "chrome-download-failed",
                                [("error", error.as_str().into())],
                            ),
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
                                .on_hover_text(tr("common-cancel"))
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
                                .on_hover_text(tr("chrome-reveal-in-finder"))
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
                                .on_hover_text(tr("common-open"))
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
                                .on_hover_text(tr("common-remove"))
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
        .data(|data| data.get_temp::<Rect>(Id::unique(BELL_ANCHOR)));

    let mut dismissed = None;
    let mut clear_all = false;
    let mut top = content_rect.top() + MARGIN;

    egui::Area::new(Id::unique("zervo_toasts"))
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
                let grow = if palette.material.animation > 0.0 {
                    glass::ease_out(
                        (toast.age.as_secs_f32() / crate::notifications::MORPH.as_secs_f32())
                            .clamp(0.0, 1.0),
                    )
                } else {
                    1.0
                };
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
                let response = ui.interact(
                    rect,
                    Id::unique("zervo_toast").with(toast.id),
                    Sense::click(),
                );
                if response.clicked() {
                    dismissed = Some(toast.id);
                }
                if response.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }

                let surface = crate::overlay_glass::Surface::begin(ui);
                if !surface.is_native() {
                    glass::paint(
                        &painter,
                        rect,
                        palette,
                        Glass::of(Surface::Menu)
                            .radius(Tier::Panel)
                            .opaque(palette.bg)
                            .border(palette.border),
                    );
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

                surface.finish(
                    ui,
                    rect.intersect(content_rect),
                    crate::overlay_glass::Role::Panel,
                );
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
                    let response =
                        ui.interact(rect, Id::unique("zervo_toasts_clear"), Sense::click());
                    if response.clicked() {
                        clear_all = true;
                    }
                    if response.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    let lit = glass::ease_out(ui.ctx().animate_bool_with_time(
                        Id::unique("zervo_toasts_clear_hover"),
                        response.hovered(),
                        0.12,
                    ));
                    let surface = crate::overlay_glass::Surface::begin(ui);
                    if !surface.is_native() {
                        glass::paint(
                            &painter,
                            rect,
                            palette,
                            Glass::of(Surface::Menu)
                                .radius(Tier::Panel)
                                .opaque(palette.bg)
                                .border(palette.border),
                        );
                    }
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        tr("chrome-clear-all"),
                        FontId::proportional(11.5),
                        theme::mix(palette.text_muted, palette.text, lit),
                    );
                    surface.finish(ui, rect, crate::overlay_glass::Role::Status);
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
    let font = FontId::proportional(11.5);
    let text = ellipsize(status, 128);
    let desired = root
        .painter()
        .layout_no_wrap(text.clone(), font.clone(), palette.text)
        .size();
    let width = (desired.x + 20.0).min((content_rect.width() - 16.0).min(520.0));
    let height = 24.0;
    if width < 24.0 {
        return;
    }
    let rect = Rect::from_min_size(
        pos2(
            content_rect.left() + 8.0,
            content_rect.bottom() - height - 8.0,
        ),
        vec2(width, height),
    );
    let role = crate::overlay_glass::Role::Status;
    egui::Area::new(Id::unique("zervo_link_preview"))
        .order(egui::Order::Foreground)
        .interactable(false)
        .fixed_pos(rect.min)
        .constrain(false)
        .fade_in(false)
        .show(root.ctx(), |ui| {
            let surface = crate::overlay_glass::Surface::begin(ui);
            if !surface.is_native() {
                glass::paint(
                    ui.painter(),
                    rect,
                    palette,
                    Glass::of(Surface::Menu)
                        .opaque(palette.bg)
                        .radius_exact(role.radius())
                        .border(palette.border),
                );
            }
            crate::overlay_glass::row_label(
                ui,
                rect.shrink2(vec2(10.0, 0.0)),
                &text,
                font,
                palette.text_muted,
            );
            surface.finish(ui, rect, role);
            ui.advance_cursor_after_rect(rect);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_navigation_aligns_in_its_settled_width() {
        let controls = Rect::from_min_max(pos2(19.0, 19.0), pos2(79.0, 33.0));
        let width_needed = 75.0 + 5.0 * (SIDEBAR_ICON + 12.0);
        let aligned = sidebar_nav_offset(controls, 12.0, width_needed, true, 5);
        assert_eq!(aligned.x, 75.0);
        assert_eq!(aligned.y + (SIDEBAR_ICON + 8.0) * 0.5, controls.center().y);
        assert_eq!(
            sidebar_nav_offset(controls, 12.0, width_needed - 1.0, true, 5),
            vec2(0.0, 41.0),
        );
        assert_eq!(
            sidebar_nav_offset(controls, 12.0, width_needed - 1.0, true, 4),
            aligned,
        );
    }

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
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            for width in [0.0_f32, 2.0, 12.0, margins, 248.0] {
                Panel::left(Id::unique("t").with(width.to_bits()))
                    .resizable(false)
                    .exact_size(width)
                    .frame(Frame::NONE.inner_margin(SIDEBAR_MARGIN))
                    .show(ui, |ui| {
                        seen.push(ui.available_width());
                        // The call the sidebar makes. Without the clamp this
                        // line is the crash.
                        ui.set_min_width(ui.available_width().max(0.0));
                    });
            }
        });
        frame.textures_delta.clear();
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
