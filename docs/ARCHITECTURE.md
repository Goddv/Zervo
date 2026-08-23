# Architecture

Zervo is one binary and one library. `zervo-core` holds the parts that do not
know the engine exists — the theme and the glass material, the card grid, the
gesture recogniser, the atomic JSON store, and the small HTTPS client the
wallpaper fetcher rides on. The binary owns a winit window, runs the Servo
engine inside it, and paints its own chrome around the page with egui.

The split is not architectural tidiness. Compiling the binary means compiling
Servo, which is the better part of an hour, so nothing that needed it could run
on a pull request — and for a long time nothing did. `zervo-core` builds in
about ten seconds, so clippy and its tests run on every one. Anything that is
arithmetic, colour, bytes or files belongs there. The binary re-exports each of
its modules at its own root, so a call site still writes `crate::theme::Palette`
and nothing had to move to make this happen.

## The frame pipeline

The interesting part is that the chrome and the engine **share a single GL
context**. That is what makes the browser feel like one surface rather than a
page in a box.

```
winit window
└── WindowRenderingContext            (surfman: the platform's GL backend)
    ├── EguiGlow                      built on the SAME glow context
    └── OffscreenRenderingContext     an FBO sized to the content rect
        └── Servo WebView(s)          one per tab
```

Each frame, in `RunningApp::redraw`:

1. `make_current` on the rendering context.
2. `EguiGlow::run` lays out the chrome. Inside that closure we
   - resize the active `WebView` to the content rect (× DPI scale),
   - call `webview.paint()`, rendering the page into the offscreen FBO,
   - register a `PaintCallback` on egui's **background** layer that blits the
     FBO into the frame, so every widget draws on top of the page.
3. `finish_content_frame` masks the blit's square corners and draws the card's
   shadow ring.
4. `prepare_for_rendering()` binds the window framebuffer, `EguiGlow::paint`
   runs (executing the blit callback first), then `present()`.

The backend under surfman is the platform's own: CGL with IOSurface and a
CALayer on macOS, EGL on X11 or Wayland, and ANGLE — EGL over Direct3D 11 — on
Windows. Windows takes ANGLE rather than WGL (the engine's `no-wgl` feature,
which is also what Servo's own Windows builds use): surfman's WGL backend hands
back a context nobody has made current, so the first `glGetString` call kills
the process, and it needs `WGL_NV_DX_interop2`, which plenty of drivers do not
have.

WebGPU is asked for the native API on each platform — Metal, Vulkan, Direct3D
12 — rather than wgpu's first choice; see `src/gpu.rs`. The same file reads the
driver's `GL_RENDERER`, `GL_VENDOR` and `GL_VERSION` once, with the context
current, which is what the about page shows.

Two consequences worth knowing:

- **A `WebView` is always exactly the size of its `RenderingContext`.** There is
  no per-view rect. "Putting the page in a sub-rect" means sizing the offscreen
  context to that rect and blitting it there.
- **Nothing clears the window framebuffer** — not surfman, not Servo. Zervo
  clears it to transparent each frame, which is what lets the chrome be
  translucent over the macOS vibrancy layer.

## Tabs

One live `WebView` per tab, all sharing the window's offscreen context.
Switching is `show()` + `focus()` on the target and `hide()` + `blur()` +
`set_throttled(true)` on the rest; hidden views drop out of WebRender's display
list, so only the active tab costs anything to paint. A `WebView` is an
`Rc` handle — dropping the last one closes the tab.

Internal pages — `zervo://settings`, `zervo://newtab`, `zervo://history`,
`zervo://downloads`, and the four in `trouble.rs` — are tabs with no `WebView`
at all; they are drawn by egui in the content rect, and the blit is skipped.

The four trouble pages (`unsupported`, `offline`, `certificate`, `notfound`)
are the ones a load that did not work lands on. Each carries what is known
about the failure in its own query string — `zervo://unsupported?host=…&
detail=…` — rather than in a field beside the tab, for two reasons: the address
bar shows it, so it has to be something you can type back in; and it means the
page reads its own address rather than a side channel, which is how the engine
will hand the detail over on the day it can report one. Today it cannot:
`WebViewDelegate` in Servo 0.5.0 has no load-failure callback and no
certificate hook, so nothing in the tree constructs these addresses
automatically and `trouble::address` is marked as having no caller yet.

## Between two pages

`transition.rs` crosses from one page to the next, and the motion is derived
from the seam rather than configured: a card recedes, a tint dissolves, a frame
slides, a floating-chrome page passes under the glass.

The mechanism is one readback. A page is either an engine blit or an egui
drawing, and neither can be replayed after the fact — so at the one moment in
the frame where the framebuffer still holds the departing page (after
`EguiGlow::paint`, before `present`, which is where `shot.rs` also hooks in) a
scaled copy is read out, uploaded as a texture, and composited over the live
page for a couple of tenths of a second. `backdrop::PageBackdrop` does the
copying for both this and the frost; the two differ only in how large a copy
they take and how far it is blurred.

The copy is drawn in an `Area` at `Order::Background` — above the background
layer the page is painted into, below the `Order::Middle` the floating sidebar
uses — which is what puts the departing page *under* the chrome at the last
seam. It is clipped to the content rect rather than to the page rect: under
that seam the page runs on beneath the sidebar, and a readback of a composited
framebuffer cannot tell the page's pixels from the chrome's, so showing the
covered part would slide the chrome with it.

## Materials

`theme.rs` resolves a `Palette` — what colour things are — and carries a
`Material`, which is how they are built: fill, sheen, edge, lift, shadow reach,
glow, whether surfaces frost, and the corner-radius tier along with the row
height, control padding and animation time. `glass::shapes` reads all of it
from the palette it is already handed, so a material reaches every surface in
the application without a signature changing anywhere.

Surfaces have a class — `Surface::{Card, Menu, Input}` — the way an element has
one in CSS, and the class carries the weight rather than the call site. Corner
radii are named on the same principle: `Tier::{Hairline, Control, Row, Card,
Panel, Pill}`, with the material saying what each comes to. `Glass::of(surface)`
and `Glass::tier(tier)` are how to ask; `Glass::new(10)` is still there for the
few radii derived from something else — a pill whose corners are half its
height. `theme::apply` feeds egui's own widget styling from the same material,
so its popups and dropdowns are the same glass as everything drawn by hand.

This is the seam a theme for another platform is written against, and
[THEMING.md](THEMING.md) is the guide to it.

Frosting needs something from outside, because egui cannot blur a shape's
backdrop as it draws it. It does not have to: the palette carries a `Backdrop` —
an already-blurred texture, the rectangle it covers, the uv within it, a coarse
luminance map, and how far past its own edges it may be sampled — and every
glass surface inside that rectangle samples it through the same mapping. Nothing
at the call sites changes.

Three things supply one, and all three take it the same way: a copy of the
framebuffer, straight after whatever filled the content rect and before anything
sits on top of it. `src/backdrop.rs` does the taking — a `glBlitFramebuffer`
downsample, one small `glReadPixels`, a CPU blur, throttled to about eight a
second. The web page's copy is taken after the engine's blit, the new tab page's
after its backdrop is painted, and Settings' after its base and navigation
column.

The moment is the design. Shapes within a layer are drawn in the order they were
added, so taking the copy at that point is what keeps a surface from frosting
against its own previous reflection.

Two pieces of this are macOS-only and worth knowing before porting: the window's
backdrop is an `NSVisualEffectView` behind a transparent framebuffer, and the
content card's corners are *erased* from the framebuffer rather than
painted over, so the chrome can be drawn back over the hole at its own tint and
match the chrome beside it exactly. Both are described in
[THEMING.md](THEMING.md); neither is needed where the chrome is opaque.

## The new tab page

`newtab.rs` draws it: cards on a twelve-column grid, arranged by dragging them
in edit mode and remembered in the settings. `grid.rs` holds the arithmetic —
where a card of a given size and position lands, what it collides with, which
cell the pointer is over, and where the first free space is. The navigation
bar's shelf still carries its own copy of the same sums in `dashboard.rs`; it
could adopt `grid.rs`, and should.

Behind the cards it can put a photograph. `wallpaper.rs` fetches one from
Wikimedia Commons or Openverse on a thread of its own, decodes and downscales
it there too, and hands back an `egui::ColorImage`; `main.rs` uploads that as a
texture, because making one needs a context the thread does not have. The
transport is `net.rs`, which is HTTP/1.1 over the rustls Zervo already links —
GET, redirects, a counted or chunked body, a hard byte ceiling, and nothing
else. Everything a wallpaper needs to say about its licence travels with it and
is drawn under the page.

## State and events

**Nothing in `ui.rs` touches the engine.** No `servo::` call, no filesystem, no
threads, no network, no clock. That rule is real and it is kept, and it is the
one to keep keeping: if you find yourself reaching for the engine there, add a
`UiAction` instead.

It used to say `ui.rs` was *pure* — that it drew from state and returned a list
of actions. That half was never quite true. `ChromeContext` hands it `&mut
BrowserState`, `&mut Settings`, `&mut Library`, `&mut Vault` and `&mut
Controls`, and it writes through all of them: some two dozen assignments
straight into `chrome.settings.*` and `chrome.browser.*`, plus `newtab::apply`
rewriting the tile arrangement wholesale. `UiAction::SettingsChanged` does not
mean "please change this"; it means "I have changed it — write the file and
re-apply the theme."

That is an ordinary immediate-mode arrangement and it works. It is written down
here because the difference matters: you cannot exercise the chrome by feeding
it a state and reading back the actions, since half of what it did never
appears in the list.

The engine talks back through `AppState`, which implements `WebViewDelegate` for
every tab. Delegate callbacks arrive on the main thread inside
`servo.spin_event_loop()`, which must be called after **every** winit event or
nothing repaints.

Because delegate callbacks fire while the engine holds its own borrows, work
that needs `&mut` state is queued (`pending_popups`, `pending_closes`,
`download_events`) and drained in `redraw`. Doing it inline is how you get a
`RefCell` panic.

**Drain into a `Vec` first.** A `drain` iterator holds the `RefMut` for the
whole loop body, and these loop bodies re-enter the engine — adopting a popup
focuses a webview, closing a tab drops the last handle to one. Either can
dispatch a delegate callback that pushes onto the queue being drained, and that
second `borrow_mut` panics. `AppState::sync` did exactly this for two years
before anyone noticed, which is a fair measure of how narrow the window is and
how little that helps once it opens.

## Rendering the chrome

- `theme.rs` resolves a `Palette` from the theme mode, system appearance and
  accent. Everything else takes colours from it; nothing hard-codes a colour.
- `glass.rs` builds the translucent material as a shape list, so callers can
  either paint it directly or reserve a slot and backfill it once a container's
  real size is known.
- `widgets.rs` replaces egui's stock checkbox/slider/radio with controls that
  share the chrome's radii, accent and animation timings.
- `icons.rs` renders [Phosphor](https://phosphoricons.com) glyphs from a
  vendored font, so icons are crisp at any size.

Gradients over translucent chrome must interpolate in **premultiplied** space;
`theme::mix` returns an opaque colour and will silently produce a black band if
used for that.

## Downloads

See [SERVO.md](SERVO.md). In short: with the engine patch, Servo offers the
embedder any response it cannot render, keeps performing the transfer, and
streams chunks to `AppState`; `downloads.rs` writes them to a `.part` file and
renames on completion.
