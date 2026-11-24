//! Keep Auto appearance tied to AppKit's application appearance. A window's
//! own appearance can be overridden, and its change events can be missed.

use std::{ffi::c_void, ptr};

use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::AnyObject,
};
use objc2_app_kit::{NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication};
use objc2_foundation::{
    NSArray, NSDictionary, NSKeyValueObservingOptions, NSObject,
    NSObjectNSKeyValueObserverRegistration, NSString, ns_string,
};
use winit::event_loop::EventLoopProxy;

use crate::WakerEvent;

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ZervoAppearanceObserver"]
    #[ivars = EventLoopProxy<WakerEvent>]
    struct Observer;

    impl Observer {
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn appearance_changed(
            &self,
            _key_path: Option<&NSString>,
            _object: Option<&AnyObject>,
            _change: Option<&NSDictionary>,
            _context: *mut c_void,
        ) {
            let _ = self.ivars().send_event(WakerEvent::SystemAppearanceChanged);
        }
    }
);

pub(crate) struct AppearanceObserver {
    app: Retained<NSApplication>,
    observer: Retained<Observer>,
}

impl AppearanceObserver {
    pub(crate) fn install(proxy: EventLoopProxy<WakerEvent>) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let app = NSApplication::sharedApplication(mtm);
        let observer = Observer::alloc(mtm).set_ivars(proxy);
        // SAFETY: Observer inherits NSObject's designated initializer.
        let observer: Retained<Observer> = unsafe { msg_send![super(observer), init] };
        // SAFETY: AppKit changes effectiveAppearance on the main thread.
        // Both objects are retained until Drop removes this registration.
        unsafe {
            app.addObserver_forKeyPath_options_context(
                &observer,
                ns_string!("effectiveAppearance"),
                NSKeyValueObservingOptions::empty(),
                ptr::null_mut(),
            );
        }
        Some(Self { app, observer })
    }
}

impl Drop for AppearanceObserver {
    fn drop(&mut self) {
        // SAFETY: This instance registered that exact observer and key path.
        unsafe {
            self.app
                .removeObserver_forKeyPath(&self.observer, ns_string!("effectiveAppearance"));
        }
    }
}

pub(crate) fn system_is_dark() -> Option<bool> {
    let app = NSApplication::sharedApplication(MainThreadMarker::new()?);
    // SAFETY: Both immutable appearance name constants are available on all
    // macOS versions supported by Zervo.
    let (light, dark) = unsafe { (NSAppearanceNameAqua, NSAppearanceNameDarkAqua) };
    app.effectiveAppearance()
        .bestMatchFromAppearancesWithNames(&NSArray::from_slice(&[light, dark]))
        .map(|matched| &*matched == dark)
}
