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
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAnimationContext, NSAutoresizingMaskOptions, NSColor, NSView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow,
    NSWindowOrderingMode,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Keeps the backdrop alive and lets you retune it at runtime.
pub struct Vibrancy {
    view: Retained<NSVisualEffectView>,
}

impl Vibrancy {
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

    /// Fade the whole backdrop in or out, over `duration` seconds.
    ///
    /// Faded out, the window has no system blur at all and shows what is
    /// behind it directly — the window is already `setOpaque(false)` with a
    /// clear background, so there is nothing else in the way. That is a step
    /// past the clearest material AppKit offers, which is the only way to make
    /// the sheerest setting visibly sheerer than the one below it: every
    /// material *is* a frost, so the way past the clearest one is to stop
    /// asking for a frost.
    ///
    /// Alpha rather than `setHidden`, so it crossfades with everything else
    /// instead of snapping a frame out of step.
    pub fn set_visible_animated(&self, visible: bool, duration: f64) {
        NSAnimationContext::beginGrouping();
        let context = NSAnimationContext::currentContext();
        context.setDuration(duration);
        context.setAllowsImplicitAnimation(true);
        self.view.setAlphaValue(if visible { 1.0 } else { 0.0 });
        NSAnimationContext::endGrouping();
    }
}

/// winit's `ns_view` *is* the window's `contentView`.
fn content_view(window: &impl HasWindowHandle) -> Option<Retained<NSView>> {
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

    Some(Vibrancy { view: effect })
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
