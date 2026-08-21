//! What is actually drawing the page.
//!
//! The about page used to claim "WebGPU via Metal" on every platform, which
//! was true on one of them. Both halves are answered properly here: the
//! compositor's strings come from the driver itself, once, while the window's
//! GL context is current; the WebGPU backend is the one Zervo asks the engine
//! for, which is the native API of the platform rather than whatever wgpu's
//! PRIMARY set happens to pick first.

use std::sync::OnceLock;

/// The `dom.webgpu.wgpu_backend` value for this platform: Metal on macOS,
/// Direct3D 12 on Windows, Vulkan everywhere else.
///
/// Windows could equally use Vulkan, but D3D12 is the API its drivers are
/// tuned for and the one the compositor's ANGLE already sits on.
pub const fn webgpu_backend() -> &'static str {
    if cfg!(target_os = "macos") {
        "metal"
    } else if cfg!(target_os = "windows") {
        "dx12"
    } else {
        "vulkan"
    }
}

/// The same backend, spelled for a human.
pub const fn webgpu_backend_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Metal"
    } else if cfg!(target_os = "windows") {
        "Direct3D 12"
    } else {
        "Vulkan"
    }
}

/// What the driver says about itself. Empty strings mean it was never asked —
/// only possible before the window exists.
#[derive(Clone, Default)]
pub struct Adapter {
    /// `GL_RENDERER`: the GPU, or the software rasteriser standing in for one.
    pub renderer: String,
    /// `GL_VENDOR`.
    pub vendor: String,
    /// `GL_VERSION`, which names the API (OpenGL, OpenGL ES, ANGLE) too.
    pub version: String,
}

static ADAPTER: OnceLock<Adapter> = OnceLock::new();

/// Read the driver's strings. Must be called with the context current; calling
/// it twice is harmless and the first answer wins.
pub fn record(gl: &glow::Context) {
    use glow::HasContext as _;

    // SAFETY: a GL context is current on this thread — the caller has just
    // made it so — and these are three constant queries with no side effects.
    let adapter = unsafe {
        Adapter {
            renderer: gl.get_parameter_string(glow::RENDERER),
            vendor: gl.get_parameter_string(glow::VENDOR),
            version: gl.get_parameter_string(glow::VERSION),
        }
    };
    let _ = ADAPTER.set(adapter);
}

/// What was recorded, or blanks if nothing was.
pub fn adapter() -> Adapter {
    ADAPTER.get().cloned().unwrap_or_default()
}
