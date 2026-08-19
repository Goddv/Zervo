# Architecture

Zervo is one binary. It owns a winit window, runs the Servo engine inside it,
and paints its own chrome around the page with egui.

## The frame pipeline

The interesting part is that the chrome and the engine **share a single GL
context**. That is what makes the browser feel like one surface rather than a
page in a box.

```
winit window
└── WindowRenderingContext            (surfman: CGL + IOSurface + CALayer)
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

Internal pages (`zervo://settings`, `zervo://newtab`, `zervo://downloads`) are
tabs with no `WebView` at all; they are drawn by egui in the content rect, and
the blit is skipped.

## State and events

`ui.rs` is pure: it draws from `BrowserState` + `Settings` and returns a list of
`UiAction`s. `main.rs` applies them. Nothing in `ui.rs` touches the engine.

The engine talks back through `AppState`, which implements `WebViewDelegate` for
every tab. Delegate callbacks arrive on the main thread inside
`servo.spin_event_loop()`, which must be called after **every** winit event or
nothing repaints.

Because delegate callbacks fire while the engine holds its own borrows, work
that needs `&mut` state is queued (`pending_popups`, `pending_closes`,
`download_events`) and drained in `redraw`. Doing it inline is how you get a
`RefCell` panic.

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
