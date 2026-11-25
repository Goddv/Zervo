# Native Linux frontends

Zervo can share browser behavior, profile data and visual identity across the
current macOS app and Linux frontends. Native widgets should own each Linux
window, the browser chrome, input, dialogs and accessibility. Servo should
render the page in the space provided by the toolkit.

## Reusable work

- [servo-gtk](https://github.com/nacho/servo-gtk) has a Rust GTK4 widget,
  OpenGL rendering, HiDPI, keyboard and navigation support, and native dialogs.
  Its Servo runner re-executes the host binary in a subprocess. Its current
  [Servo pin](https://raw.githubusercontent.com/nacho/servo-gtk/main/Cargo.toml)
  differs from Zervo's. The first prototype needs one shared Servo revision and
  a decision about where Zervo's download and notification delegates run.
- [KDAB's Qt demo](https://github.com/KDABLabs/cxx-qt-servo-webview) gives us
  the shape of the QML item, CXX-Qt bindings and frame handoff. Its documented
  setup targets Wayland and uses a 2024 Rust nightly. The rendering path uses
  `QQuickFramebufferObject` and OpenGL. Port the viewport to Zervo's current
  Servo API and test Qt's render thread and frame lifetime before wiring the
  rest of the browser. [KDAB integration walkthrough](https://www.kdab.com/embedding-servo-in-qt/)

## Port sequence

1. Move tab, Space, folder, history, download and settings operations out of
   the window and rendering state where they are coupled today. Give
   frontends the same browser commands, observable state and profile schema.
2. Start with a GTK4 slice that opens a real page, switches tabs and restores a
   Space after restart. Keep the page frame, tab rows and sidebar proportion
   familiar. Use GTK for focus, shortcuts, native menus and dialogs.
3. Exercise resize, HiDPI, IME, clipboard, downloads, and accessibility under
   Wayland and X11. Check that scrolling and sidebar transitions avoid copying
   the page back to CPU memory each frame. Publish a Linux build once this
   slice is reliable on a real desktop.
4. Add Qt Quick for KDE against the same contracts. Ship toolkit-specific
   Linux artifacts under the Zervo name with compatible profiles. The macOS
   app can keep AppKit controls and Liquid Glass.

Use shared labels, sizing, semantic colours and navigation behavior to give
all frontends a consistent identity. Let the toolkit supply each platform's
window chrome, accessibility, theme and input behavior. GTK and Qt ports
still need Linux runtime verification.
