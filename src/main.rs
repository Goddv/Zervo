//! Zervo: winit + Servo + egui chrome.
//!
//! Architecture (mirrors servoshell): one surfman GL context renders to the
//! window; Servo webviews render into an FBO-backed `OffscreenRenderingContext`
//! sized to the content rect, which an egui background paint-callback blits
//! under the chrome. Tabs are one live `WebView` each, switched via
//! show/focus + hide/blur.

// A release build on Windows is a GUI application: without this it is a
// console one, and launching the browser leaves a terminal window sitting
// behind it. Debug builds keep the console, which is where the logs go.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod backdrop;
mod controls;
mod dashboard;
mod downloads;
mod gestures;
mod glass;
mod grid;
mod icons;
mod keyboard;
mod library;
mod net;
mod newtab;
mod passwords;
mod phosphor;
// macOS has its own paths inline, using the AppKit bindings it already has.
#[cfg(not(target_os = "macos"))]
mod platform;
mod settings;
mod state;
mod store;
mod theme;
mod ui;
#[cfg(target_os = "macos")]
mod vibrancy;
mod wallpaper;
mod widgets;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use egui::LayerId;
use egui_glow::{CallbackFn, EguiGlow};
use euclid::{Point2D, Rect, Scale, Size2D};
use keyboard_types::ShortcutMatcher;
use servo::{
    DeviceIndependentPixel, DevicePixel, EditingActionEvent, EventLoopWaker, InputEvent, Key,
    MouseButton as ServoMouseButton, MouseButtonAction, MouseButtonEvent, MouseLeftViewportEvent,
    MouseMoveEvent, NamedKey, RenderingContext, ServoBuilder, WheelDelta, WheelEvent, WheelMode,
    WindowRenderingContext,
};
use url::Url;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::ModifiersState;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

/// How long the chrome takes to cross from one theme to the other.
const THEME_FADE: std::time::Duration = std::time::Duration::from_millis(220);

/// The soonest egui has asked to be woken.
///
/// egui reports the delay it wants through a callback rather than through
/// `EguiGlow::run`, which destructures each `ViewportOutput { commands, .. }`
/// and drops `repaint_delay` on the floor. Without it the loop has no idea
/// whether egui wanted twenty milliseconds or twenty seconds.
///
/// The callback can be invoked from anywhere, so the answer lands here and the
/// event loop collects it after the pass. Requests accumulate by taking the
/// earliest: two cards asking for different delays both want to be served, and
/// the sooner of the two is the one that decides when to wake.
#[derive(Clone, Default)]
struct RepaintAt(Arc<Mutex<Option<std::time::Instant>>>);

impl RepaintAt {
    fn request(&self, delay: std::time::Duration) {
        // `Duration::MAX` is how egui says "nothing pending", and adding it to a
        // point in time would overflow. `checked_add` covers that and every
        // other absurd delay in one.
        let Some(deadline) = std::time::Instant::now().checked_add(delay) else {
            return;
        };
        let mut slot = self.0.lock().unwrap_or_else(|error| error.into_inner());
        *slot = Some(slot.map_or(deadline, |at| at.min(deadline)));
    }

    fn take(&self) -> Option<std::time::Instant> {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }
}

/// Which side of the window owns the pointer between a press and its release.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PointerOwner {
    Free,
    Chrome,
    Page,
}

use crate::app::AppState;
use crate::keyboard::{CMD_OR_ALT, CMD_OR_CONTROL, keyboard_event_from_winit};
use crate::settings::{NewTabPage, Settings};
use crate::state::{BrowserState, TabId, TabKind};
use crate::theme::Palette;
use crate::ui::UiAction;

/// Servo's own user agent already claims Firefox 140, but carries a `Servo/x.y`
/// token and no `Gecko/20100101`. Enough sites match on exactly those to serve a
/// "browser not supported" page, so offer the plain Firefox string instead.
const COMPAT_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:140.0) Gecko/20100101 Firefox/140.0";

fn main() -> Result<(), Box<dyn Error>> {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install crypto provider");

    let event_loop = EventLoop::with_user_event().build()?;
    let mut app = App::Initial(Waker::new(&event_loop));
    Ok(event_loop.run_app(&mut app)?)
}

enum App {
    Initial(Waker),
    Running(RunningApp),
}

struct RunningApp {
    state: Rc<AppState>,
    egui_glow: EguiGlow,
    settings: Settings,
    /// Current OS appearance, driving ThemeMode::Auto.
    system_dark: bool,
    /// Favicon textures by tab, rebuilt when the engine reports changes.
    favicons: HashMap<TabId, egui::TextureHandle>,
    /// Whether the settings page currently covers the content area.
    settings_open: bool,
    modifiers: ModifiersState,
    /// Last cursor position in window physical pixels.
    cursor: PhysicalPosition<f64>,
    /// Last cursor position relative to the webview origin, physical pixels.
    webview_relative_mouse: Cell<Point2D<f32, DevicePixel>>,
    /// The content rect left over after the chrome panels, in egui points.
    content_rect_points: egui::Rect,
    /// A page-initiated dialog or menu is up.
    controls_open: bool,
    /// Accumulates scroll events into trackpad swipes.
    swipe: gestures::Recognizer,
    /// Whichever side the pointer went down on keeps it until the button
    /// comes back up, wherever it has travelled to since.
    ///
    /// Without this a drag that crosses the boundary is handed to the other
    /// side halfway through and never finishes, because the release is
    /// delivered somewhere the press was not: a tab dragged out over the page
    /// loses its drop, and a control dragged off the page onto the sidebar is
    /// left stuck to the cursor.
    pointer_owner: PointerOwner,
    /// Throttles history writes: every visit marks the library dirty, and
    /// rewriting the file on each one would be a lot of churn for a browse.
    library_saved_at: std::time::Instant,
    /// Deadline for a deferred egui repaint (e.g. caret blink) — served via
    /// ControlFlow::WaitUntil instead of a max-FPS redraw loop.
    pending_repaint_at: Option<std::time::Instant>,
    /// Where egui leaves the delay it actually asked for. See `RepaintAt`.
    repaint_at: RepaintAt,
    /// What `apply_theme` was last run for, and the icon last handed to the
    /// Dock. Every settings write used to redo both, and redoing the theme
    /// means restyling egui, retuning the window's appearance and the frosted
    /// material, and telling every webview its prefers-color-scheme changed —
    /// which makes the page relayout. Doing all that because someone toggled
    /// "outline around content", or on every frame of a slider drag, is what
    /// made the chrome jump.
    applied_theme: (
        theme::ThemeMode,
        theme::AccentColor,
        bool,
        theme::Translucency,
    ),
    applied_icon: settings::AppIcon,
    /// A theme change in flight: the palette it started from, and when.
    ///
    /// The chrome crosses over rather than snapping, because half of what the
    /// user sees is not ours to snap. The frosted sidebar backdrop is an
    /// NSVisualEffectView composited by the WindowServer, a frame behind our
    /// own GL output, so an instant switch changed everything opaque on one
    /// frame and the frost on the next — a staggered redraw, and the jolt
    /// people reported. Crossing both over the same interval leaves no single
    /// frame for them to disagree on.
    theme_fade: Option<(Palette, std::time::Instant)>,
    /// File downloads (Servo has no download subsystem — we do it ourselves).
    downloads: downloads::DownloadManager,
    /// The new tab page's photograph: where it comes from, and what is known
    /// about the one currently up.
    wallpaper: wallpaper::Wallpaper,
    /// A blurred copy of the page, for the chrome to frost itself against.
    /// Shared because the copy is taken inside a paint callback, which runs
    /// while `self` is already borrowed by `EguiGlow::run` — and behind a
    /// mutex because egui requires a paint callback to be `Send + Sync`, not
    /// because anything here is on another thread. It never contends.
    page_backdrop: Arc<std::sync::Mutex<backdrop::PageBackdrop>>,
    page_backdrop_texture: Option<theme::Frost>,
    /// Owns the little program that takes the card's corners out of the
    /// framebuffer; it is compiled on first use inside a paint callback.
    corner_eraser: Arc<Mutex<backdrop::Eraser>>,
    /// The uploaded textures for it: the picture, and the blurred copy every
    /// glass surface over it is frosted against. Held here rather than in the
    /// manager because making one needs an egui context, and the manager runs
    /// on a thread that has none.
    wallpaper_texture: Option<egui::TextureHandle>,
    wallpaper_frost: Option<theme::Frost>,
    /// Retained frosted-glass backdrop, kept so its material can be retuned.
    #[cfg(target_os = "macos")]
    _vibrancy: Option<vibrancy::Vibrancy>,
}

impl ApplicationHandler<WakerEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Self::Initial(waker) = self else {
            return;
        };

        let attributes = Window::default_attributes()
            .with_title("Zervo")
            .with_inner_size(LogicalSize::new(1200.0, 800.0));
        #[cfg(target_os = "macos")]
        let attributes = {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attributes
                .with_titlebar_transparent(true)
                .with_title_hidden(true)
                .with_fullsize_content_view(true)
                // surfman samples NSWindow.isOpaque() when it creates the
                // window surface and marks its CALayer accordingly, so this
                // must be set before WindowRenderingContext::new below. It is
                // the only lever that makes translucent chrome possible.
                .with_transparent(true)
        };
        let window = event_loop
            .create_window(attributes)
            .expect("Failed to create winit Window");

        // Frosted-glass backdrop behind the (translucent) chrome.
        #[cfg(target_os = "macos")]
        let vibrancy = vibrancy::install(&window, objc2_app_kit::NSVisualEffectMaterial::Sidebar);

        let display_handle = event_loop
            .display_handle()
            .expect("Failed to get display handle");
        let window_handle = window.window_handle().expect("Failed to get window handle");
        let window_rendering_context = Rc::new(
            WindowRenderingContext::new(display_handle, window_handle, window.inner_size())
                .expect("Could not create RenderingContext for window"),
        );
        window_rendering_context
            .make_current()
            .expect("Could not make window rendering context current");
        // Insurance: surfman samples the window's opacity when it builds its
        // layer, so make sure that layer really is compositing with alpha.
        #[cfg(target_os = "macos")]
        vibrancy::force_layer_transparent(&window);

        // egui paints with the same surfman-created GL context — no second context.
        let mut egui_glow = EguiGlow::new(
            event_loop,
            window_rendering_context.glow_gl_api(),
            None,
            None,
            // Dithering. The chrome's glow strip ramps across 280 points with
            // only about twenty 8-bit steps in it, which is one step every
            // dozen pixels or so — visible banding, and this is the mechanism
            // egui ships to break it up.
            true,
        );

        // The platform accessibility adapter. `EguiGlow::new` builds the
        // `egui_winit::State` but never calls this, so without it egui composes
        // an AccessKit tree every pass and has nobody to hand it to. Requests
        // from the client arrive back through the same proxy the engine's waker
        // uses -- see `WakerEvent`.
        egui_glow
            .egui_winit
            .init_accesskit(event_loop, &window, waker.0.clone());

        // egui hands the delay to a callback and nothing else, so this is the
        // only place it can be caught.
        let repaint_at = RepaintAt::default();
        {
            let slot = repaint_at.clone();
            egui_glow
                .egui_ctx
                .set_request_repaint_callback(move |info| slot.request(info.delay));
        }

        let settings = settings::load();
        let system_dark = matches!(window.theme(), Some(winit::window::Theme::Dark));
        theme::apply(
            &egui_glow.egui_ctx,
            &theme::resolve(settings.theme, system_dark, settings.accent),
        );
        install_fonts(&egui_glow.egui_ctx);

        // Webviews render into this FBO, blitted under the chrome each frame.
        let rendering_context =
            Rc::new(window_rendering_context.offscreen_context(window.inner_size()));

        // Without a config dir Servo keeps the cookie jar, the auth cache and
        // the HSTS list in memory only and never writes them out, so every
        // launch starts logged out of everything.
        let mut opts = servo::Opts::default();
        if let Some(dir) = settings::data_dir() {
            let _ = std::fs::create_dir_all(&dir);
            opts.config_dir = Some(dir);
        }
        let mut preferences = servo::Preferences::default();
        if settings.user_agent_compat {
            preferences.user_agent = COMPAT_USER_AGENT.to_owned();
        }
        let servo = ServoBuilder::default()
            .opts(opts)
            .preferences(preferences)
            .event_loop_waker(Box::new(waker.clone()))
            .build();
        servo.setup_logging();

        let asked_for = std::env::args()
            .nth(1)
            .map(|arg| ui::normalize_url(&arg, settings.search_engine))
            .and_then(|arg| Url::parse(&arg).ok());
        // A zervo:// address on the command line names one of Zervo's own
        // pages, which the engine cannot load — handing it one produced a tab
        // that sat there empty. The window opens on the homepage and the
        // internal page is put in front of it once there is a browser to put
        // it in.
        let internal = asked_for
            .as_ref()
            .filter(|url| url.scheme() == "zervo")
            .cloned();
        let start_url = asked_for
            .filter(|url| url.scheme() != "zervo")
            .or_else(|| Url::parse(&settings.homepage).ok())
            .unwrap_or_else(|| Url::parse("https://servo.org").expect("valid default URL"));

        let resolved_dark = theme::resolve(settings.theme, system_dark, settings.accent).dark;
        let state = Rc::new(AppState {
            servo,
            window_rendering_context,
            rendering_context,
            browser: RefCell::new(BrowserState::new(start_url.as_str())),
            needs_repaint: Cell::new(false),
            pending_popups: RefCell::new(Vec::new()),
            pending_closes: RefCell::new(Vec::new()),
            pending_keyboard_events: RefCell::new(HashMap::new()),
            dark_theme: Cell::new(resolved_dark),
            favicons_dirty: Cell::new(false),
            quit_requested: Cell::new(false),
            controls: RefCell::new(controls::Controls::default()),
            library: RefCell::new(library::Library::load()),
            media: RefCell::new(dashboard::Media::default()),
            vault: RefCell::new(passwords::Vault::load()),
            visible_input_method: Cell::new(None),
            content_origin: Cell::new((0.0, 0.0)),
            #[cfg(feature = "engine-downloads")]
            download_events: RefCell::new(Vec::new()),
            last_window_title: RefCell::new(String::new()),
            window,
        });

        // The (transparent) titlebar material must follow the app theme, or a
        // light app under a dark system keeps a dark band across the top.
        sync_window_theme(&state.window, settings.theme);
        #[cfg(target_os = "macos")]
        set_dock_icon(settings.app_icon);

        let initial_tab = state
            .browser
            .borrow()
            .active_tab
            .expect("initial tab exists");
        state.open_tab(initial_tab, start_url);

        let applied_theme = (
            settings.theme,
            settings.accent,
            system_dark,
            settings.translucency,
        );
        let applied_icon = settings.app_icon;
        // Whatever was cached last time, decoded on a thread so a launch never
        // waits on a photograph.
        let mut wallpaper = wallpaper::Wallpaper::default();
        wallpaper.restore();
        *self = Self::Running(RunningApp {
            state,
            egui_glow,
            settings,
            system_dark,
            favicons: HashMap::new(),
            settings_open: false,
            modifiers: ModifiersState::default(),
            cursor: PhysicalPosition::default(),
            webview_relative_mouse: Cell::new(Point2D::zero()),
            content_rect_points: egui::Rect::ZERO,
            controls_open: false,
            swipe: gestures::Recognizer::default(),
            pointer_owner: PointerOwner::Free,
            library_saved_at: std::time::Instant::now(),
            pending_repaint_at: None,
            repaint_at,
            applied_theme,
            applied_icon,
            theme_fade: None,
            downloads: downloads::DownloadManager::default(),
            wallpaper,
            page_backdrop: Arc::new(std::sync::Mutex::new(backdrop::PageBackdrop::default())),
            page_backdrop_texture: None,
            corner_eraser: Arc::new(Mutex::new(backdrop::Eraser::default())),
            wallpaper_texture: None,
            wallpaper_frost: None,
            #[cfg(target_os = "macos")]
            _vibrancy: vibrancy,
        });

        if let (Self::Running(app), Some(url)) = (&mut *self, internal) {
            app.apply_action(UiAction::Navigate(url.to_string()));
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        if let Self::Running(app) = self
            && matches!(cause, winit::event::StartCause::ResumeTimeReached { .. })
        {
            app.pending_repaint_at = None;
            app.state.window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Self::Running(app) = self {
            match app.pending_repaint_at {
                Some(deadline) => {
                    event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(deadline))
                },
                None => event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait),
            }
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: WakerEvent) {
        let Self::Running(app) = self else {
            return;
        };
        match event {
            WakerEvent::Engine => app.state.servo.spin_event_loop(),
            WakerEvent::Accessibility(event) => {
                use egui_winit::accesskit_winit::WindowEvent as AccessEvent;
                match event.window_event {
                    // The client has just attached and has no tree yet. egui
                    // only builds one as part of a pass, so the answer to both
                    // of these is to run one.
                    AccessEvent::InitialTreeRequested => app.state.window.request_redraw(),
                    AccessEvent::ActionRequested(request) => {
                        app.egui_glow
                            .egui_winit
                            .on_accesskit_action_request(request);
                        app.state.window.request_redraw();
                    },
                    AccessEvent::AccessibilityDeactivated => {},
                }
            },
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Self::Running(app) = self else {
            return;
        };
        app.handle_window_event(event_loop, event);
        app.state.servo.spin_event_loop();
        if app.state.quit_requested.get() {
            event_loop.exit();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Self::Running(app) = self {
            app.state.shutdown();
        }
    }
}

impl RunningApp {
    fn handle_window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        let state = self.state.clone();

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            },
            WindowEvent::Resized(new_size) => {
                state.window_rendering_context.resize(new_size);
                let _ = self.egui_glow.on_window_event(&state.window, &event);
                self.redraw();
                return;
            },
            WindowEvent::RedrawRequested => {
                self.redraw();
                return;
            },
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(webview) = state.active_webview() {
                    webview.set_hidpi_scale_factor(Scale::new(scale_factor as f32));
                }
                state.window.request_redraw();
                return;
            },
            WindowEvent::ThemeChanged(new_theme) => {
                self.system_dark = matches!(new_theme, winit::window::Theme::Dark);
                self.applied_theme = (
                    self.settings.theme,
                    self.settings.accent,
                    self.system_dark,
                    self.settings.translucency,
                );
                self.apply_theme();
                state.window.request_redraw();
                return;
            },
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                let _ = self.egui_glow.on_window_event(&state.window, &event);
                return;
            },
            _ => {},
        }

        // Route the remaining (input) events: egui first, unless the cursor is
        // over the web content, in which case pointer events bypass the chrome.
        let scale = state.window.scale_factor() as f32;
        let cursor_points = egui::pos2(self.cursor.x as f32 / scale, self.cursor.y as f32 / scale);
        // While the settings page covers the content area, egui owns it — as
        // it does the strip a revealed autohide sidebar floats over, or its
        // clicks and scrolls would reach the page underneath as well.
        // Anything egui has floating over the content owns the pointer: the
        // favourites card, the downloads card, the revealed sidebar, widget
        // menus, page dialogs. Asking egui which layer is under the pointer
        // covers all of them, including ones added later — the previous
        // version listed overlays by hand, and every overlay anyone forgot to
        // add sent its clicks straight through to the page underneath.
        let pointer_on_page = self.content_rect_points.contains(cursor_points)
            && !self.settings_open
            // A modal dialog blocks the whole page, not just its own rect.
            && !self.controls_open
            && !self.egui_glow.egui_ctx.is_pointer_over_egui();

        // Re-decided on every press rather than remembered indefinitely, so a
        // mouse-up we never see — AppKit swallows one whenever it runs its own
        // window-drag loop — cannot wedge either side on. Not cleared on
        // CursorLeft: leaving the window mid-drag is exactly when the owner
        // matters most.
        if matches!(
            &event,
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            }
        ) {
            self.pointer_owner = if pointer_on_page {
                PointerOwner::Page
            } else {
                PointerOwner::Chrome
            };
        }
        if matches!(&event, WindowEvent::Focused(false)) {
            self.pointer_owner = PointerOwner::Free;
        }
        if let WindowEvent::MouseWheel { delta, phase, .. } = &event
            && self.settings.gestures.enabled
        {
            let step = match delta {
                MouseScrollDelta::LineDelta(dx, dy) => egui::vec2(dx * 38.0, dy * 38.0),
                MouseScrollDelta::PixelDelta(pixels) => {
                    egui::vec2(pixels.x as f32, pixels.y as f32)
                },
            };
            // Fed in points, so the thresholds mean the same thing on a
            // Retina display as anywhere else.
            let scale = self.egui_glow.egui_ctx.pixels_per_point().max(0.1);
            if let Some(direction) =
                self.swipe
                    .feed(step / scale, *phase, std::time::Instant::now())
            {
                // Up and down are scrolling everywhere except the strip above
                // the page, which has nothing to scroll.
                let vertical_ok = cursor_points.y < self.content_rect_points.min.y;
                let horizontal = matches!(
                    direction,
                    gestures::Direction::Left | gestures::Direction::Right
                );
                if horizontal || vertical_ok {
                    self.apply_gesture(self.settings.gestures.action(direction));
                }
            }
        }

        let over_content = match self.pointer_owner {
            PointerOwner::Chrome => false,
            PointerOwner::Page => true,
            PointerOwner::Free => pointer_on_page,
        };
        if matches!(
            &event,
            WindowEvent::MouseInput {
                state: ElementState::Released,
                ..
            }
        ) {
            // Cleared after the routing decision, so the release itself still
            // goes wherever the press went.
            self.pointer_owner = PointerOwner::Free;
        }

        let mut consumed = false;
        match &event {
            WindowEvent::MouseWheel { .. } | WindowEvent::MouseInput { .. } if over_content => {
                // Clicks on the page take keyboard focus away from the chrome,
                // or the address bar would keep eating keystrokes.
                self.egui_glow.egui_ctx.memory_mut(|memory| {
                    if let Some(focused) = memory.focused() {
                        memory.surrender_focus(focused);
                    }
                });
            },
            WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(..)
                if !self
                    .egui_glow
                    .egui_ctx
                    .memory(|memory| memory.focused().is_some()) => {},
            event => {
                let response = self.egui_glow.on_window_event(&state.window, event);
                if response.repaint {
                    state.window.request_redraw();
                }
                // CursorMoved always goes to egui (tooltips), but is still
                // forwarded to the webview when over the content.
                consumed = if matches!(event, WindowEvent::CursorMoved { .. }) && over_content {
                    false
                } else {
                    response.consumed
                };
            },
        }
        if consumed {
            return;
        }

        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                let point = self.webview_point(position, scale);
                let previous = self.webview_relative_mouse.replace(point);
                if let Some(webview) = state.active_webview() {
                    let rect: Rect<f32, DevicePixel> = webview.size().into();
                    if rect.contains(point) {
                        webview.notify_input_event(InputEvent::MouseMove(MouseMoveEvent::new(
                            point.into(),
                        )));
                    } else if rect.contains(previous) {
                        webview.notify_input_event(InputEvent::MouseLeftViewport(
                            MouseLeftViewportEvent::default(),
                        ));
                    }
                }
            },
            WindowEvent::Ime(ime) => {
                use servo::{CompositionEvent, CompositionState, ImeEvent};
                use winit::event::Ime;

                if let Some(webview) = state.active_webview() {
                    let composition = |state, data| {
                        InputEvent::Ime(ImeEvent::Composition(CompositionEvent { state, data }))
                    };
                    match ime {
                        Ime::Enabled => {
                            webview.notify_input_event(composition(
                                CompositionState::Start,
                                String::new(),
                            ));
                        },
                        Ime::Preedit(text, _) => {
                            webview.notify_input_event(composition(CompositionState::Update, text));
                        },
                        Ime::Commit(text) => {
                            webview.notify_input_event(composition(CompositionState::End, text));
                        },
                        // Either the user dismissed the input method, or we
                        // withdrew it ourselves because focus moved. Only the
                        // first should reach the page: reporting the second
                        // blurs the element that has just been focused.
                        Ime::Disabled => {
                            if state.visible_input_method.take().is_some() {
                                webview.notify_input_event(InputEvent::Ime(ImeEvent::Dismissed));
                            }
                        },
                    }
                }
            },
            WindowEvent::CursorLeft { .. } => {
                if let Some(webview) = state.active_webview() {
                    webview.notify_input_event(InputEvent::MouseLeftViewport(
                        MouseLeftViewportEvent::default(),
                    ));
                }
            },
            WindowEvent::MouseInput {
                state: element_state,
                button,
                ..
            } => {
                let point = self.webview_relative_mouse.get();
                if let Some(webview) = state.active_webview() {
                    let rect: Rect<f32, DevicePixel> = webview.size().into();
                    if rect.contains(point) {
                        let mouse_button = match button {
                            MouseButton::Left => ServoMouseButton::Left,
                            MouseButton::Right => ServoMouseButton::Right,
                            MouseButton::Middle => ServoMouseButton::Middle,
                            MouseButton::Back => ServoMouseButton::Back,
                            MouseButton::Forward => ServoMouseButton::Forward,
                            MouseButton::Other(value) => ServoMouseButton::Other(value),
                        };
                        let action = match element_state {
                            ElementState::Pressed => MouseButtonAction::Down,
                            ElementState::Released => MouseButtonAction::Up,
                        };
                        webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
                            action,
                            mouse_button,
                            point.into(),
                        )));
                    }
                }
            },
            WindowEvent::MouseWheel { delta, .. } if over_content => {
                if let Some(webview) = state.active_webview() {
                    let (delta_x, delta_y) = match delta {
                        MouseScrollDelta::LineDelta(dx, dy) => {
                            ((dx * 38.0) as f64, (dy * 38.0) as f64)
                        },
                        MouseScrollDelta::PixelDelta(delta) => (delta.x, delta.y),
                    };
                    webview.notify_input_event(InputEvent::Wheel(WheelEvent::new(
                        WheelDelta {
                            x: delta_x,
                            y: delta_y,
                            z: 0.0,
                            mode: WheelMode::DeltaPixel,
                        },
                        self.webview_relative_mouse.get().into(),
                    )));
                }
            },
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                self.handle_keyboard_input(key_event);
            },
            _ => {},
        }
    }

    /// Window-space physical cursor position -> webview-relative DevicePixels.
    fn webview_point(
        &self,
        position: PhysicalPosition<f64>,
        scale: f32,
    ) -> Point2D<f32, DevicePixel> {
        Point2D::new(
            position.x as f32 - self.content_rect_points.min.x * scale,
            position.y as f32 - self.content_rect_points.min.y * scale,
        )
    }

    fn handle_keyboard_input(&mut self, winit_event: KeyEvent) {
        let keyboard_event = keyboard_event_from_winit(&winit_event, self.modifiers);
        if winit_event.state == ElementState::Pressed
            && self.handle_intercepted_key_bindings(&keyboard_event)
        {
            return;
        }
        let Some(webview) = self.state.active_webview() else {
            return;
        };
        let id = webview.notify_input_event(InputEvent::Keyboard(keyboard_event.clone()));
        self.state
            .pending_keyboard_events
            .borrow_mut()
            .insert(id, keyboard_event);
    }

    /// App shortcuts the page can never see or override.
    fn handle_intercepted_key_bindings(&mut self, key_event: &servo::KeyboardEvent) -> bool {
        let state = self.state.clone();
        let active_webview = state.active_webview();
        let mut handled = true;
        ShortcutMatcher::from_event(key_event.event.clone())
            .shortcut(CMD_OR_CONTROL, 'T', || {
                // Hoisted: the Ref guard must drop before apply_action
                // takes a mutable borrow of the same RefCell.
                let workspace = state.browser.borrow().active_workspace;
                self.apply_action(UiAction::NewTab { workspace });
            })
            .shortcut(CMD_OR_CONTROL, 'W', || {
                let active_tab = state.browser.borrow().active_tab;
                if let Some(tab_id) = active_tab {
                    self.apply_action(UiAction::CloseTab(tab_id));
                }
            })
            .shortcut(CMD_OR_CONTROL, 'Q', || {
                // Handled here rather than by AppKit: there is no menu bar to
                // carry the standard Quit item, and quitting has to go through
                // the event loop's exit so the engine is shut down properly and
                // gets to write its cookie jar out.
                state.quit_requested.set(true);
                state.window.request_redraw();
            })
            .shortcut(CMD_OR_CONTROL, ',', || {
                self.apply_action(UiAction::OpenSettings);
            })
            .shortcut(CMD_OR_CONTROL, 'L', || {
                state.browser.borrow_mut().focus_address = true;
                state.window.request_redraw();
            })
            .shortcut(CMD_OR_CONTROL, 'X', || {
                if let Some(webview) = &active_webview {
                    webview.notify_input_event(InputEvent::EditingAction(EditingActionEvent::Cut));
                }
            })
            .shortcut(CMD_OR_CONTROL, 'C', || {
                if let Some(webview) = &active_webview {
                    webview.notify_input_event(InputEvent::EditingAction(EditingActionEvent::Copy));
                }
            })
            .shortcut(CMD_OR_CONTROL, 'V', || {
                if let Some(webview) = &active_webview {
                    webview
                        .notify_input_event(InputEvent::EditingAction(EditingActionEvent::Paste));
                }
            })
            .shortcut(CMD_OR_ALT, Key::Named(NamedKey::ArrowLeft), || {
                if let Some(webview) = &active_webview
                    && webview.can_go_back()
                {
                    webview.go_back(1);
                }
            })
            .shortcut(CMD_OR_ALT, Key::Named(NamedKey::ArrowRight), || {
                if let Some(webview) = &active_webview
                    && webview.can_go_forward()
                {
                    webview.go_forward(1);
                }
            })
            .otherwise(|| handled = false);
        handled
    }

    fn redraw(&mut self) {
        let state = self.state.clone();
        let _ = state.rendering_context.make_current();

        // Adopt popups, drop script-closed tabs, refresh tab titles/urls.
        state.sync();
        state.needs_repaint.set(false);
        #[cfg(feature = "engine-downloads")]
        {
            // Downloads the engine handed us (Servo does the transfer; we store).
            //
            // Collected before the loop for the same reason as the queues in
            // `AppState::sync`: the body activates a tab, which re-enters the
            // engine, and `notify_response_chunk` pushes onto this very queue.
            // A `drain` iterator would still be holding the borrow.
            let events: Vec<app::DownloadEvent> =
                state.download_events.borrow_mut().drain(..).collect();
            for event in events {
                match event {
                    app::DownloadEvent::Offered {
                        request_id,
                        url,
                        default_filename,
                    } => {
                        self.downloads
                            .accept_from_engine(request_id, &url, &default_filename);
                        let tab_id = state.browser.borrow_mut().find_or_create_downloads_tab();
                        state.activate_tab(tab_id);
                    },
                    app::DownloadEvent::Chunk { request_id, chunk } => {
                        self.downloads.engine_chunk(request_id, &chunk);
                    },
                    app::DownloadEvent::Finished { request_id, ok } => {
                        self.downloads.engine_finished(request_id, ok);
                    },
                }
            }
        }

        if state.favicons_dirty.take() {
            self.refresh_favicons();
        }

        // The wallpaper: collect anything the fetch thread finished, and start
        // a new one when the cadence says so. Both are cheap when there is
        // nothing to do, which is almost always.
        // The page's blurred copy, taken by last frame's paint callback. On an
        // internal page there is nothing to copy, and a stale copy of the page
        // you were on before is worse than none.
        if let Ok(mut backdrop) = self.page_backdrop.lock()
            && let Some(image) = backdrop.take()
        {
            self.page_backdrop_texture = Some(theme::Frost::upload(
                &self.egui_glow.egui_ctx,
                "zervo-page-backdrop",
                image,
                egui::TextureOptions::LINEAR,
            ));
        }

        // Collected whatever the page is showing: a result left sitting in the
        // channel would keep the loop waking up for it forever.
        if self.wallpaper.poll()
            && let Some(image) = self.wallpaper.take_image()
        {
            // Mipmapped, because a two-thousand-pixel photograph drawn into a
            // nine-hundred-point window is a minification, and bilinear
            // minification of a photograph is what makes one shimmer.
            self.wallpaper_texture = Some(self.egui_glow.egui_ctx.load_texture(
                "zervo-wallpaper",
                image.sharp,
                egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear)),
            ));
            // The frost is the other way round: a small picture magnified,
            // where plain bilinear filtering is exactly what is wanted, since
            // smoothing between its pixels is more blur.
            self.wallpaper_frost = Some(theme::Frost::upload(
                &self.egui_glow.egui_ctx,
                "zervo-wallpaper-frost",
                image.frost,
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.settings.new_tab_background == settings::NewTabBackground::Photo {
            if self.wallpaper.due(
                &self.settings.wallpaper_source,
                self.settings.wallpaper_cadence,
            ) {
                self.wallpaper.fetch(&self.settings.wallpaper_source);
            }
        }

        let palette = self.palette();
        if self.theme_fade.is_some() {
            // egui's own style has to follow the crossfade frame by frame:
            // stock widgets take their colors from there rather than from the
            // palette we hand to `draw`.
            theme::apply(&self.egui_glow.egui_ctx, &palette);
            let done = self
                .theme_fade
                .as_ref()
                .is_some_and(|(_, started)| started.elapsed() >= THEME_FADE);
            if done {
                self.theme_fade = None;
            } else {
                state.window.request_redraw();
            }
        }
        let active_webview = state.active_webview();
        let offscreen = state.rendering_context.clone();
        let mut ui_output = None;

        // Move UI-facing state out of `self` for the closure; restored below.
        let mut settings = self.settings.clone();
        let favicons = std::mem::take(&mut self.favicons);
        // Handed to the paint callback, which runs inside `run` and so cannot
        // borrow `self`. Only asked for when it is due, so an idle window is
        // not stalling its own pipeline on a readback every frame.
        // Nothing shows through a solid surface, so nothing needs copying:
        // under Solid this whole path — the readback, the blur, the upload —
        // costs nothing at all rather than costing a little.
        let frosting = palette.translucency == theme::Translucency::Frosted;
        // Handed to the paint callback, which runs inside `run` and so cannot
        // borrow `self`.
        let eraser = self.corner_eraser.clone();
        let capture = (frosting
            && self
                .page_backdrop
                .lock()
                .is_ok_and(|backdrop| backdrop.due()))
        .then(|| self.page_backdrop.clone());
        // Every glass surface inside the content rect frosts against the page,
        // the same way the new tab page's cards frost against its wallpaper —
        // the new tab page then replaces it with the wallpaper's own, since
        // that is what is behind *its* cards.
        let palette = match (&self.page_backdrop_texture, !frosting) {
            (Some(frost), false) => palette.with_backdrop(Some(theme::Backdrop {
                texture: frost.id(),
                luma: frost.luma(),
                rect: self.content_rect_points,
                reach: 0.0,
                uv: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                alpha: 1.0,
            })),
            _ => palette,
        };

        let wallpaper = wallpaper::View {
            texture: self.wallpaper_texture.as_ref(),
            frost: self.wallpaper_frost.as_ref(),
            credit: self.wallpaper.credit(),
            error: self.wallpaper.error.as_deref(),
            loading: self.wallpaper.is_loading(),
        };

        self.egui_glow.run(&state.window, |root| {
            let mut browser = state.browser.borrow_mut();
            let mut controls = state.controls.borrow_mut();
            let mut library = state.library.borrow_mut();
            let mut vault = state.vault.borrow_mut();
            let media = state.media.borrow().clone();
            let mut chrome = ui::ChromeContext {
                browser: &mut browser,
                controls: &mut controls,
                library: &mut library,
                vault: &mut vault,
                media: &media,
                settings: &mut settings,
                palette,
                favicons: &favicons,
                downloads: &self.downloads,
                wallpaper,
                capture: capture.clone(),
            };
            let output = ui::draw(root, &mut chrome);
            drop(browser);
            drop(controls);
            drop(library);
            drop(vault);

            let content_rect = output.content_rect;
            let scale = root.pixels_per_point();
            // Whether the page arrived as a blit, and so had its corners cut
            // out of the framebuffer. The masks that round them off are drawn
            // at the chrome's own tint now, which is right over the
            // transparency the cut leaves and wrong over anything else — an
            // internal page draws itself with rounded corners already, so
            // masking it would only lay a second tint over its own.
            let mut blitted = false;
            if !output.settings_open {
                if let Some(webview) = &active_webview {
                    let size = Size2D::new(
                        content_rect.width().max(1.0),
                        content_rect.height().max(1.0),
                    ) * Scale::<f32, DeviceIndependentPixel, DevicePixel>::new(scale);
                    if size != webview.size() {
                        // Also resizes the offscreen context, which must stay
                        // sized to exactly the content viewport.
                        webview.resize(PhysicalSize::new(size.width as u32, size.height as u32));
                    }
                    // Render Servo into the offscreen FBO before egui paints.
                    webview.paint();

                    // Blit the page under all chrome widgets. Only when a live
                    // webview exists — otherwise the FBO holds a stale frame.
                    if let Some(render_to_parent) = offscreen.render_to_parent_callback() {
                        root.layer_painter(LayerId::background())
                            .add(egui::PaintCallback {
                                rect: content_rect,
                                callback: Arc::new(CallbackFn::new(move |info, painter| {
                                    let clip = info.viewport_in_pixels();
                                    let rect_in_parent = euclid::default::Rect::new(
                                        euclid::default::Point2D::new(
                                            clip.left_px,
                                            clip.from_bottom_px,
                                        ),
                                        euclid::default::Size2D::new(clip.width_px, clip.height_px),
                                    );
                                    render_to_parent(painter.gl(), rect_in_parent);
                                })),
                            });

                        // And immediately after it, while the page is the only
                        // thing on the framebuffer, take the blurred copy the
                        // chrome frosts itself against. Ordered here on
                        // purpose: a frame later and it would contain the
                        // cards, which would then be frosting against
                        // themselves.
                        // And the corners come out of it, so the chrome can
                        // be drawn back over transparency rather than over the
                        // page. Ordered after the copy so the copy is of the
                        // page as the engine drew it.
                        if let Some(capture) = &capture {
                            backdrop::capture_into(
                                &root.layer_painter(LayerId::background()),
                                content_rect,
                                capture,
                            );
                        }
                        backdrop::cut_corners_into(
                            &root.layer_painter(LayerId::background()),
                            content_rect,
                            theme::CONTENT_RADIUS,
                            &eraser,
                        );
                        blitted = true;
                    }
                }
            }
            // Rounded-corner masks and border, drawn over the blit.
            ui::finish_content_frame(
                root,
                content_rect,
                &palette,
                blitted,
                settings.top_glow,
                settings.content_border,
                settings
                    .content_shadow
                    .then_some(settings.content_shadow_amount),
                settings
                    .content_halo
                    .then_some((settings.content_halo_tint, settings.content_halo_amount)),
            );

            ui_output = Some(output);
        });

        self.favicons = favicons;
        self.settings = settings;

        let mut ambient = false;
        if let Some(output) = ui_output {
            self.content_rect_points = output.content_rect;
            state
                .content_origin
                .set((output.content_rect.min.x, output.content_rect.min.y));
            self.controls_open = output.controls_open;
            self.settings_open = output.settings_open;
            ambient = output.ambient;
            for action in output.actions {
                self.apply_action(action);
            }
        }
        // Ambient animations (aurora new-tab page) tick at ~30fps via timed
        // wakes rather than a max-FPS redraw loop.
        // Nothing else will wake the loop when a fetch finishes on its own
        // thread, so while one is running the page checks back.
        if self.wallpaper.is_loading() {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
            self.pending_repaint_at = Some(
                self.pending_repaint_at
                    .map_or(deadline, |at| at.min(deadline)),
            );
        }
        if ambient {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(33);
            self.pending_repaint_at = Some(
                self.pending_repaint_at
                    .map_or(deadline, |d| d.min(deadline)),
            );
        }
        if state.library.borrow().needs_save()
            && self.library_saved_at.elapsed() > std::time::Duration::from_secs(10)
        {
            state.library.borrow_mut().save();
            self.library_saved_at = std::time::Instant::now();
        }

        // Whatever egui asked for during this pass, taken whichever branch runs
        // so a stale deadline cannot survive into the next one.
        let egui_deadline = self.repaint_at.take();
        if self.egui_glow.egui_ctx.requested_repaint_last_pass() {
            state.window.request_redraw();
        } else if let Some(deadline) = egui_deadline {
            // Delayed request (caret blink, a clock's next minute): wake on a
            // timer instead of redrawing at max FPS until it fires.
            //
            // Two things used to be wrong here. It woke after a flat 300ms
            // regardless of what was asked for, and `has_requested_repaint()` is
            // true whenever *any* delay is pending — so the stock new tab page,
            // which ships a clock asking for twenty seconds, re-armed 300ms
            // forever and never idled. And it *assigned* rather than taking the
            // earlier of the two, so the 33ms the ambient backdrop had just
            // asked for two branches up was overwritten by the clock's, and the
            // default animated backdrop ran at three frames a second.
            self.pending_repaint_at = Some(
                self.pending_repaint_at
                    .map_or(deadline, |at| at.min(deadline)),
            );
        }

        // Paint order: bind the window framebuffer, egui paint (runs the blit
        // callback first, then draws chrome over it), swap.
        let _ = state.rendering_context.make_current();
        state.window_rendering_context.prepare_for_rendering();
        // Nothing in surfman or Servo clears the window framebuffer, so clear
        // it to fully transparent ourselves: with a non-opaque layer, any
        // pixel the chrome does not cover shows what is behind the window.
        {
            use glow::HasContext as _;
            let gl = state.window_rendering_context.glow_gl_api();
            unsafe {
                gl.clear_color(0.0, 0.0, 0.0, 0.0);
                gl.clear(glow::COLOR_BUFFER_BIT);
            }
        }
        self.egui_glow.paint(&state.window);
        state.window_rendering_context.present();
    }

    fn apply_action(&mut self, action: UiAction) {
        let state = self.state.clone();
        match action {
            UiAction::Navigate(address) => {
                let Ok(url) = Url::parse(&address) else {
                    return;
                };
                // zervo:// URLs are internal pages, never engine loads. All
                // four of them: every one is shown in the address bar, so
                // every one has to be something you can type back into it.
                // History and the new tab page both used to open Settings.
                if url.scheme() == "zervo" {
                    let page = url.as_str();
                    if page.contains("downloads") {
                        self.apply_action(UiAction::OpenDownloads);
                    } else if page.contains("history") {
                        self.apply_action(UiAction::OpenHistory);
                    } else if page.contains("newtab") {
                        let workspace = state.browser.borrow().active_workspace;
                        self.apply_action(UiAction::NewTab { workspace });
                    } else {
                        self.apply_action(UiAction::OpenSettings);
                    }
                    return;
                }
                // No URL heuristic needed any more: the engine offers us any
                // response it cannot render, with the real Content-Disposition
                // filename, via notify_unsupported_response.
                // Navigating from the settings tab opens a new web tab; the
                // zervo://newtab page navigates in place (open_tab converts it
                // to a web tab below).
                let active_is_settings = state
                    .browser
                    .borrow()
                    .active_tab()
                    .is_some_and(|tab| tab.kind == TabKind::Settings);
                if active_is_settings {
                    let workspace = state.browser.borrow().active_workspace;
                    let tab_id = state.browser.borrow_mut().add_tab(workspace, url.as_str());
                    state.open_tab(tab_id, url);
                    return;
                }
                if let Some(webview) = state.active_webview() {
                    webview.load(url);
                    return;
                }
                let active_tab = state.browser.borrow().active_tab;
                if let Some(tab_id) = active_tab {
                    state.open_tab(tab_id, url);
                }
            },
            UiAction::Back => {
                if let Some(webview) = state.active_webview()
                    && webview.can_go_back()
                {
                    webview.go_back(1);
                }
            },
            UiAction::Forward => {
                if let Some(webview) = state.active_webview()
                    && webview.can_go_forward()
                {
                    webview.go_forward(1);
                }
            },
            UiAction::Reload => {
                if let Some(webview) = state.active_webview() {
                    webview.reload();
                }
            },
            UiAction::SelectTab(tab_id) => {
                let webview_missing_url = {
                    let browser = state.browser.borrow();
                    browser
                        .tab(tab_id)
                        .filter(|tab| tab.kind == TabKind::Web && tab.webview.is_none())
                        .map(|tab| tab.url.clone())
                };
                match webview_missing_url {
                    Some(url) => {
                        let url = ui::normalize_url(&url, self.settings.search_engine);
                        if let Ok(url) = Url::parse(&url) {
                            state.open_tab(tab_id, url);
                        }
                    },
                    None => state.activate_tab(tab_id),
                }
            },
            UiAction::NewTab { workspace } => match self.settings.new_tab_page {
                NewTabPage::ZervoPage => {
                    let tab_id = state.browser.borrow_mut().add_zervo_page(workspace);
                    state.activate_tab(tab_id);
                },
                NewTabPage::Homepage => {
                    let page = self.settings.homepage.clone();
                    let tab_id = state.browser.borrow_mut().add_tab(workspace, page.as_str());
                    if let Ok(url) = Url::parse(&page) {
                        state.open_tab(tab_id, url);
                    }
                },
            },
            UiAction::ToggleSidebar => {
                let collapsed = state.browser.borrow().sidebar_collapsed;
                state.browser.borrow_mut().sidebar_collapsed = !collapsed;
                state.window.request_redraw();
            },
            UiAction::SavePassword => {
                let (site, username, password) = {
                    let browser = state.browser.borrow();
                    browser.password_draft.clone()
                };
                let outcome = state.vault.borrow_mut().set(&site, &username, &password);
                let mut browser = state.browser.borrow_mut();
                browser.password_notice = match outcome {
                    Ok(()) => {
                        browser.password_draft = Default::default();
                        format!("Saved the login for {site}.")
                    },
                    Err(why) => why,
                };
                state.window.request_redraw();
            },
            UiAction::RemovePassword(site, username) => {
                state.vault.borrow_mut().remove(&site, &username);
                state.window.request_redraw();
            },
            UiAction::ExportPasswords => {
                let path = dirs::download_dir()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("zervo-passwords.json");
                let outcome = state.vault.borrow().export(&path);
                state.browser.borrow_mut().password_notice = match outcome {
                    Ok(count) => format!(
                        "Exported {count} login(s) to {} — the file is not encrypted.",
                        path.display()
                    ),
                    Err(why) => why,
                };
                state.window.request_redraw();
            },
            UiAction::ImportPasswords => {
                let picked = state.pick_file();
                let notice = match picked {
                    Some(path) => match state.vault.borrow_mut().import(&path) {
                        Ok(count) => format!("Imported {count} login(s)."),
                        Err(why) => why,
                    },
                    None => String::new(),
                };
                if !notice.is_empty() {
                    state.browser.borrow_mut().password_notice = notice;
                }
                state.window.request_redraw();
            },
            UiAction::MoveNavItem { item, side, index } => {
                let settings = &mut self.settings;
                settings.navbar_left.retain(|placed| *placed != item);
                settings.navbar_right.retain(|placed| *placed != item);
                let list = match side {
                    ui::NavSide::Left => &mut settings.navbar_left,
                    ui::NavSide::Right => &mut settings.navbar_right,
                };
                let at = index.min(list.len());
                list.insert(at, item);
                settings.save();
                state.window.request_redraw();
            },
            UiAction::RemoveNavItem(item) => {
                self.settings.navbar_left.retain(|placed| *placed != item);
                self.settings.navbar_right.retain(|placed| *placed != item);
                self.settings.save();
                state.window.request_redraw();
            },
            UiAction::AddNavItem(item) => {
                if !self.settings.navbar_left.contains(&item)
                    && !self.settings.navbar_right.contains(&item)
                {
                    self.settings.navbar_right.push(item);
                    self.settings.save();
                }
                state.window.request_redraw();
            },
            UiAction::AddWidget(kind) => {
                let mut widget = dashboard::Placed::new(kind);
                (widget.col, widget.row) =
                    dashboard::free_cell(&self.settings.navbar_widgets, widget.size);
                self.settings.navbar_widgets.push(widget);
                self.settings.save();
                state.window.request_redraw();
            },
            UiAction::RemoveWidget(index) => {
                if index < self.settings.navbar_widgets.len() {
                    self.settings.navbar_widgets.remove(index);
                    self.settings.save();
                }
                state.window.request_redraw();
            },
            UiAction::SwapWidgets(a, b) => {
                let widgets = &mut self.settings.navbar_widgets;
                if a < widgets.len() && b < widgets.len() && a != b {
                    widgets.swap(a, b);
                    self.settings.save();
                }
                state.window.request_redraw();
            },
            UiAction::PlaceWidget { index, col, row } => {
                if let Some(widget) = self.settings.navbar_widgets.get_mut(index) {
                    widget.col = col;
                    widget.row = row;
                    self.settings.save();
                }
                state.window.request_redraw();
            },
            UiAction::ResizeWidget(index, size) => {
                if let Some(widget) = self.settings.navbar_widgets.get_mut(index) {
                    widget.size = size;
                    self.settings.save();
                }
                state.window.request_redraw();
            },
            UiAction::MediaAction(action) => {
                if let Some(webview) = state.active_webview() {
                    webview.notify_media_session_action_event(action);
                }
            },
            UiAction::OpenHistory => {
                let tab_id = state.browser.borrow_mut().find_or_create_history_tab();
                state.activate_tab(tab_id);
            },
            UiAction::ToggleFavourite => {
                let (url, title) = state
                    .browser
                    .borrow()
                    .active_tab()
                    .map(|tab| (tab.url.clone(), tab.title.clone()))
                    .unwrap_or_default();
                if !url.is_empty() {
                    state.library.borrow_mut().toggle_favourite(&url, &title);
                }
                state.window.request_redraw();
            },
            UiAction::RenameFavourite(url, title) => {
                state.library.borrow_mut().rename_favourite(&url, &title);
                state.browser.borrow_mut().favourite_edit = None;
                state.window.request_redraw();
            },
            UiAction::RemoveFavourite(url) => {
                state.library.borrow_mut().remove_favourite(&url);
                state.window.request_redraw();
            },
            UiAction::ForgetVisit(index) => {
                state.library.borrow_mut().forget(index);
                state.window.request_redraw();
            },
            UiAction::ClearHistory => {
                state.library.borrow_mut().clear_history();
                state.window.request_redraw();
            },
            UiAction::OpenDownloads => {
                let tab_id = state.browser.borrow_mut().find_or_create_downloads_tab();
                state.activate_tab(tab_id);
            },
            UiAction::CancelDownload(id) => {
                self.downloads.cancel(id);
                state.window.request_redraw();
            },
            UiAction::RemoveDownload(id) => {
                self.downloads.remove(id);
                state.window.request_redraw();
            },
            UiAction::RevealDownload(id) => {
                if let Some(item) = self.downloads.items.iter().find(|item| item.id == id) {
                    downloads::reveal(&item.path);
                }
            },
            UiAction::OpenDownload(id) => {
                if let Some(item) = self.downloads.items.iter().find(|item| item.id == id) {
                    downloads::open_file(&item.path);
                }
            },
            UiAction::ShuffleWallpaper => {
                // Asking for a picture is also asking to see one, so the page
                // switches to photographs if it was not already showing them.
                self.settings.new_tab_background = settings::NewTabBackground::Photo;
                self.settings.save();
                self.wallpaper.fetch(&self.settings.wallpaper_source);
                state.window.request_redraw();
            },
            UiAction::PickWallpaper => {
                if let Some(path) = state.pick_file() {
                    self.settings.wallpaper_source =
                        wallpaper::Source::File(path.to_string_lossy().into_owned());
                    self.settings.new_tab_background = settings::NewTabBackground::Photo;
                    self.settings.save();
                    self.wallpaper.fetch(&self.settings.wallpaper_source);
                }
                state.window.request_redraw();
            },
            UiAction::ResetLayout => {
                let settings = &mut self.settings;
                settings.navbar_left = ui::NavItem::default_left();
                settings.navbar_right = ui::NavItem::default_right();
                settings.navbar_widgets = dashboard::Placed::defaults();
                settings.newtab_tiles = newtab::Tile::defaults();
                settings.newtab_world_clocks = newtab::Zone::defaults();
                settings.navbar_height = ui::NAVBAR_DEFAULT_HEIGHT;
                settings.address_pill_width = ui::ADDRESS_PILL_DEFAULT_WIDTH;
                settings.sidebar_width = ui::SIDEBAR_DEFAULT_WIDTH;
                settings.save();
                // egui remembers the panel's width itself, and would otherwise
                // keep the old one until the next launch.
                self.egui_glow.egui_ctx.data_mut(|data| {
                    data.remove::<egui::PanelState>(egui::Id::new(ui::SIDEBAR_ID))
                });
                state.window.request_redraw();
            },
            UiAction::RestartDownload(id) => {
                // Servo has no "fetch this again" call, so the way to start a
                // download over is to ask for the URL again — which is how it
                // started in the first place.
                let url = self
                    .downloads
                    .items
                    .iter()
                    .find(|item| item.id == id)
                    .map(|item| item.url.clone());
                self.downloads.remove(id);
                if let Some(url) = url {
                    self.apply_action(UiAction::Navigate(url));
                }
                state.window.request_redraw();
            },
            UiAction::ClearDownloads => {
                self.downloads.clear_finished();
                state.window.request_redraw();
            },
            UiAction::OpenSettings => {
                let tab_id = state.browser.borrow_mut().find_or_create_settings_tab();
                state.activate_tab(tab_id);
            },
            UiAction::CloseTab(tab_id) => state.close_tab(tab_id),
            UiAction::TogglePin(tab_id) => {
                if let Some(tab) = state.browser.borrow_mut().tab_mut(tab_id) {
                    tab.pinned = !tab.pinned;
                }
                state.window.request_redraw();
            },
            UiAction::MoveTab {
                tab,
                workspace,
                index,
            } => {
                let moved = {
                    let mut browser = state.browser.borrow_mut();
                    let moved = browser.move_tab(tab, workspace, index);
                    if moved {
                        browser.active_workspace = workspace;
                    }
                    moved
                };
                if moved {
                    // Through SelectTab, not by assigning active_tab: that
                    // field is the model's opinion, and the engine only shows,
                    // focuses and unthrottles a webview when activate_tab says
                    // so. Setting it directly leaves the page you were on
                    // displayed and focused while the sidebar claims otherwise.
                    // Following the tab is what the gesture means — dropping it
                    // somewhere you are not looking reads as having lost it.
                    self.apply_action(ui::UiAction::SelectTab(tab));
                }
                state.window.request_redraw();
            },
            UiAction::GroupTabs { onto, dragged } => {
                {
                    let mut browser = state.browser.borrow_mut();
                    let name = Self::group_name(&browser, onto, dragged);
                    let index = browser.group_tabs(name, onto, dragged);
                    browser.active_workspace = index;
                    // Opened for editing straight away: the group is the answer
                    // to a question the user has not been asked yet.
                    browser.workspace_edit = Some((index, browser.workspaces[index].name.clone()));
                }
                // See MoveTab — the engine has to be told, not just the model.
                self.apply_action(ui::UiAction::SelectTab(dragged));
                state.window.request_redraw();
            },
            UiAction::RenameWorkspace(index, name) => {
                let mut browser = state.browser.borrow_mut();
                if let Some(workspace) = browser.workspaces.get_mut(index) {
                    let name = name.trim();
                    if !name.is_empty() {
                        workspace.name = name.to_owned();
                    }
                }
                drop(browser);
                state.window.request_redraw();
            },
            UiAction::NewWorkspace => {
                let mut browser = state.browser.borrow_mut();
                let name = format!("Workspace {}", browser.workspaces.len() + 1);
                let index = browser.add_workspace(name);
                browser.active_workspace = index;
            },
            UiAction::SelectWorkspace(index) => {
                state.browser.borrow_mut().active_workspace = index;
            },
            UiAction::DragWindow => {
                let _ = state.window.drag_window();
            },
            UiAction::PersistSettings => {
                self.settings.save();
            },
            UiAction::SettingsChanged => {
                self.settings.save();
                // Only redo the parts whose input actually changed — see the
                // note on `applied_theme`.
                let look = (
                    self.settings.theme,
                    self.settings.accent,
                    self.system_dark,
                    self.settings.translucency,
                );
                if look != self.applied_theme {
                    self.applied_theme = look;
                    self.start_theme_fade();
                }
                #[cfg(target_os = "macos")]
                if self.settings.app_icon != self.applied_icon {
                    self.applied_icon = self.settings.app_icon;
                    set_dock_icon(self.settings.app_icon);
                }
                state.window.request_redraw();
            },
        }
    }

    /// The palette to paint with — the target one, or a point on the way to it
    /// while a theme change is crossing over.
    /// Run a bound swipe. Dispatched from the event handler rather than from a
    /// frame, so unlike every other caller of these actions it has to ask for
    /// the repaint itself.
    fn apply_gesture(&mut self, action: gestures::GestureAction) {
        use gestures::GestureAction as Action;
        let state = self.state.clone();
        match action {
            Action::None => return,
            Action::Back => self.apply_action(ui::UiAction::Back),
            Action::Forward => self.apply_action(ui::UiAction::Forward),
            Action::ToggleSidebar => self.apply_action(ui::UiAction::ToggleSidebar),
            Action::NewTab => {
                let workspace = state.browser.borrow().active_workspace;
                self.apply_action(ui::UiAction::NewTab { workspace });
            },
            Action::NextWorkspace | Action::PreviousWorkspace => {
                let mut browser = state.browser.borrow_mut();
                let count = browser.workspaces.len();
                if count > 1 {
                    let step = if matches!(action, Action::NextWorkspace) {
                        1
                    } else {
                        count - 1
                    };
                    browser.active_workspace = (browser.active_workspace + step) % count;
                }
            },
            Action::ToggleShelf => {
                let uncovered = ui::shelf_uncovered_height(&self.settings.navbar_widgets);
                self.settings.navbar_height =
                    if self.settings.navbar_height > ui::NAVBAR_DEFAULT_HEIGHT + 10.0 {
                        ui::NAVBAR_DEFAULT_HEIGHT
                    } else {
                        uncovered
                    };
                self.settings.save();
            },
        }
        state.window.request_redraw();
    }

    /// A provisional name for a group made from two tabs.
    ///
    /// Their shared host, when they have one — grouping two pages from the
    /// same site is the common case and "github.com" is a better guess than
    /// anything generic. Internal pages are skipped rather than parsed: the
    /// authority of `zervo://newtab` is `newtab`, so two new tabs would
    /// otherwise produce a workspace called "newtab". Otherwise the numbered
    /// default, which at least says what to replace.
    fn group_name(browser: &state::BrowserState, onto: TabId, dragged: TabId) -> String {
        let host = |id: TabId| {
            browser
                .tab(id)
                .filter(|tab| tab.url.starts_with("https://") || tab.url.starts_with("http://"))
                .and_then(|tab| tab.url.split("://").nth(1))
                .and_then(|rest| rest.split('/').next())
                .map(|host| host.trim_start_matches("www.").to_owned())
                .filter(|host| !host.is_empty())
        };
        match (host(onto), host(dragged)) {
            (Some(a), Some(b)) if a == b => a,
            _ => format!("Workspace {}", browser.workspaces.len() + 1),
        }
    }

    fn palette(&self) -> Palette {
        let target = self.target_palette();
        let Some((from, started)) = &self.theme_fade else {
            return target;
        };
        let t = started.elapsed().as_secs_f32() / THEME_FADE.as_secs_f32();
        if t >= 1.0 {
            target
        } else {
            theme::lerp(from, &target, glass::ease_out(t))
        }
    }

    fn target_palette(&self) -> Palette {
        theme::resolve(self.settings.theme, self.system_dark, self.settings.accent)
            .with_translucency(self.settings.translucency)
    }

    /// Begin crossing to whatever the settings now say, from wherever the
    /// chrome currently is — so changing the theme twice in quick succession
    /// carries on from the middle rather than snapping back.
    fn start_theme_fade(&mut self) {
        let from = self.palette();
        self.theme_fade = Some((from, std::time::Instant::now()));
        self.apply_platform_theme();
        self.state.window.request_redraw();
    }

    /// Re-style the chrome and tell every webview the new prefers-color-scheme.
    fn apply_theme(&mut self) {
        theme::apply(&self.egui_glow.egui_ctx, &self.palette());
        self.apply_platform_theme();
    }

    /// The half of a theme change that is not egui's: the engine, the window's
    /// appearance and the frosted backdrop. Aimed at the theme being crossed
    /// *to*, and started at the same moment as the crossfade, so the frost has
    /// the whole interval to get there.
    fn apply_platform_theme(&mut self) {
        let target = self.target_palette();
        self.state.set_engine_theme(target.dark);
        sync_window_theme(&self.state.window, self.settings.theme);
        // Every material here is appearance-adaptive on its own; what differs
        // is how much of the desktop survives it. `UnderWindowBackground` is
        // the clearest of them — it is meant for what shows *through* a window
        // — and it is the one that lets the colours behind come through rather
        // than a grey suggestion of them.
        #[cfg(target_os = "macos")]
        if let Some(vibrancy) = &self._vibrancy {
            use objc2_app_kit::NSVisualEffectMaterial;
            let backdrop = target.translucency.backdrop();
            let material = match backdrop {
                theme::SystemBackdrop::Opaque => NSVisualEffectMaterial::WindowBackground,
                _ => NSVisualEffectMaterial::UnderWindowBackground,
            };
            vibrancy.set_material_animated(material, THEME_FADE.as_secs_f64());
        }
    }

    /// Rebuild favicon textures from the engine's cached favicon images.
    fn refresh_favicons(&mut self) {
        let state = self.state.clone();
        let browser = state.browser.borrow();
        for workspace in &browser.workspaces {
            for tab in &workspace.tabs {
                let Some(webview) = &tab.webview else {
                    continue;
                };
                let Some(favicon) = webview.favicon() else {
                    continue;
                };
                let (width, height) = (favicon.width as usize, favicon.height as usize);
                let bytes = favicon.data();
                if width == 0 || height == 0 || bytes.len() < width * height * 4 {
                    continue;
                }
                let rgba: Vec<u8> = match favicon.format {
                    servo::PixelFormat::RGBA8 => bytes[..width * height * 4].to_vec(),
                    servo::PixelFormat::BGRA8 => bytes[..width * height * 4]
                        .chunks_exact(4)
                        .flat_map(|px| [px[2], px[1], px[0], px[3]])
                        .collect(),
                    _ => continue,
                };
                drop(favicon);
                let image = egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba);
                let texture = self.egui_glow.egui_ctx.load_texture(
                    format!("favicon-{}", tab.id),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                self.favicons.insert(tab.id, texture);
            }
        }
    }
}

/// Rasterize the Zervo icon and set it as the Dock icon at runtime. (An .app
/// bundle will carry the real Icon Composer icon; this covers dev runs.)
#[cfg(target_os = "macos")]
fn set_dock_icon(icon: settings::AppIcon) {
    // Both variants are composed from the layers of the Icon Composer
    // document in assets/icon/Zervo.icon, so the Dock icon and the bundled
    // .icns always come from the same artwork.
    const DEFAULT_SVG: &str = include_str!("../assets/icon/zervo-icon-default.svg");
    const TRANSPARENT_SVG: &str = include_str!("../assets/icon/zervo-icon-transparent.svg");
    let icon_svg = match icon {
        settings::AppIcon::Default => DEFAULT_SVG,
        settings::AppIcon::Transparent => TRANSPARENT_SVG,
    };
    let options = resvg::usvg::Options::default();
    let Ok(tree) = resvg::usvg::Tree::from_str(icon_svg, &options) else {
        return;
    };
    let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(512, 512) else {
        return;
    };
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(0.5, 0.5),
        &mut pixmap.as_mut(),
    );
    let Ok(png) = pixmap.encode_png() else {
        return;
    };

    use objc2::AnyThread;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(&png);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // Safety: called on the main thread (MainThreadMarker above) with a
    // valid NSImage; matches AppKit's documented usage.
    unsafe {
        app.setApplicationIconImage(Some(&image));
    }
}

/// Keep the OS window appearance (titlebar material, traffic lights) in sync
/// with the app theme. Auto releases the override so the window follows the
/// system again (and keeps delivering ThemeChanged events).
fn sync_window_theme(window: &Window, mode: theme::ThemeMode) {
    let override_theme = match mode {
        theme::ThemeMode::Auto => None,
        theme::ThemeMode::Light => Some(winit::window::Theme::Light),
        theme::ThemeMode::Dark => Some(winit::window::Theme::Dark),
    };
    window.set_theme(override_theme);
}

/// Use the macOS system font (SF Pro) for all proportional text, falling back
/// to egui's bundled fonts when unavailable.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Phosphor icon set, vendored — registered as its own family so icons are
    // drawn as text (crisp at any DPI) rather than hand-traced paths.
    fonts.font_data.insert(
        "phosphor".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/Phosphor.ttf"
        ))),
    );
    fonts.families.insert(
        egui::FontFamily::Name(icons::PHOSPHOR_FAMILY.into()),
        vec!["phosphor".to_owned()],
    );
    // Also as a fallback on the text families, so an icon glyph inline in a
    // label still resolves.
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("phosphor".to_owned());
    }

    #[cfg(target_os = "macos")]
    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/SFNS.ttf") {
        fonts.font_data.insert(
            "SF Pro".to_owned(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(proportional) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            proportional.insert(0, "SF Pro".to_owned());
        }
    }

    ctx.set_fonts(fonts);
}

impl Drop for RunningApp {
    fn drop(&mut self) {
        let _ = self.state.rendering_context.make_current();
        self.egui_glow.destroy();
    }
}

#[derive(Clone)]
struct Waker(winit::event_loop::EventLoopProxy<WakerEvent>);

/// Anything that can wake the loop from outside a window event.
#[derive(Debug)]
enum WakerEvent {
    /// The engine has something to do; spin its event loop.
    Engine,
    /// The platform's accessibility client is asking for the tree, or asking
    /// for something in it to be activated. `accesskit_winit` delivers these
    /// through the same proxy, which is why this is an enum now.
    Accessibility(egui_winit::accesskit_winit::Event),
}

impl From<egui_winit::accesskit_winit::Event> for WakerEvent {
    fn from(event: egui_winit::accesskit_winit::Event) -> Self {
        Self::Accessibility(event)
    }
}

impl Waker {
    fn new(event_loop: &EventLoop<WakerEvent>) -> Self {
        Self(event_loop.create_proxy())
    }
}

impl EventLoopWaker for Waker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(Self(self.0.clone()))
    }

    fn wake(&self) {
        if let Err(error) = self.0.send_event(WakerEvent::Engine) {
            log::warn!("Failed to wake event loop: {error:?}");
        }
    }
}
