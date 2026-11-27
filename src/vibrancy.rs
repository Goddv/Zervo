//! macOS frosted-glass (NSVisualEffectView) backdrop for a winit 0.30 window
//! whose content view is driven by OpenGL (surfman / glutin).
//!
//! Why it is installed as a *sibling* of winit's view rather than a subview:
//!
//! * winit's `WindowDelegate::view()` does `contentView().unwrap()` and then an
//!   unchecked `Retained::cast::<WinitView>()`, so replacing the window's
//!   `contentView` with an `NSVisualEffectView` is UB (winit will send
//!   WinitView selectors / read WinitView ivars from it).
//! * surfman calls `view.setLayer(...)` + `setWantsLayer(true)` on winit's
//!   view, making it *layer-hosting*; AppKit forbids subviews on such a view,
//!   and a subview's layer would in any case composite *above* the host
//!   layer's own contents (i.e. above the GL output).
//!
//! So the effect view goes into the window's frame view
//! (`contentView.superview`, an `NSThemeFrame`) ordered `Below` the content
//! view. Verified at runtime to survive fullscreen enter/exit and to autoresize.

#![cfg(target_os = "macos")]

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, msg_send, runtime::AnyClass, sel};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSColor, NSGlassEffectView, NSGlassEffectViewStyle,
    NSTitlebarSeparatorStyle, NSToolbar, NSView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowButton,
    NSWindowOrderingMode, NSWindowStyleMask, NSWindowToolbarStyle, NSWorkspace,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSURL, ns_string};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Keeps the backdrop alive and lets you retune it at runtime.
pub struct Vibrancy {
    view: Retained<NSVisualEffectView>,
    glass: Option<Retained<NSGlassEffectView>>,
    content: Retained<NSView>,
    sidebar: Retained<NSVisualEffectView>,
    address: Retained<NSVisualEffectView>,
    fullscreen_controls: Option<FullscreenControls>,
}

impl Vibrancy {
    pub fn has_window_glass(&self) -> bool {
        self.glass.is_some()
    }

    pub fn update_window_backdrop(
        &self,
        visible: bool,
        new_tab_backdrop: bool,
        palette: crate::theme::Palette,
    ) {
        use crate::theme::SystemBackdrop;
        // One system material samples the desktop on a transparent New Tab.
        // A visual-effect view under Liquid Glass would blur its input twice.
        let native_liquid = new_tab_backdrop && visible;
        if self.view.isHidden() != native_liquid {
            self.view.setHidden(native_liquid);
        }
        let material = if new_tab_backdrop {
            NSVisualEffectMaterial::Sidebar
        } else if palette.translucency.backdrop() == SystemBackdrop::Opaque {
            NSVisualEffectMaterial::WindowBackground
        } else {
            NSVisualEffectMaterial::UnderWindowBackground
        };
        if self.view.material() != material {
            self.view.setMaterial(material);
        }
        if let Some(glass) = self.glass.as_ref()
            && glass.isHidden() == visible
        {
            glass.setHidden(!visible);
        }
    }

    pub fn update_sidebar(&self, rect: Option<egui::Rect>, zoom: f32) {
        update_surface(
            &self.content,
            &self.sidebar,
            rect,
            zoom,
            NSWindowOrderingMode::Below,
        );
    }

    pub fn update_address(&self, rect: Option<(egui::Rect, f32)>, zoom: f32) {
        if let Some((_, radius)) = rect {
            round_frost(&self.address, radius * zoom);
        }
        update_surface(
            &self.content,
            &self.address,
            rect.map(|(rect, _)| rect),
            zoom,
            NSWindowOrderingMode::Below,
        );
    }

    pub fn window_controls(&self, zoom: f32) -> (Option<egui::Rect>, bool) {
        let Some(window) = self.content.window() else {
            return (None, false);
        };
        if window.styleMask().contains(NSWindowStyleMask::FullScreen) {
            return (
                self.fullscreen_controls
                    .as_ref()
                    .map(|row| row.bounds(zoom)),
                true,
            );
        }
        let bounds = self.content.bounds();
        let rect = WINDOW_CONTROLS
            .into_iter()
            .filter_map(|kind| {
                let button = window.standardWindowButton(kind)?;
                if button.isHiddenOrHasHiddenAncestor() {
                    return None;
                }
                let rect = button.convertRect_toView(button.bounds(), Some(&self.content));
                let y = if self.content.isFlipped() {
                    rect.min().y - bounds.min().y
                } else {
                    bounds.max().y - rect.max().y
                };
                Some(egui::Rect::from_min_size(
                    egui::pos2((rect.min().x - bounds.min().x) as f32, y as f32) / zoom,
                    egui::vec2(rect.size.width as f32, rect.size.height as f32) / zoom,
                ))
            })
            .reduce(egui::Rect::union);
        (rect, false)
    }

    pub fn update_window_controls(&self, opacity: f32) {
        let Some(window) = self.content.window() else {
            return;
        };
        let fullscreen = window.styleMask().contains(NSWindowStyleMask::FullScreen);
        if let Some(row) = &self.fullscreen_controls {
            row.update(&self.content, fullscreen);
        }
        let opacity = if fullscreen && self.fullscreen_controls.is_some() {
            0.0
        } else {
            f64::from(opacity)
        };
        for kind in WINDOW_CONTROLS {
            if let Some(button) = window.standardWindowButton(kind)
                && button.alphaValue() != opacity
            {
                button.setAlphaValue(opacity);
            }
        }
    }
}

/// winit's `ns_view` *is* the window's `contentView`.
pub(crate) fn content_view(window: &impl HasWindowHandle) -> Option<Retained<NSView>> {
    let handle = window.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::AppKit(h) => {
            let ptr: *mut NSView = h.ns_view.as_ptr().cast();
            // SAFETY: winit keeps this NSView alive for the Window's lifetime.
            unsafe { Retained::retain(ptr) }
        },
        _ => None,
    }
}

/// Ask AppKit for the unified titlebar's native window-control layout.
pub fn configure_titlebar(window: &impl HasWindowHandle) -> Option<()> {
    let mtm = MainThreadMarker::new()?;
    let window = content_view(window)?.window()?;
    let toolbar = NSToolbar::initWithIdentifier(NSToolbar::alloc(mtm), ns_string!("ZervoTitlebar"));
    window.setToolbar(Some(&toolbar));
    window.setToolbarStyle(NSWindowToolbarStyle::Unified);
    // AppKit can reset visibility when attaching a toolbar. Hide it after
    // attachment so AppKit doesn't draw an empty toolbar in fullscreen.
    toolbar.setVisible(false);
    window.setTitlebarSeparatorStyle(NSTitlebarSeparatorStyle::None);
    Some(())
}

const WINDOW_CONTROLS: [NSWindowButton; 3] = [
    NSWindowButton::CloseButton,
    NSWindowButton::MiniaturizeButton,
    NSWindowButton::ZoomButton,
];

struct FullscreenControls {
    view: Retained<NSView>,
}

impl FullscreenControls {
    const LEFT: f64 = 13.0;
    const CENTER_Y: f64 = 20.0;
    const GAP: f64 = 9.0;

    fn new(window: &NSWindow, mtm: MainThreadMarker) -> Option<Self> {
        let view = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
        let style = window.styleMask();
        let mut width: f64 = 0.0;
        let mut height: f64 = 0.0;
        let mut buttons = Vec::with_capacity(WINDOW_CONTROLS.len());
        for (kind, action) in WINDOW_CONTROLS.into_iter().zip([
            sel!(performClose:),
            sel!(performMiniaturize:),
            sel!(toggleFullScreen:),
        ]) {
            let button = NSWindow::standardWindowButton_forStyleMask(kind, style, mtm)?;
            // SAFETY: the window owns the view hierarchy and implements each
            // one-argument AppKit action. NSControl keeps a weak target.
            unsafe {
                button.setTarget(Some(window));
                button.setAction(Some(action));
            }
            button.setEnabled(kind != NSWindowButton::MiniaturizeButton);
            let size = button.frame().size;
            width += size.width;
            height = height.max(size.height);
            buttons.push(button);
        }
        width += Self::GAP * (buttons.len() - 1) as f64;
        view.setFrameSize(NSSize::new(width, height));
        let mut x = 0.0;
        for button in buttons {
            let size = button.frame().size;
            button.setFrameOrigin(NSPoint::new(x, (height - size.height) * 0.5));
            view.addSubview(&button);
            x += size.width + Self::GAP;
        }
        view.setHidden(true);
        Some(Self { view })
    }

    fn bounds(&self, zoom: f32) -> egui::Rect {
        let size = self.view.frame().size;
        egui::Rect::from_min_size(
            egui::pos2(
                Self::LEFT as f32,
                (Self::CENTER_Y - size.height * 0.5) as f32,
            ) / zoom,
            egui::vec2(size.width as f32, size.height as f32) / zoom,
        )
    }

    fn update(&self, content: &NSView, fullscreen: bool) {
        // AppKit detaches the titlebar when entering fullscreen. Keep these
        // buttons above the browser's content view in the window itself.
        update_surface(
            content,
            &self.view,
            fullscreen.then(|| self.bounds(1.0)),
            1.0,
            NSWindowOrderingMode::Above,
        );
    }
}

fn update_surface(
    content: &NSView,
    surface: &NSView,
    rect: Option<egui::Rect>,
    zoom: f32,
    order: NSWindowOrderingMode,
) {
    let Some(rect) = rect else {
        if !surface.isHidden() {
            surface.setHidden(true);
        }
        return;
    };
    // SAFETY: the retained AppKit views are main-thread objects.
    let Some(parent) = (unsafe { content.superview() }) else {
        return;
    };
    let bounds = content.bounds();
    let zoom = f64::from(zoom);
    let x = bounds.min().x + f64::from(rect.left()) * zoom;
    let y = if content.isFlipped() {
        bounds.min().y + f64::from(rect.top()) * zoom
    } else {
        bounds.max().y - f64::from(rect.bottom()) * zoom
    };
    let local = NSRect::new(
        NSPoint::new(x, y),
        NSSize::new(
            f64::from(rect.width()) * zoom,
            f64::from(rect.height()) * zoom,
        ),
    );
    let frame = content.convertRect_toView(local, Some(&parent));
    // SAFETY: reads the superview of a live main-thread AppKit view.
    if unsafe { surface.superview() }.as_deref() != Some(&*parent) {
        let relative = (order == NSWindowOrderingMode::Below).then_some(content);
        parent.addSubview_positioned_relativeTo(surface, order, relative);
    }
    if surface.frame() != frame {
        surface.setFrame(frame);
    }
    if surface.isHidden() {
        surface.setHidden(false);
    }
}

fn round_frost(view: &NSVisualEffectView, radius: f32) {
    if let Some(layer) = view.layer()
        && layer.cornerRadius() != f64::from(radius)
    {
        layer.setCornerRadius(f64::from(radius));
        layer.setMasksToBounds(true);
    }
}

fn frosted_field(mtm: MainThreadMarker) -> Retained<NSVisualEffectView> {
    let view = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), NSRect::ZERO);
    view.setMaterial(NSVisualEffectMaterial::Popover);
    view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    view.setState(NSVisualEffectState::FollowsWindowActiveState);
    view.setWantsLayer(true);
    view.setHidden(true);
    view
}

/// Install a full-window `NSVisualEffectView` behind winit's content view.
///
/// Must be called on the main thread. Returns `None` off-main-thread or on a
/// non-AppKit handle.
pub fn install(
    window: &impl HasWindowHandle,
    material: NSVisualEffectMaterial,
) -> Option<Vibrancy> {
    let mtm = MainThreadMarker::new()?;

    let content = content_view(window)?;
    let ns_window: Retained<NSWindow> = content.window()?;

    // Idempotent even if `with_transparent(true)` already did it.
    ns_window.setOpaque(false);
    ns_window.setBackgroundColor(Some(&NSColor::clearColor()));

    // SAFETY: main thread; the frame view is owned by the window.
    let frame_view: Retained<NSView> = unsafe { content.superview() }?;

    let effect =
        NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), frame_view.bounds());
    effect.setMaterial(material);
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setState(NSVisualEffectState::Active);
    effect.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );

    frame_view.setAutoresizesSubviews(true);
    frame_view.addSubview_positioned_relativeTo(
        &effect,
        NSWindowOrderingMode::Below,
        Some(&content),
    );

    let glass = new_glass_view();
    if let Some(glass) = &glass {
        // The window clips the outer silhouette. One continuous effect stays
        // under the page and shows through the inset and rounded page corners.
        glass.setFrame(frame_view.bounds());
        glass.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        glass.setHidden(true);
        frame_view.addSubview_positioned_relativeTo(
            glass,
            NSWindowOrderingMode::Below,
            Some(&content),
        );
    }

    let sidebar = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), NSRect::ZERO);
    sidebar.setMaterial(NSVisualEffectMaterial::Sidebar);
    sidebar.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    sidebar.setState(NSVisualEffectState::FollowsWindowActiveState);
    sidebar.setHidden(true);
    frame_view.addSubview_positioned_relativeTo(
        &sidebar,
        NSWindowOrderingMode::Below,
        Some(&content),
    );

    let address = frosted_field(mtm);
    frame_view.addSubview_positioned_relativeTo(
        &address,
        NSWindowOrderingMode::Below,
        Some(&content),
    );

    Some(Vibrancy {
        view: effect,
        glass,
        content,
        sidebar,
        address,
        fullscreen_controls: FullscreenControls::new(&ns_window, mtm),
    })
}

/// Belt-and-braces: force the GL view's layer tree to composite with alpha.
///
/// surfman copies `NSWindow.isOpaque` into its `CALayer.opaque` **once**, when
/// the rendering context is created. If the window was not created with
/// `with_transparent(true)`, call this *after* `WindowRenderingContext::new`.
pub fn force_layer_transparent(window: &impl HasWindowHandle) -> Option<()> {
    let _mtm = MainThreadMarker::new()?;
    let content = content_view(window)?;
    let layer = content.layer()?;
    layer.setOpaque(false);
    // SAFETY: main thread; read-only walk of the layer's children.
    if let Some(sublayers) = unsafe { layer.sublayers() } {
        for sub in sublayers.to_vec() {
            sub.setOpaque(false);
        }
    }
    Some(())
}

pub(crate) fn glass_class() -> Option<&'static AnyClass> {
    AnyClass::get(c"NSGlassEffectView")
}

pub(crate) fn new_glass_view() -> Option<Retained<NSGlassEffectView>> {
    let _mtm = MainThreadMarker::new()?;
    let class = glass_class()?;
    // SAFETY: this is AppKit's public glass class, on the main thread.
    let glass: Retained<NSGlassEffectView> = unsafe { msg_send![class, new] };
    // Leave tint to AppKit so the glass responds to the system appearance.
    glass.setStyle(NSGlassEffectViewStyle::Regular);
    Some(glass)
}

pub(crate) fn open_appearance_settings() {
    if let Some(url) = NSURL::URLWithString(ns_string!(
        "x-apple.systempreferences:com.apple.Appearance-Settings.extension"
    )) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}

pub fn accessibility_preferences() -> (bool, bool) {
    let workspace = NSWorkspace::sharedWorkspace();
    (
        workspace.accessibilityDisplayShouldReduceTransparency(),
        workspace.accessibilityDisplayShouldReduceMotion(),
    )
}
