//! Engine glue: owns the Servo instance, the rendering contexts, and the tab
//! collection, and implements `WebViewDelegate` for all tabs. Follows
//! servoshell's architecture: one shared delegate, one `WindowRenderingContext`
//! per window wrapped in an `OffscreenRenderingContext` so the egui chrome can
//! composite around the web content; tab switching is show/focus + hide/blur.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use euclid::Scale;
use servo::{
    InputEventId, InputEventResult, KeyboardEvent, LoadStatus, OffscreenRenderingContext, Servo,
    WebView, WebViewBuilder, WindowRenderingContext,
};
use url::Url;
use winit::window::Window;

use crate::controls::Controls;
use crate::keyboard::CMD_OR_CONTROL;
use crate::state::{BrowserState, TabId};

/// Engine-driven download traffic, queued for the UI thread.
#[cfg(feature = "engine-downloads")]
pub enum DownloadEvent {
    /// Servo cannot render this response and is offering it to us.
    Offered {
        request_id: servo::RequestId,
        url: String,
        default_filename: String,
    },
    Chunk {
        request_id: servo::RequestId,
        chunk: Vec<u8>,
    },
    Finished {
        request_id: servo::RequestId,
        ok: bool,
    },
}

fn engine_theme(dark: bool) -> servo::Theme {
    if dark {
        servo::Theme::Dark
    } else {
        servo::Theme::Light
    }
}

pub struct AppState {
    pub servo: Servo,
    /// Renders to the OS window; egui paints into this.
    pub window_rendering_context: Rc<WindowRenderingContext>,
    /// Webviews render here, sized to the content rect; blitted under the chrome.
    pub rendering_context: Rc<OffscreenRenderingContext>,
    pub browser: RefCell<BrowserState>,
    /// Set when Servo has a new frame or the chrome needs redrawing.
    pub needs_repaint: Cell<bool>,
    /// Popups created by script (window.open) waiting to be adopted as tabs.
    pub pending_popups: RefCell<Vec<WebView>>,
    /// Webviews closed by script (window.close) waiting for tab removal.
    pub pending_closes: RefCell<Vec<WebView>>,
    /// In-flight keyboard events, so page-overridable shortcuts can run after
    /// the page had its chance to preventDefault.
    pub pending_keyboard_events: RefCell<HashMap<InputEventId, KeyboardEvent>>,
    /// Whether the chrome (and therefore the page prefers-color-scheme) is dark.
    pub dark_theme: Cell<bool>,
    /// Set when any tab's favicon changed; the UI reloads textures on redraw.
    pub favicons_dirty: Cell<bool>,
    /// ⌘Q was pressed; the event loop exits once the current event is done.
    pub quit_requested: Cell<bool>,
    /// Page-initiated UI (dialogs, pickers, context menus) awaiting an answer.
    pub controls: RefCell<Controls>,
    /// Download events from the engine, drained by the UI each redraw.
    #[cfg(feature = "engine-downloads")]
    pub download_events: RefCell<Vec<DownloadEvent>>,
    /// Last title pushed to the OS window, to avoid redundant sets.
    pub last_window_title: RefCell<String>,
    /// Declared last so the rendering contexts are torn down while the OS
    /// window still exists (see servo/servo#36711).
    pub window: Window,
}

impl AppState {
    pub fn hidpi_scale_factor(
        &self,
    ) -> Scale<f32, servo::DeviceIndependentPixel, servo::DevicePixel> {
        Scale::new(self.window.scale_factor() as f32)
    }

    pub fn active_webview(&self) -> Option<WebView> {
        self.browser
            .borrow()
            .active_tab()
            .and_then(|tab| tab.webview.clone())
    }

    /// Create the engine view for a tab and make it the active one.
    /// Internal (zervo://) tabs never get a webview.
    pub fn open_tab(self: &Rc<Self>, tab_id: TabId, url: Url) {
        log::debug!("ZERVO open_tab tab={tab_id} url={url}");
        if url.scheme() == "zervo" {
            self.activate_tab(tab_id);
            return;
        }
        let webview = WebViewBuilder::new(&self.servo, self.rendering_context.clone())
            .url(url)
            .hidpi_scale_factor(self.hidpi_scale_factor())
            .delegate(self.clone())
            .build();
        webview.notify_theme_change(engine_theme(self.dark_theme.get()));
        if let Some(tab) = self.browser.borrow_mut().tab_mut(tab_id) {
            tab.webview = Some(webview);
            // A tab with an engine view is a web tab, whatever it was before.
            tab.kind = crate::state::TabKind::Web;
        }
        self.activate_tab(tab_id);
    }

    /// Release everything that keeps the engine alive, so the `Servo` handle is
    /// dropped and its `Drop` tells the constellation to exit — which is what
    /// makes Servo write the cookie jar, auth cache and HSTS list out to
    /// `config_dir`. The webviews each hold a clone of the shared delegate,
    /// which is an `Rc` back to this state, so without clearing them the cycle
    /// keeps the engine alive until the process dies and nothing is saved.
    pub fn shutdown(&self) {
        {
            let mut browser = self.browser.borrow_mut();
            for workspace in &mut browser.workspaces {
                for tab in &mut workspace.tabs {
                    tab.webview = None;
                }
            }
        }
        self.pending_popups.borrow_mut().clear();
        self.pending_closes.borrow_mut().clear();
    }

    /// Propagate a chrome theme change to every live webview so pages see the
    /// matching prefers-color-scheme.
    pub fn set_engine_theme(&self, dark: bool) {
        self.dark_theme.set(dark);
        let browser = self.browser.borrow();
        for workspace in &browser.workspaces {
            for tab in &workspace.tabs {
                if let Some(webview) = &tab.webview {
                    webview.notify_theme_change(engine_theme(dark));
                }
            }
        }
    }

    /// Zervo tab switch: show + focus the target, hide + blur + throttle
    /// every other webview (hidden tabs drop out of the WebRender scene).
    pub fn activate_tab(&self, tab_id: TabId) {
        let mut browser = self.browser.borrow_mut();
        browser.active_tab = Some(tab_id);
        for workspace in &browser.workspaces {
            for tab in &workspace.tabs {
                let Some(webview) = &tab.webview else {
                    continue;
                };
                if tab.id == tab_id {
                    webview.show();
                    webview.focus();
                    webview.set_throttled(false);
                } else {
                    webview.hide();
                    webview.blur();
                    webview.set_throttled(true);
                }
            }
        }
        drop(browser);
        self.needs_repaint.set(true);
        self.window.request_redraw();
    }

    pub fn close_tab(&self, tab_id: TabId) {
        log::debug!("ZERVO close_tab tab={tab_id}");
        // Dropping the returned handle (the last one) closes the webview.
        let _closing = self.browser.borrow_mut().remove_tab(tab_id);
        let next = self.browser.borrow().active_tab;
        if let Some(next) = next {
            self.activate_tab(next);
        }
        self.needs_repaint.set(true);
        self.window.request_redraw();
    }

    /// Adopt script-created popups as tabs, drop script-closed webviews, and
    /// refresh per-tab UI state from the engine's cached values. Called once
    /// per event-loop turn, after `spin_event_loop`.
    pub fn sync(self: &Rc<Self>) {
        for webview in self.pending_popups.borrow_mut().drain(..) {
            let mut browser = self.browser.borrow_mut();
            let workspace = browser.active_workspace;
            let url = webview.url().map(|u| u.to_string()).unwrap_or_default();
            let id = browser.add_tab(workspace, url);
            if let Some(tab) = browser.tab_mut(id) {
                tab.webview = Some(webview);
            }
            drop(browser);
            self.activate_tab(id);
        }

        for webview in self.pending_closes.borrow_mut().drain(..) {
            let tab_id = self
                .browser
                .borrow_mut()
                .tab_for_webview_mut(&webview)
                .map(|tab| tab.id);
            if let Some(tab_id) = tab_id {
                self.close_tab(tab_id);
            }
        }

        let mut browser = self.browser.borrow_mut();
        let editing_address = browser.editing_address;
        for workspace in &mut browser.workspaces {
            for tab in &mut workspace.tabs {
                let Some(webview) = tab.webview.clone() else {
                    continue;
                };
                if let Some(url) = webview.url() {
                    tab.url = url.to_string();
                }
                tab.title = webview
                    .page_title()
                    .filter(|title| !title.is_empty())
                    .unwrap_or_else(|| tab.url.clone());
                tab.loading = webview.load_status() != LoadStatus::Complete;
                tab.can_go_back = webview.can_go_back();
                tab.can_go_forward = webview.can_go_forward();
            }
        }
        // The address bar mirrors the active tab (web or internal page).
        let address = browser.active_tab().map(|tab| tab.url.clone());
        if let (false, Some(address)) = (editing_address, address) {
            browser.address_bar = address;
        }

        // Keep the OS window title in sync with the active page.
        let title = browser
            .active_tab()
            .map(|tab| {
                if tab.title.is_empty() {
                    "Zervo".to_owned()
                } else {
                    format!("{} — Zervo", tab.title)
                }
            })
            .unwrap_or_else(|| "Zervo".to_owned());
        drop(browser);
        if *self.last_window_title.borrow() != title {
            self.window.set_title(&title);
            *self.last_window_title.borrow_mut() = title;
        }
    }
}

impl AppState {
    /// Answer a `<input type=file>` request with the system open panel.
    ///
    /// `runModal` blocks until the user is done, which is what the engine
    /// expects here, and matches how every other macOS app behaves.
    #[cfg(target_os = "macos")]
    fn run_file_picker(&self, mut picker: servo::FilePicker) {
        use std::path::PathBuf;

        use objc2::MainThreadMarker;
        use objc2_app_kit::NSOpenPanel;

        /// `NSModalResponseOK`, which objc2-app-kit does not re-export.
        const RESPONSE_OK: isize = 1;

        let Some(mtm) = MainThreadMarker::new() else {
            // Off the main thread there is no safe way to show a panel, and an
            // unanswered picker would hang the page.
            picker.dismiss();
            return;
        };
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setCanChooseFiles(true);
        panel.setCanChooseDirectories(false);
        panel.setAllowsMultipleSelection(picker.allow_select_multiple());

        if panel.runModal() != RESPONSE_OK {
            picker.dismiss();
            return;
        }
        let paths: Vec<PathBuf> = panel
            .URLs()
            .iter()
            .filter_map(|url| url.path())
            .map(|path| PathBuf::from(path.to_string()))
            .collect();
        if paths.is_empty() {
            picker.dismiss();
            return;
        }
        picker.select(&paths);
        picker.submit();
    }

    #[cfg(not(target_os = "macos"))]
    fn run_file_picker(&self, picker: servo::FilePicker) {
        picker.dismiss();
    }
}

impl servo::WebViewDelegate for AppState {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.needs_repaint.set(true);
        self.window.request_redraw();
    }

    fn notify_page_title_changed(&self, _webview: WebView, _title: Option<String>) {
        self.window.request_redraw();
    }

    fn notify_history_changed(&self, _webview: WebView, _entries: Vec<Url>, _current: usize) {
        self.window.request_redraw();
    }

    fn notify_load_status_changed(&self, _webview: WebView, _status: LoadStatus) {
        self.window.request_redraw();
    }

    fn request_create_new(&self, parent_webview: WebView, request: servo::CreateNewWebViewRequest) {
        log::debug!(
            "ZERVO request_create_new from parent {:?}",
            parent_webview.id()
        );
        let webview = request
            .builder(self.rendering_context.clone())
            .hidpi_scale_factor(parent_webview.hidpi_scale_factor())
            .delegate(parent_webview.delegate())
            .build();
        // Adopted as a tab (and activated) in the next `sync`.
        self.pending_popups.borrow_mut().push(webview);
        self.window.request_redraw();
    }

    fn notify_closed(&self, webview: WebView) {
        log::debug!("ZERVO notify_closed webview {:?}", webview.id());
        self.pending_closes.borrow_mut().push(webview);
        self.window.request_redraw();
    }

    /// Servo hit a response it cannot display. Accepting hands us the bytes;
    /// the engine keeps doing the transfer, so cookies, auth and redirects
    /// all still apply.
    #[cfg(feature = "engine-downloads")]
    fn notify_unsupported_response(
        &self,
        _webview: WebView,
        mut response: servo::UnsupportedResponse,
    ) {
        self.download_events
            .borrow_mut()
            .push(DownloadEvent::Offered {
                request_id: response.request_id,
                url: response.url.to_string(),
                default_filename: response.default_filename.clone(),
            });
        response.accept();
        self.window.request_redraw();
    }

    #[cfg(feature = "engine-downloads")]
    fn notify_response_chunk(
        &self,
        _webview: WebView,
        request_id: servo::RequestId,
        chunk: Vec<u8>,
    ) {
        self.download_events
            .borrow_mut()
            .push(DownloadEvent::Chunk { request_id, chunk });
        self.window.request_redraw();
    }

    #[cfg(feature = "engine-downloads")]
    fn notify_response_eof(
        &self,
        _webview: WebView,
        request_id: servo::RequestId,
        result: Result<(), ()>,
    ) {
        self.download_events
            .borrow_mut()
            .push(DownloadEvent::Finished {
                request_id,
                ok: result.is_ok(),
            });
        self.window.request_redraw();
    }

    fn notify_favicon_changed(&self, _webview: WebView) {
        self.favicons_dirty.set(true);
        self.window.request_redraw();
    }

    fn notify_cursor_changed(&self, _webview: WebView, cursor: servo::Cursor) {
        if let Some(winit_cursor) = winit_cursor(cursor) {
            self.window.set_cursor(winit_cursor);
        }
    }

    /// Page-overridable shortcuts: run only when the page didn't preventDefault.
    fn notify_input_event_handled(
        &self,
        webview: WebView,
        event_id: InputEventId,
        result: InputEventResult,
    ) {
        let Some(keyboard_event) = self.pending_keyboard_events.borrow_mut().remove(&event_id)
        else {
            return;
        };
        if result.intersects(InputEventResult::DefaultPrevented | InputEventResult::Consumed) {
            return;
        }
        keyboard_types::ShortcutMatcher::from_event(keyboard_event.event)
            .shortcut(CMD_OR_CONTROL, 'R', || webview.reload())
            .shortcut(CMD_OR_CONTROL, '=', || {
                webview.set_page_zoom(webview.page_zoom() + 0.1)
            })
            .shortcut(CMD_OR_CONTROL, '-', || {
                webview.set_page_zoom(webview.page_zoom() - 0.1)
            })
            .shortcut(CMD_OR_CONTROL, '0', || webview.set_page_zoom(1.0));
    }

    fn show_embedder_control(&self, _webview: WebView, control: servo::EmbedderControl) {
        match control {
            // The file picker is the one control the OS draws better than we
            // can, and it has to be answered synchronously anyway.
            servo::EmbedderControl::FilePicker(picker) => self.run_file_picker(picker),
            control => self.controls.borrow_mut().push(control),
        }
        self.window.request_redraw();
    }

    fn hide_embedder_control(&self, _webview: WebView, control_id: servo::EmbedderControlId) {
        self.controls.borrow_mut().hide(control_id);
        self.window.request_redraw();
    }
}

fn winit_cursor(cursor: servo::Cursor) -> Option<winit::window::Cursor> {
    use servo::Cursor;
    use winit::window::CursorIcon;
    let icon = match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::ContextMenu => CursorIcon::ContextMenu,
        Cursor::Help => CursorIcon::Help,
        Cursor::Progress => CursorIcon::Progress,
        Cursor::Wait => CursorIcon::Wait,
        Cursor::Cell => CursorIcon::Cell,
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::Text => CursorIcon::Text,
        Cursor::VerticalText => CursorIcon::VerticalText,
        Cursor::Alias => CursorIcon::Alias,
        Cursor::Copy => CursorIcon::Copy,
        Cursor::Move => CursorIcon::Move,
        Cursor::NoDrop => CursorIcon::NoDrop,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::EResize => CursorIcon::EResize,
        Cursor::NResize => CursorIcon::NResize,
        Cursor::NeResize => CursorIcon::NeResize,
        Cursor::NwResize => CursorIcon::NwResize,
        Cursor::SResize => CursorIcon::SResize,
        Cursor::SeResize => CursorIcon::SeResize,
        Cursor::SwResize => CursorIcon::SwResize,
        Cursor::WResize => CursorIcon::WResize,
        Cursor::EwResize => CursorIcon::EwResize,
        Cursor::NsResize => CursorIcon::NsResize,
        Cursor::NeswResize => CursorIcon::NeswResize,
        Cursor::NwseResize => CursorIcon::NwseResize,
        Cursor::ColResize => CursorIcon::ColResize,
        Cursor::RowResize => CursorIcon::RowResize,
        Cursor::AllScroll => CursorIcon::AllScroll,
        Cursor::ZoomIn => CursorIcon::ZoomIn,
        Cursor::ZoomOut => CursorIcon::ZoomOut,
        Cursor::None => return None,
    };
    Some(icon.into())
}
