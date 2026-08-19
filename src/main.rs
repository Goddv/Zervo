//! Zervo: winit + Servo + egui chrome.
//!
//! Architecture (mirrors servoshell): one surfman GL context renders to the
//! window; Servo webviews render into an FBO-backed `OffscreenRenderingContext`
//! sized to the content rect, which an egui background paint-callback blits
//! under the chrome. Tabs are one live `WebView` each, switched via
//! show/focus + hide/blur.

mod app;
mod controls;
mod dashboard;
mod downloads;
mod glass;
mod icons;
mod keyboard;
mod library;
mod passwords;
mod phosphor;
mod settings;
mod state;
mod theme;
mod ui;
#[cfg(target_os = "macos")]
mod vibrancy;
mod widgets;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;
use std::sync::Arc;

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
    /// Revealed autohide sidebar, floating over the content card.
    overlay_rect_points: Option<egui::Rect>,
    /// A page-initiated dialog or menu is up.
    controls_open: bool,
    /// Throttles history writes: every visit marks the library dirty, and
    /// rewriting the file on each one would be a lot of churn for a browse.
    library_saved_at: std::time::Instant,
    /// Deadline for a deferred egui repaint (e.g. caret blink) — served via
    /// ControlFlow::WaitUntil instead of a max-FPS redraw loop.
    pending_repaint_at: Option<std::time::Instant>,
    /// File downloads (Servo has no download subsystem — we do it ourselves).
    downloads: downloads::DownloadManager,
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
        let egui_glow = EguiGlow::new(
            event_loop,
            window_rendering_context.glow_gl_api(),
            None,
            None,
            false,
        );

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

        let start_url = std::env::args()
            .nth(1)
            .map(|arg| ui::normalize_url(&arg, settings.search_engine))
            .and_then(|arg| Url::parse(&arg).ok())
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
            overlay_rect_points: None,
            controls_open: false,
            library_saved_at: std::time::Instant::now(),
            pending_repaint_at: None,
            downloads: downloads::DownloadManager::default(),
            #[cfg(target_os = "macos")]
            _vibrancy: vibrancy,
        });
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

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: WakerEvent) {
        if let Self::Running(app) = self {
            app.state.servo.spin_event_loop();
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
        let over_content = self.content_rect_points.contains(cursor_points)
            && !self.settings_open
            && !self.controls_open
            && !self
                .overlay_rect_points
                .is_some_and(|rect| rect.contains(cursor_points));

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
            for event in state.download_events.borrow_mut().drain(..) {
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

        let palette = self.palette();
        let active_webview = state.active_webview();
        let offscreen = state.rendering_context.clone();
        let mut ui_output = None;

        // Move UI-facing state out of `self` for the closure; restored below.
        let mut settings = self.settings.clone();
        let favicons = std::mem::take(&mut self.favicons);

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
            };
            let output = ui::draw(root, &mut chrome);
            drop(browser);
            drop(controls);
            drop(library);
            drop(vault);

            let content_rect = output.content_rect;
            let scale = root.pixels_per_point();
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
                    }
                }
            }
            // Rounded-corner masks and border, drawn over the blit.
            ui::finish_content_frame(
                root,
                content_rect,
                &palette,
                !output.settings_open,
                settings.top_glow,
                settings.content_border,
                settings.chrome_opacity,
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
            self.overlay_rect_points = output.chrome_overlay;
            self.controls_open = output.controls_open;
            self.settings_open = output.settings_open;
            ambient = output.ambient;
            for action in output.actions {
                self.apply_action(action);
            }
        }
        // Ambient animations (aurora new-tab page) tick at ~30fps via timed
        // wakes rather than a max-FPS redraw loop.
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

        if self.egui_glow.egui_ctx.requested_repaint_last_pass() {
            state.window.request_redraw();
        } else if self.egui_glow.egui_ctx.has_requested_repaint() {
            // Delayed request (caret blink etc.): wake on a timer instead of
            // redrawing at max FPS until it fires.
            self.pending_repaint_at =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(300));
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
                // zervo:// URLs are internal pages, never engine loads.
                if url.scheme() == "zervo" {
                    if url.as_str().contains("downloads") {
                        self.apply_action(UiAction::OpenDownloads);
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
                #[cfg(target_os = "macos")]
                set_dock_icon(self.settings.app_icon);
                self.apply_theme();
                state.window.request_redraw();
            },
        }
    }

    fn palette(&self) -> Palette {
        theme::resolve(self.settings.theme, self.system_dark, self.settings.accent)
    }

    /// Re-style the chrome and tell every webview the new prefers-color-scheme.
    fn apply_theme(&mut self) {
        let palette = self.palette();
        theme::apply(&self.egui_glow.egui_ctx, &palette);
        self.state.set_engine_theme(palette.dark);
        sync_window_theme(&self.state.window, self.settings.theme);
        // The frost is appearance-adaptive on its own, but a lighter material
        // reads better behind light chrome.
        #[cfg(target_os = "macos")]
        if let Some(vibrancy) = &self._vibrancy {
            vibrancy.set_material(if palette.dark {
                objc2_app_kit::NSVisualEffectMaterial::Sidebar
            } else {
                objc2_app_kit::NSVisualEffectMaterial::HeaderView
            });
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

#[derive(Debug)]
struct WakerEvent;

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
        if let Err(error) = self.0.send_event(WakerEvent) {
            log::warn!("Failed to wake event loop: {error:?}");
        }
    }
}
