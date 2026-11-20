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
    NSGlassEffectViewStyle, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowOrderingMode, NSWorkspace,
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

/// AppKit owns the material. The window's OpenGL view above it owns the
/// foreground, pointer routing and accessibility tree.
pub struct LiquidGlass {
    content: Retained<NSView>,
    views: [Retained<NSGlassEffectView>; crate::ui::NativeGlass::SURFACES],
}

impl LiquidGlass {
    pub fn install(window: &impl HasWindowHandle) -> Option<Self> {
        let _mtm = MainThreadMarker::new()?;
        let class = AnyClass::get(c"NSGlassEffectView")?;
        let content = content_view(window)?;
        // SAFETY: the content view is live and we're on AppKit's main thread.
        let frame = unsafe { content.superview() }?;
        let views = std::array::from_fn(|_| {
            // SAFETY: this is AppKit's public NSGlassEffectView class. NSObject's
            // new returns an owned instance, and this runs on the main thread.
            let view: Retained<NSGlassEffectView> = unsafe { msg_send![class, new] };
            view.setStyle(NSGlassEffectViewStyle::Regular);
            view.setHidden(true);
            frame.addSubview_positioned_relativeTo(
                &view,
                NSWindowOrderingMode::Below,
                Some(&content),
            );
            view
        });
        Some(Self { content, views })
    }

    pub fn update(&self, glass: &crate::ui::NativeGlass, zoom: f32) {
        // SAFETY: all views and this method are confined to the main thread.
        let Some(parent) = (unsafe { self.content.superview() }) else {
            return;
        };
        let bounds = self.content.bounds();
        let zoom = f64::from(zoom);
        for (view, surface) in self.views.iter().zip(glass.surfaces) {
            let hidden = surface.is_none();
            if let Some((rect, radius)) = surface {
                // Egui measures down from the top, in zoomed points. Convert
                // through the content view so fullscreen/titlebar changes use
                // the same coordinate origin as the OpenGL foreground.
                let x = bounds.min().x + f64::from(rect.left()) * zoom;
                let y = if self.content.isFlipped() {
                    bounds.min().y + f64::from(rect.top()) * zoom
                } else {
                    bounds.max().y - f64::from(rect.bottom()) * zoom
                };
                let local = objc2_foundation::NSRect::new(
                    objc2_foundation::NSPoint::new(x, y),
                    objc2_foundation::NSSize::new(
                        f64::from(rect.width()) * zoom,
                        f64::from(rect.height()) * zoom,
                    ),
                );
                let frame = self.content.convertRect_toView(local, Some(&parent));
                // SAFETY: reads the superview of a retained AppKit view on the main thread.
                if unsafe { view.superview() }.as_deref() != Some(&*parent) {
                    parent.addSubview_positioned_relativeTo(
                        view,
                        NSWindowOrderingMode::Below,
                        Some(&self.content),
                    );
                }
                if view.frame() != frame {
                    view.setFrame(frame);
                }
                let radius = f64::from(radius) * zoom;
                if view.cornerRadius() != radius {
                    view.setCornerRadius(radius);
                }
            }
            if view.isHidden() != hidden {
                view.setHidden(hidden);
            }
        }
    }
}

pub fn accessibility_preferences() -> (bool, bool) {
    let workspace = NSWorkspace::sharedWorkspace();
    (
        workspace.accessibilityDisplayShouldReduceTransparency(),
        workspace.accessibilityDisplayShouldReduceMotion(),
    )
}
