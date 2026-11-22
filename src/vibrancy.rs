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
    sidebar_glass: Option<Retained<NSGlassEffectView>>,
    sidebar_dark: Option<bool>,
    address: Retained<NSVisualEffectView>,
    tabs: Vec<Retained<NSVisualEffectView>>,
}

impl Vibrancy {
    pub fn update_sidebar(
        &mut self,
        rect: Option<egui::Rect>,
        zoom: f32,
        liquid: bool,
        dark: bool,
    ) {
        let glass_rect = if liquid { rect } else { None };
        let mut frost_rect = rect;
        if let Some(glass) = &self.sidebar_glass {
            if glass_rect.is_some() {
                if self.sidebar_dark != Some(dark) {
                    glass.setTintColor(Some(&crate::overlay_glass::Role::Sidebar.tint(dark)));
                    self.sidebar_dark = Some(dark);
                }
                let radius =
                    f64::from(crate::overlay_glass::Role::Sidebar.radius()) * f64::from(zoom);
                if glass.cornerRadius() != radius {
                    glass.setCornerRadius(radius);
                }
                frost_rect = None;
            }
            // Match the hover sidebar, whose left edge is clipped by the window.
            let glass_rect = glass_rect.map(|rect| rect.with_min_x(rect.left() - 20.0));
            update_surface(&self.content, glass, glass_rect, zoom);
        }
        update_surface(&self.content, &self.sidebar, frost_rect, zoom);
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
        );
    }

    pub fn update_tabs(&mut self, tabs: &[egui::Rect], zoom: f32) {
        let mtm = self.content.mtm();
        let radius = f32::from(crate::overlay_glass::Role::Tab.radius());
        // Reuse the visible rows' layers as the list scrolls. Their labels and
        // interactions stay in the existing OpenGL pass.
        for (index, &rect) in tabs.iter().enumerate() {
            if index == self.tabs.len() {
                self.tabs.push(frosted_field(mtm));
            }
            let tab = &self.tabs[index];
            round_frost(tab, radius.min(rect.height() * 0.5) * zoom);
            update_surface(&self.content, tab, Some(rect), zoom);
        }
        for tab in self.tabs.iter().skip(tabs.len()) {
            if !tab.isHidden() {
                tab.setHidden(true);
            }
        }
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

const WINDOW_CONTROLS: [NSWindowButton; 3] = [
    NSWindowButton::CloseButton,
    NSWindowButton::MiniaturizeButton,
    NSWindowButton::ZoomButton,
];

pub fn set_window_controls_opacity(window: &impl HasWindowHandle, opacity: f32) {
    let Some(window) = content_view(window).and_then(|content| content.window()) else {
        return;
    };
    let opacity = f64::from(opacity);
    for kind in WINDOW_CONTROLS {
        if let Some(button) = window.standardWindowButton(kind)
            && button.alphaValue() != opacity
        {
            button.setAlphaValue(opacity);
        }
    }
}

pub fn window_controls(window: &impl HasWindowHandle, zoom: f32) -> Option<egui::Rect> {
    let _mtm = MainThreadMarker::new()?;
    let content = content_view(window)?;
    let window = content.window()?;
    if window.styleMask().contains(NSWindowStyleMask::FullScreen) {
        return None;
    }
    let bounds = content.bounds();
    WINDOW_CONTROLS
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

    let sidebar_glass = new_glass_view();
    if let Some(glass) = &sidebar_glass {
        glass.setStyle(NSGlassEffectViewStyle::Regular);
        glass.setHidden(true);
        frame_view.addSubview_positioned_relativeTo(
            glass,
            NSWindowOrderingMode::Below,
            Some(&content),
        );
    }
    let address = frosted_field(mtm);
    frame_view.addSubview_positioned_relativeTo(
        &address,
        NSWindowOrderingMode::Below,
        Some(&content),
    );

    Some(Vibrancy {
        view: effect,
        content,
        sidebar,
        sidebar_glass,
        sidebar_dark: None,
        address,
        tabs: Vec::new(),
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
    unsafe { msg_send![class, new] }
}

pub fn accessibility_preferences() -> (bool, bool) {
    let workspace = NSWorkspace::sharedWorkspace();
    (
        workspace.accessibilityDisplayShouldReduceTransparency(),
        workspace.accessibilityDisplayShouldReduceMotion(),
    )
}
