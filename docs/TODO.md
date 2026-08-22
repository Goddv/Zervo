# What's next

The short queue: what is actually being worked on, in the order it is likely to
be picked up. [PARITY.md](PARITY.md) is the exhaustive list of everything
missing, with where in the code each gap lives — this is the subset with an
intention behind it.

Nothing here is assigned. If you want one, say so in an issue first so two
people do not write it twice.

## Next

- **Themes that are not a recompile.** The material seam is real and every
  surface goes through it ([THEMING.md](THEMING.md)), but a theme is still a
  Rust `const`. What is missing is a file format, a loader, and palettes that a
  theme can set rather than only the material. The stated goal is that somebody
  can write a Windows, GTK, Android Material or Liquid Glass theme without
  touching drawing code; the engine is arranged for it and the door is not open
  yet.
- **Run the Linux and Windows builds.** They compile, package and install and
  nobody has started one. This is the single most useful thing anyone with a
  Linux box or a Windows machine can do right now, and a report that it does not
  start is as useful as one that says it does. Windows had one known failure —
  it died reading `GL_VERSION` from a context surfman's WGL backend never made
  current — and 0.4.1 moves it to ANGLE; that fix was made from a crash log and
  has not been watched.
- **Session restore.** Workspaces and tabs written out and read back at launch.
  Pure chrome work, no engine involvement, and the most-missed thing after a
  crash. `state.rs` already holds everything that needs serialising.
- **Clearing browsing data.** Cookies persist now and there is no way to get rid
  of them. `Servo::site_data_manager` has `clear_cookies`,
  `clear_session_cookies` and `clear_site_data`; Settings needs to call them.
- **A favicon store on disk.** Icons are fetched per live webview and kept in
  memory, so history and favourites show initials instead. One small cache would
  fix both lists.
- **Crash handling.** `notify_crashed` is unimplemented, so a dead content
  process is a frozen page with no explanation. Even an error page beats that.

## Soon

- **Fullscreen** (`notify_fullscreen_state_changed`) — the button on a video
  player currently does nothing useful.
- **Protocol handlers** (`request_protocol_handler`) — a delegate method with an
  empty default body, so a page offering to handle `mailto:` is ignored.
- **Notifications to the system tray.** They are shown inside the window now,
  which is the honest version of the feature rather than the expected one: a
  notification you cannot see because Zervo is behind another window is a
  notification that did not arrive. macOS wants a signed bundle and
  `UNUserNotificationCenter`. The icons, badges and images the spec allows are
  dropped too — each would need uploading as a texture.
- **The page's accessibility tree** (`notify_accessibility_tree_update`) — the
  chrome has an AccessKit tree; everything inside the page is still invisible to
  VoiceOver, which is the half that matters more.
- **Dropped files** — dragging a file onto the window should open it.
- **More widgets, and a way to write them.** The shelf takes any grid-placed
  widget; there are three. Notes, a downloads strip and a page-actions row are
  obvious next ones.
- **Three- and four-finger swipes.** Two-finger ones are in. winit surfaces
  neither of the others, so they need an AppKit event monitor, and macOS
  assigns them to Mission Control by default — so they would arrive only for
  people who have changed a system setting. Worth doing with that stated
  plainly, not worth pretending otherwise.

## Wanted, but blocked on the engine

Listed so nobody starts them expecting to finish. Details in
[PARITY.md](PARITY.md).

- Streaming video — no Media Source Extensions in Servo.
- Find in page — no find API on `WebView`.
- Pausing a download — no way to suspend a transfer, and no range-request
  restart.
- Offering to save a login you have just typed into a form — no embedder hook
  for a submitted form. *Filling* a saved one works (⌘⇧L); it was listed here as
  impossible for a long time and never was.
- Location, camera and microphone — the permission prompt is wired, but the
  engine ships no `Geolocation` and no `getUserMedia` IDL, so a page cannot ask
  for them and the prompt will never appear for those features.
- Extensions — no engine support at all.

## Housekeeping

- **The style-thread stack size never reaches a shipped binary.**
  `.cargo/config.toml` sets `SERVO_STYLE_THREAD_STACK_SIZE_KB = "2048"` because
  unoptimised Stylo frames overflow the engine's 512 KB default on complex
  pages. Cargo's `[env]` only reaches processes cargo itself starts, so it
  applies to `cargo run` and to nothing that was installed: a `.app` opened from
  the Dock, a `zervo` from a `.deb`, the Windows exe. It matters most for the
  `dev-fast` profile, which is unoptimised and which both the macOS and Linux
  workflows will happily package. The fix is to set the floor inside the program
  before the engine is built, so it travels with the binary; keep the `[env]`
  line for `cargo run`, but stop treating it as the mechanism.
- **The Windows exe has no icon or manifest of its own.** The installer's
  shortcuts point at `assets/icon/zervo.ico`, so the Start menu is right and
  Explorer is not. Embedding it needs a `build.rs` and a resource compiler —
  and the same build script is where `longPathAware` would go, which is the
  other half of making long paths work on Windows rather than relying on a
  short workspace root.
- **Nothing is signed, on any platform.** [PACKAGING.md](PACKAGING.md) has the
  numbers: $99/year for an Apple Developer ID, about $10/month for Azure
  Artifact Signing on Windows. The macOS workflow is already wired for it and
  inert without the secrets; Windows has no signing step at all yet.
- **The AUR packages are generated but never published.** Every release attaches
  a ready `PKGBUILD` and `.SRCINFO` for `zervo` and `zervo-bin`. Pushing them is
  a git push over SSH to an account CI does not have, so it stays manual — but
  nobody has done it once.
- **No automated *UI* tests.** There are sixty-seven unit tests now, and
  `zervo-core` runs its fifty-three on every pull request — but they cover
  arithmetic, colour, bytes and files, not anything drawn. [TESTING.md](TESTING.md)
  is still a checklist someone walks by hand for everything else. A harness that
  drives egui and asserts on layout (`egui_kittest`) would pay for itself
  quickly, and the sidebar refactor below is waiting on one.
- **`ui.rs` is still past three thousand lines.** The four parts of it that
  never read `ChromeContext` have left — `ui/frame.rs`, `ui/settings_page.rs`,
  `ui/backdrops.rs`, `ui/text.rs`. What is left is the sidebar, the navigation
  bar, the hover cards and the download and history pages, and the sidebar is
  the hard one: its drag-and-drop spans three functions through untyped
  `ctx.data` ids, so the compiler will not catch a reordering mistake and
  nothing else will either. Do the navigation bar and the download pages first;
  leave the sidebar until there is a UI harness.
- **`dashboard.rs` still has its own copy of the grid.** `grid.rs` now has
  fifteen tests pinning its behaviour, which is what that port was waiting for.
  The catch is `Span::Full`: resolve it to concrete cells once, up front, and
  hand `grid.rs` plain placements, rather than letting three functions each
  decide what "full" means — which is how they came to disagree.
- **`TabId` and `DownloadId` are both `u64` aliases**, so `CloseTab(TabId)` and
  `CancelDownload(u64)` are the same type and nothing stops one reaching the
  other. Two newtypes would close that for good; the compiler finds every site.
- **`apply_action` is four hundred and forty lines and forty-seven arms**, and
  thirty of them end in `request_redraw()` while seventeen deliberately do not.
  The `Settings`-only and `BrowserState`-only arms are model logic sitting in
  the event loop, and moving them down would make them testable.
- **Most corner radii are still numbers.** `theme::Tier` names six sizes and the
  material decides what each comes to, and the new tab page uses nothing else —
  but `ui.rs` and `controls.rs` still write theirs in figures. The tiers were
  picked to cover seventy of the seventy-seven radii in the tree, so the sweep
  is close to a find-and-replace. Until it happens, a second material only
  reaches half the application.
- **Changing the wallpaper is a cut, not a cross-fade.** A new picture ramps up
  from nothing while the old one is simply gone, so for the length of the fade
  the cards are frosted against a backdrop that is barely there. A real
  cross-fade means holding the outgoing texture and its backdrop, drawing both,
  and ramping one down as the other comes up — so `Backdrop` would have to
  describe two layers rather than one. Worth doing properly rather than
  approximating.
- **`Palette` has drifted into being a `Style`.** It began as colours. It now
  also carries `dark`, the card-opacity setting, the whole `Material` and the
  wallpaper to frost against — none of which are colours. The name is the only
  thing wrong: everything in there genuinely wants to reach every surface, and
  hanging it off the palette is what let all of it arrive without changing a
  single signature. Renaming touches every file, so it wants doing on its own
  and not alongside anything else.
