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
use objc2::{MainThreadMarker, MainThreadOnly, msg_send, runtime::AnyClass};
use objc2_app_kit::{
    NSAnimationContext, NSAutoresizingMaskOptions, NSColor, NSGlassEffectView,
    NSGlassEffectViewStyle, NSTitlebarSeparatorStyle, NSToolbar, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindow, NSWindowButton, NSWindowOrderingMode, NSWindowStyleMask, NSWindowToolbarStyle,
    NSWorkspace,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, ns_string};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Keeps the backdrop alive and lets you retune it at runtime.
pub struct Vibrancy {
    view: Retained<NSVisualEffectView>,
    content: Retained<NSView>,
    sidebar: Retained<NSVisualEffectView>,
}

impl Vibrancy {
    pub fn update_sidebar(&self, rect: Option<egui::Rect>, zoom: f32) {
        update_surface(&self.content, &self.sidebar, rect, zoom);
    }

    /// Swap material, crossfading over `duration` seconds.
    ///
    /// A bare `setMaterial` snaps, and the WindowServer composites this view a
    /// frame behind our own GL output — so the snap lands one frame after
    /// everything we draw ourselves, and the frosted panel visibly changes
    /// last. Handing the change to AppKit inside an animation group turns it
    /// into a crossfade, which has no single frame to be out of step on.
    pub fn set_material_animated(&self, material: NSVisualEffectMaterial, duration: f64) {
        NSAnimationContext::beginGrouping();
        let context = NSAnimationContext::currentContext();
        context.setDuration(duration);
        context.setAllowsImplicitAnimation(true);
        self.view.setMaterial(material);
        NSAnimationContext::endGrouping();
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
    // A hidden toolbar selects the unified titlebar geometry without
    // reserving a toolbar row or introducing any browser controls.
    toolbar.setVisible(false);
    window.setToolbar(Some(&toolbar));
    window.setToolbarStyle(NSWindowToolbarStyle::Unified);
    window.setTitlebarSeparatorStyle(NSTitlebarSeparatorStyle::None);
    Some(())
}

pub fn window_controls(window: &impl HasWindowHandle, zoom: f32) -> Option<egui::Rect> {
    let _mtm = MainThreadMarker::new()?;
    let content = content_view(window)?;
    let window = content.window()?;
    if window.styleMask().contains(NSWindowStyleMask::FullScreen) {
        return None;
    }
    let bounds = content.bounds();
    [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ]
    .into_iter()
    .filter_map(|kind| {
        let button = window.standardWindowButton(kind)?;
        if button.isHiddenOrHasHiddenAncestor() {
            return None;
        }
        let rect = button.convertRect_toView(button.bounds(), Some(&content));
        let x = rect.min().x - bounds.min().x;
        let y = if content.isFlipped() {
            rect.min().y - bounds.min().y
        } else {
            bounds.max().y - rect.max().y
        };
        Some(egui::Rect::from_min_size(
            egui::pos2(x as f32, y as f32) / zoom,
            egui::vec2(rect.size.width as f32, rect.size.height as f32) / zoom,
        ))
    })
    .reduce(egui::Rect::union)
}

fn update_surface(content: &NSView, surface: &NSView, rect: Option<egui::Rect>, zoom: f32) {
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
        parent.addSubview_positioned_relativeTo(
            surface,
            NSWindowOrderingMode::Below,
            Some(content),
        );
    }
    if surface.frame() != frame {
        surface.setFrame(frame);
    }
    if surface.isHidden() {
        surface.setHidden(false);
    }
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

    Some(Vibrancy {
        view: effect,
        content,
        sidebar,
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

/// AppKit owns the material. The window's OpenGL view above it owns the
/// foreground, pointer routing and accessibility tree.
pub struct LiquidGlass {
    content: Retained<NSView>,
    view: Retained<NSGlassEffectView>,
    dark: std::cell::Cell<Option<bool>>,
}

impl LiquidGlass {
    pub fn install(window: &impl HasWindowHandle) -> Option<Self> {
        let _mtm = MainThreadMarker::new()?;
        let content = content_view(window)?;
        // SAFETY: the content view is live and we're on AppKit's main thread.
        let frame = unsafe { content.superview() }?;
        let view = new_glass_view()?;
        view.setStyle(NSGlassEffectViewStyle::Regular);
        view.setHidden(true);
        frame.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, Some(&content));
        Some(Self {
            content,
            view,
            dark: Default::default(),
        })
    }

    pub fn update(&self, surface: Option<(egui::Rect, f32)>, zoom: f32, dark: bool) {
        if let Some((_, radius)) = surface {
            if self.dark.replace(Some(dark)) != Some(dark) {
                self.view
                    .setTintColor(Some(&crate::overlay_glass::Role::Panel.tint(dark)));
            }
            let radius = f64::from(radius) * f64::from(zoom);
            if self.view.cornerRadius() != radius {
                self.view.setCornerRadius(radius);
            }
        }
        update_surface(
            &self.content,
            &self.view,
            surface.map(|(rect, _)| rect),
            zoom,
        );
    }
}

pub(crate) fn glass_class() -> Option<&'static AnyClass> {
    AnyClass::get(c"NSGlassEffectView")
}

pub(crate) fn new_glass_view() -> Option<Retained<NSGlassEffectView>> {
    let _mtm = MainThreadMarker::new()?;
    let class = glass_class()?;
    // SAFETY: this is AppKit's public glass class, on the main thread.
    unsafe { msg_send![class, new] }
}

pub fn accessibility_preferences() -> (bool, bool) {
    let workspace = NSWorkspace::sharedWorkspace();
    (
        workspace.accessibilityDisplayShouldReduceTransparency(),
        workspace.accessibilityDisplayShouldReduceMotion(),
    )
}
