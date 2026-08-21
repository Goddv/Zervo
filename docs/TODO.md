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
  start is as useful as one that says it does.
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
- **Permission prompts** (`request_permission`) — geolocation, notifications,
  camera and microphone never reach the user.
- **The link target on hover** — `notify_status_text_changed` and
  `WebView::status_text` are both there; the bottom-left overlay is not.
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
- Filling saved logins into web forms — no embedder hook for a submitted form.
- Extensions — no engine support at all.

## Housekeeping

- **No automated UI tests.** [TESTING.md](TESTING.md) is a checklist someone
  walks by hand. A harness that drives egui and asserts on layout would pay for
  itself quickly.
- **`ui.rs` is past four thousand lines.** The cards, the navigation bar and the
  settings page each want to be their own module.
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
