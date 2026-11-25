use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    os::unix::ffi::OsStrExt,
    ptr::NonNull,
};

use objc2::MainThreadMarker;
use winit::event_loop::EventLoopProxy;

use super::{Preference, Snapshot};
use crate::WakerEvent;

// Values from the macOS <dlfcn.h>.
const RTLD_NOW: c_int = 0x2;
const RTLD_LOCAL: c_int = 0x4;

type Proxy = EventLoopProxy<WakerEvent>;
type Wake = extern "C" fn(*mut c_void);
type Start = unsafe extern "C" fn(Wake, *mut c_void) -> *mut c_void;
type Stop = unsafe extern "C" fn(*mut c_void);
type Check = unsafe extern "C" fn(*mut c_void);
type Configure = unsafe extern "C" fn(*mut c_void, u8, u32);
type ReadSnapshot = unsafe extern "C" fn(*mut c_void) -> *mut c_char;

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlerror() -> *const c_char;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn free(pointer: *mut c_void);
}

pub(crate) struct Updater {
    context: NonNull<c_void>,
    stop: Stop,
    check: Check,
    configure: Configure,
    read_snapshot: ReadSnapshot,
    _proxy: Box<Proxy>,
    _main_thread: MainThreadMarker,
}

extern "C" fn wake(context: *mut c_void) {
    // SAFETY: The native delegate only calls this before stop returns.
    let proxy = unsafe { &*context.cast::<Proxy>() };
    let _ = proxy.send_event(WakerEvent::UpdaterChanged);
}

impl Updater {
    pub fn install(proxy: Proxy) -> Option<Self> {
        let main_thread = MainThreadMarker::new()?;
        let executable = std::env::current_exe().ok()?;
        let library = executable
            .parent()?
            .parent()?
            .join("Frameworks/ZervoUpdater.dylib");
        if !library.is_file() {
            return None;
        }
        let path = CString::new(library.as_os_str().as_bytes()).ok()?;
        // SAFETY: The absolute path points into the app bundle. Mach-O code
        // signature validation applies in the hardened release build.
        // Keep the handle open because AppKit registers its Objective-C classes.
        let Some(handle) = NonNull::new(unsafe { dlopen(path.as_ptr(), RTLD_NOW | RTLD_LOCAL) })
        else {
            // SAFETY: dlerror returns NULL or a thread-local C string which
            // remains live until the next dynamic loader call on this thread.
            if let Some(error) = NonNull::new(unsafe { dlerror() }.cast_mut()) {
                log::warn!(
                    "Unable to load the updater: {}",
                    unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()
                );
            }
            return None;
        };
        // SAFETY: These names and ABIs are defined in platform/macos/ZervoUpdater.m.
        // Resolve all functions before passing the event proxy to the library.
        let (start, stop, check, configure, read_snapshot) = unsafe {
            let symbol = |name: &CStr| NonNull::new(dlsym(handle.as_ptr(), name.as_ptr()));
            let start =
                std::mem::transmute::<*mut c_void, Start>(symbol(c"zervo_updater_start")?.as_ptr());
            let stop =
                std::mem::transmute::<*mut c_void, Stop>(symbol(c"zervo_updater_stop")?.as_ptr());
            let check =
                std::mem::transmute::<*mut c_void, Check>(symbol(c"zervo_updater_check")?.as_ptr());
            let configure = std::mem::transmute::<*mut c_void, Configure>(
                symbol(c"zervo_updater_configure")?.as_ptr(),
            );
            let read_snapshot = std::mem::transmute::<*mut c_void, ReadSnapshot>(
                symbol(c"zervo_updater_snapshot")?.as_ptr(),
            );
            (start, stop, check, configure, read_snapshot)
        };
        let mut proxy = Box::new(proxy);
        // SAFETY: The box keeps this address live until stop unregisters it.
        let context = NonNull::new(unsafe { start(wake, (&mut *proxy as *mut Proxy).cast()) })?;
        Some(Self {
            context,
            stop,
            check,
            configure,
            read_snapshot,
            _proxy: proxy,
            _main_thread: main_thread,
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        // SAFETY: The native bridge returns a strdup allocation or NULL.
        unsafe {
            let Some(json) = NonNull::new((self.read_snapshot)(self.context.as_ptr())) else {
                return Snapshot::default();
            };
            let snapshot = serde_json::from_slice(CStr::from_ptr(json.as_ptr()).to_bytes());
            free(json.as_ptr().cast());
            snapshot.unwrap_or_default()
        }
    }

    pub fn check(&self) {
        // SAFETY: The handle remains live on the main thread until Drop.
        unsafe { (self.check)(self.context.as_ptr()) };
    }

    pub fn configure(&self, preference: Preference) {
        let (operation, value) = match preference {
            Preference::AutomaticChecks(enabled) => (0, u32::from(enabled)),
            Preference::AutomaticInstall(enabled) => (1, u32::from(enabled)),
            Preference::CheckInterval(seconds) => (2, seconds),
            Preference::SystemNotifications(enabled) => (3, u32::from(enabled)),
            Preference::RemindTomorrow => (4, 0),
        };
        // SAFETY: The handle remains live on the main thread until Drop.
        // The operation values match the switch in ZervoUpdater.m.
        unsafe { (self.configure)(self.context.as_ptr(), operation, value) };
    }
}

impl Drop for Updater {
    fn drop(&mut self) {
        // SAFETY: stop consumes the retained handle and clears its callback
        // before the event proxy can be dropped.
        unsafe { (self.stop)(self.context.as_ptr()) };
    }
}
