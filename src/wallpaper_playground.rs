//! The optional Swift bridge to Apple's on-device Image Playground sheet.

use std::ffi::c_void;
use std::sync::OnceLock;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

use crate::WakerEvent;

type Proxy = EventLoopProxy<WakerEvent>;
type Completion = extern "C" fn(*mut c_void, *const u8, isize);
type Open = unsafe extern "C" fn(*mut c_void, *mut c_void, Completion) -> bool;
type Available = unsafe extern "C" fn() -> bool;

struct Bridge {
    open: Open,
    available: Available,
}

fn bridge() -> Option<&'static Bridge> {
    MainThreadMarker::new()?;
    static BRIDGE: OnceLock<Option<Bridge>> = OnceLock::new();
    BRIDGE
        .get_or_init(|| {
            let executable = std::env::current_exe().ok()?;
            let path = executable
                .parent()?
                .parent()?
                .join("Frameworks/HeionImagePlayground.dylib");
            // SAFETY: This is the signed bridge bundled with Heion. On a
            // system without its framework, the optional load fails cleanly.
            let library = unsafe { libloading::Library::new(path) }.ok()?;
            // SAFETY: The Swift source declares these exact C symbols and ABIs.
            let (open, available) = unsafe {
                (
                    *library.get::<Open>(b"heion_playground_open\0").ok()?,
                    *library
                        .get::<Available>(b"heion_playground_available\0")
                        .ok()?,
                )
            };
            // AppKit and Swift retain class metadata for the process lifetime.
            std::mem::forget(library);
            Some(Bridge { open, available })
        })
        .as_ref()
}

pub fn available() -> bool {
    // SAFETY: bridge() verifies the main thread and retains the library.
    bridge().is_some_and(|bridge| unsafe { (bridge.available)() })
}

pub fn open(window: &Window, proxy: Proxy) -> bool {
    let Some(bridge) = bridge() else {
        return false;
    };
    let Some(window) = crate::vibrancy::content_view(window).and_then(|view| view.window()) else {
        return false;
    };
    let context = Box::into_raw(Box::new(proxy));
    // SAFETY: The retained NSWindow is live for this call and the accepted
    // Swift sheet retains it. A successful open transfers the boxed proxy
    // to the exactly-once completion; a declined open leaves it with us.
    let accepted = unsafe {
        (bridge.open)(
            Retained::as_ptr(&window).cast_mut().cast(),
            context.cast(),
            complete,
        )
    };
    if !accepted {
        // SAFETY: The Swift bridge never calls back after declining.
        drop(unsafe { Box::from_raw(context) });
    }
    accepted
}

extern "C" fn complete(context: *mut c_void, bytes: *const u8, length: isize) {
    // SAFETY: An accepted Swift sheet returns this boxed proxy once. Its data
    // is immutable and borrowed for this callback, so copy it before returning.
    let proxy = unsafe { Box::from_raw(context.cast::<Proxy>()) };
    let result = match length {
        0 => Ok(None),
        1..=33_554_432 if !bytes.is_null() => Ok(Some(
            unsafe { std::slice::from_raw_parts(bytes, length as usize) }.to_vec(),
        )),
        _ => Err(crate::newtab_wallpaper::Error::Format),
    };
    let _ = proxy.send_event(WakerEvent::WallpaperGenerated(result));
}
