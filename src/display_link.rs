//! A display refresh driver shared by Servo and the chrome on macOS.

use std::{cell::RefCell, rc::Rc};

use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, rc::Retained, sel};
use objc2_foundation::{NSObject, NSObjectProtocol, NSRunLoop, NSRunLoopCommonModes};
use objc2_quartz_core::CADisplayLink;
use servo::RefreshDriver;
use winit::window::Window;

type Callback = Box<dyn Fn() + Send + 'static>;

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ZervoDisplayLinkTarget"]
    #[ivars = RefCell<Vec<Callback>>]
    struct Target;

    impl Target {
        #[unsafe(method(displayLinkDidFire:))]
        fn tick(&self, link: &CADisplayLink) {
            link.setPaused(true);
            let callbacks = self.ivars().take();
            for callback in callbacks {
                callback();
            }
        }
    }
);

pub(crate) struct DisplayLink {
    link: Retained<CADisplayLink>,
    target: Retained<Target>,
}

impl DisplayLink {
    pub(crate) fn for_window(window: &Window) -> Option<Rc<Self>> {
        let mtm = MainThreadMarker::new()?;
        let view = crate::vibrancy::content_view(window)?;
        if !view.respondsToSelector(sel!(displayLinkWithTarget:selector:)) {
            return None;
        }
        let target = Target::alloc(mtm).set_ivars(RefCell::default());
        // SAFETY: Target inherits NSObject's designated initializer.
        let target: Retained<Target> = unsafe { objc2::msg_send![super(target), init] };
        // SAFETY: the selector has CADisplayLink's one-argument callback signature.
        let link =
            unsafe { view.displayLinkWithTarget_selector(&target, sel!(displayLinkDidFire:)) };
        link.setPaused(true);
        // SAFETY: the main thread owns the target and the main run loop.
        unsafe { link.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes) };
        Some(Rc::new(Self { link, target }))
    }
}

impl RefreshDriver for DisplayLink {
    fn observe_next_frame(&self, callback: Callback) {
        self.target.ivars().borrow_mut().push(callback);
        self.link.setPaused(false);
    }
}

impl Drop for DisplayLink {
    fn drop(&mut self) {
        self.link.invalidate();
    }
}
