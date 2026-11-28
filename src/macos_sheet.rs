//! Optional Swift sheets bundled with the macOS app.

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

fn bridge(
    cache: &'static OnceLock<Option<Bridge>>,
    filename: &str,
    open_symbol: &[u8],
    available_symbol: &[u8],
) -> Option<&'static Bridge> {
    MainThreadMarker::new()?;
    cache
        .get_or_init(|| {
            let executable = std::env::current_exe().ok()?;
            let path = executable
                .parent()?
                .parent()?
                .join("Frameworks")
                .join(filename);
            // SAFETY: This is the signed bridge bundled with Heion. On a
            // system without its framework, the optional load fails cleanly.
            let library = unsafe { libloading::Library::new(path) }.ok()?;
            // SAFETY: The Swift source declares these exact C symbols and ABIs.
            let (open, available) = unsafe {
                (
                    *library.get::<Open>(open_symbol).ok()?,
                    *library.get::<Available>(available_symbol).ok()?,
                )
            };
            // AppKit and Swift retain class metadata for the process lifetime.
            std::mem::forget(library);
            Some(Bridge { open, available })
        })
        .as_ref()
}

fn wallpaper_bridge() -> Option<&'static Bridge> {
    static CACHE: OnceLock<Option<Bridge>> = OnceLock::new();
    bridge(
        &CACHE,
        "HeionImagePlayground.dylib",
        b"heion_playground_open\0",
        b"heion_playground_available\0",
    )
}

type Generate = unsafe extern "C" fn(*const u8, isize, *mut c_void, Completion) -> bool;
type Dictate = unsafe extern "C" fn(*mut c_void, Completion, Completion) -> bool;
type End = unsafe extern "C" fn();

struct WidgetBridge {
    available: Available,
    generate: Generate,
    cancel_generation: End,
    dictate: Dictate,
    stop_dictation: End,
    cancel_dictation: End,
}

fn widget_bridge() -> Option<&'static WidgetBridge> {
    static CACHE: OnceLock<Option<WidgetBridge>> = OnceLock::new();
    MainThreadMarker::new()?;
    CACHE
        .get_or_init(|| {
            let exe = std::env::current_exe().ok()?;
            let path = exe
                .parent()?
                .parent()?
                .join("Frameworks/HeionWidgetPlayground.dylib");
            // SAFETY: The bundled library declares these C functions in
            // HeionWidgetPlayground.swift and remains loaded for its callbacks.
            let library = unsafe { libloading::Library::new(path).ok()? };
            let bridge = unsafe {
                WidgetBridge {
                    available: *library.get(b"heion_widget_playground_available\0").ok()?,
                    generate: *library.get(b"heion_widget_playground_generate\0").ok()?,
                    cancel_generation: *library.get(b"heion_widget_playground_cancel\0").ok()?,
                    dictate: *library.get(b"heion_widget_speech_start\0").ok()?,
                    stop_dictation: *library.get(b"heion_widget_speech_stop\0").ok()?,
                    cancel_dictation: *library.get(b"heion_widget_speech_cancel\0").ok()?,
                }
            };
            std::mem::forget(library);
            Some(bridge)
        })
        .as_ref()
}

pub fn wallpaper_available() -> bool {
    // SAFETY: The bridge checks the main thread and keeps the library loaded.
    wallpaper_bridge().is_some_and(|bridge| unsafe { (bridge.available)() })
}

pub fn widget_available() -> bool {
    // SAFETY: The bridge checks the main thread and keeps the library loaded.
    widget_bridge().is_some_and(|bridge| unsafe { (bridge.available)() })
}

type Reply = Result<Option<Vec<u8>>, ()>;

struct Pending {
    proxy: Proxy,
    byte_limit: usize,
    event: fn(Reply) -> WakerEvent,
}

pub fn open_wallpaper(window: &Window, proxy: Proxy) -> bool {
    open(
        wallpaper_bridge(),
        window,
        Pending {
            proxy,
            byte_limit: 33_554_432,
            event: |result| {
                WakerEvent::WallpaperGenerated(
                    result.map_err(|()| crate::newtab_wallpaper::Error::Format),
                )
            },
        },
    )
}

struct WidgetRequest {
    proxy: Proxy,
    ticket: u64,
}

pub fn generate_widget(prompt: &str, ticket: u64, proxy: Proxy) -> bool {
    if prompt.is_empty() || prompt.chars().count() > crate::widget_composer::PROMPT_LIMIT {
        return false;
    }
    let Some(bridge) = widget_bridge() else {
        return false;
    };
    let context = Box::into_raw(Box::new(WidgetRequest { proxy, ticket }));
    // SAFETY: The bridge copies the borrowed UTF-8 synchronously. Acceptance
    // transfers the context to the exactly-once completion on the main thread.
    let started = unsafe {
        (bridge.generate)(
            prompt.as_ptr(),
            prompt.len() as isize,
            context.cast(),
            widget_generated,
        )
    };
    if !started {
        // SAFETY: A declined request leaves the context owned by this call.
        drop(unsafe { Box::from_raw(context) });
    }
    started
}

pub fn cancel_widget_generation() {
    if let Some(bridge) = widget_bridge() {
        // SAFETY: widget_bridge restricts calls to the main thread.
        unsafe { (bridge.cancel_generation)() };
    }
}

pub fn start_widget_dictation(ticket: u64, proxy: Proxy) -> bool {
    let Some(bridge) = widget_bridge() else {
        return false;
    };
    let context = Box::into_raw(Box::new(WidgetRequest { proxy, ticket }));
    // SAFETY: On acceptance, Swift borrows the context for each transcript
    // update and passes it to the final callback exactly once on the main thread.
    let started =
        unsafe { (bridge.dictate)(context.cast(), transcript_update, transcript_finished) };
    if !started {
        // SAFETY: A declined request leaves the context owned by this call.
        drop(unsafe { Box::from_raw(context) });
    }
    started
}

pub fn stop_widget_dictation() {
    if let Some(bridge) = widget_bridge() {
        // SAFETY: widget_bridge restricts calls to the main thread.
        unsafe { (bridge.stop_dictation)() };
    }
}

pub fn cancel_widget_dictation() {
    if let Some(bridge) = widget_bridge() {
        // SAFETY: widget_bridge restricts calls to the main thread.
        unsafe { (bridge.cancel_dictation)() };
    }
}

extern "C" fn widget_generated(context: *mut c_void, bytes: *const u8, length: isize) {
    // SAFETY: Swift transfers the boxed context once. A positive length
    // describes the immutable bytes borrowed during this invocation.
    let context = unsafe { Box::from_raw(context.cast::<WidgetRequest>()) };
    let result = match length {
        0 => Ok(None),
        1..=131_072 if !bytes.is_null() => {
            // SAFETY: The bridge supplies this many readable bytes for the call.
            Ok(Some(
                unsafe { std::slice::from_raw_parts(bytes, length as usize) }.to_vec(),
            ))
        },
        _ => Err(()),
    };
    let _ = context.proxy.send_event(WakerEvent::CustomWidgetGenerated {
        ticket: context.ticket,
        result,
    });
}

extern "C" fn transcript_update(context: *mut c_void, bytes: *const u8, length: isize) {
    // SAFETY: Swift sends updates serially on the main thread before returning
    // the context in transcript_finished.
    let context = unsafe { &*context.cast::<WidgetRequest>() };
    send_transcript(context, bytes, length, false);
}

extern "C" fn transcript_finished(context: *mut c_void, bytes: *const u8, length: isize) {
    // SAFETY: Swift transfers the boxed context once and emits no more updates.
    let context = unsafe { Box::from_raw(context.cast::<WidgetRequest>()) };
    send_transcript(&context, bytes, length, true);
}

fn send_transcript(context: &WidgetRequest, bytes: *const u8, length: isize, done: bool) {
    let text = match length {
        0 => Ok(None),
        1..=8192 if !bytes.is_null() => {
            // SAFETY: The bridge supplies this many readable bytes for the call.
            let bytes = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
            std::str::from_utf8(bytes)
                .map(|text| {
                    Some(
                        text.chars()
                            .take(crate::widget_composer::PROMPT_LIMIT)
                            .collect(),
                    )
                })
                .map_err(|_| "newtab-dictation-failed")
        },
        -2 => Err("newtab-dictation-microphone"),
        -3 => Err("newtab-dictation-permission"),
        -4 => Err("newtab-dictation-unavailable"),
        -5 => Err("newtab-dictation-empty"),
        _ => Err("newtab-dictation-failed"),
    };
    let _ = context.proxy.send_event(WakerEvent::WidgetTranscript {
        ticket: context.ticket,
        text,
        done,
    });
}

fn open(bridge: Option<&Bridge>, window: &Window, pending: Pending) -> bool {
    let Some(bridge) = bridge else {
        return false;
    };
    let Some(window) = crate::vibrancy::content_view(window).and_then(|view| view.window()) else {
        return false;
    };
    let context = Box::into_raw(Box::new(pending));
    // SAFETY: The retained NSWindow is live for this call and the accepted
    // Swift sheet retains it. A successful open transfers the boxed context
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
    // SAFETY: An accepted Swift sheet returns this boxed context once. Its data
    // is immutable and borrowed for this callback, so copy it before returning.
    let pending = unsafe { Box::from_raw(context.cast::<Pending>()) };
    let result = match length {
        0 => Ok(None),
        length if length > 0 && length as usize <= pending.byte_limit && !bytes.is_null() => Ok(
            Some(unsafe { std::slice::from_raw_parts(bytes, length as usize) }.to_vec()),
        ),
        _ => Err(()),
    };
    let _ = pending.proxy.send_event((pending.event)(result));
}
