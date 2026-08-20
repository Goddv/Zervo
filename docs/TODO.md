# What's next

The short queue: what is actually being worked on, in the order it is likely to
be picked up. [PARITY.md](PARITY.md) is the exhaustive list of everything
missing, with where in the code each gap lives — this is the subset with an
intention behind it.

Nothing here is assigned. If you want one, say so in an issue first so two
people do not write it twice.

## Next

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
