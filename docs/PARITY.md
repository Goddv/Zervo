# Parity

What Zervo is missing before it behaves like a browser people can use, roughly in
the order the gaps hurt. Most of it is embedder work that belongs here rather than
in the engine — the delegate methods exist and are simply not implemented yet.

Nothing below is a Servo limitation unless it says so.

## Already there

Navigation and history, tabs and workspaces, favicons, page titles, cursors,
`window.open` (as a new tab), clipboard (Servo's default delegate, `clipboard`
feature is on), copy/cut/paste shortcuts, theme propagated into
`prefers-color-scheme`, and file downloads behind `--features engine-downloads`.

## Tier 0 — the things that make it feel broken

### Downloads are off in the default build

Clicking a release asset on GitHub, or any other `Content-Disposition: attachment`
response, does nothing at all in a stock build. The three delegate methods that
handle it (`notify_unsupported_response`, `notify_response_chunk`,
`notify_response_eof` in `src/app.rs`) are behind `engine-downloads`, which needs a
patched engine because Servo has no download support of its own yet
([servo#40210][downloads-issue]).

Until that lands upstream, the fix is to keep a fork of Servo with
`patches/servo/0001-embedder-file-downloads.patch` applied, pin it, and build
releases against it. See [SERVO.md](SERVO.md).

### Nothing is remembered between launches

Cookies, HTTP auth and the HSTS list are all lost when you quit, so every login
has to be redone. Two separate causes, and both need fixing:

1. `ServoBuilder::default()` in `src/main.rs` never sets `Opts`, so `config_dir`
   is `None`. Servo writes `cookie_jar.json`, `auth_cache.json` and
   `hsts_list.json` into that directory and skips writing entirely when it is
   unset.
2. Servo only flushes them when the constellation gets `Exit`, which
   `impl Drop for ServoInner` sends when the `Servo` handle is dropped. Zervo
   attaches its delegate with `.delegate(self.clone())` on an `Rc<AppState>` that
   also owns the `Servo` handle, so the `Rc` is in a cycle and never reaches zero.
   Nothing is ever dropped, so nothing is ever written.

Fixing (1) alone changes nothing. The delegate wants to be a `Weak`, or the
webviews and the engine handle need dropping explicitly on exit.

### Dialogs, pickers and the context menu do nothing

`show_embedder_control` in `src/app.rs` logs `alert()` and drops everything else,
and dropping an `EmbedderControl` takes its safe default, which is "cancel". That
one function is the reason all of these are silently dead:

| Control | What the user sees |
|---|---|
| `SelectElement` | `<select>` dropdowns do not open |
| `FilePicker` | file uploads are impossible |
| `SimpleDialog` | `confirm()` is always false, `prompt()` always null, `alert()` invisible |
| `ColorPicker` | `<input type=color>` does nothing |
| `ContextMenu` | right-click does nothing anywhere |
| `InputMethod` | no IME, so no CJK input |

`hide_embedder_control` is not implemented either, so anything that does get shown
has no way to be dismissed by the engine.

This is the single highest-value thing to work on. It is pure chrome work, it
needs no engine changes, and each control can land separately.

### No audio or video

The `media-gstreamer` feature is not enabled on the `servo` dependency, so
`<video>` and `<audio>` do not play. Turning it on means a GStreamer dependency and
bundling its libraries into the `.app`, which is the reason it is off.

### Sites refuse the user agent

`Preferences::user_agent` is left at its default. A surprising number of sites
serve an error page or a "browser not supported" wall to anything they do not
recognise, which makes the engine look worse than it is. Overriding this is one
line and probably the cheapest possible improvement to how many sites work.

## Tier 1 — expected behaviour, and the API already exists

Each of these is a delegate method or a `WebView` call away.

- **Crash handling.** `notify_crashed` is not implemented, so a crashed content
  process is a frozen page with no explanation.
- **Fullscreen.** `notify_fullscreen_state_changed` is not implemented, so the
  fullscreen button on a video player does nothing useful. `WebView::exit_fullscreen`
  is there for the way back out.
- **Permissions.** `request_permission` is not implemented. Geolocation,
  notifications, camera and microphone requests never reach the user.
- **HTTP authentication.** `request_authentication` is not implemented, so
  anything behind basic auth is unreachable.
- **Zoom.** `WebView::set_page_zoom` exists and nothing calls it. ⌘+, ⌘- and ⌘0.
- **The link target on hover.** `notify_status_text_changed` and
  `WebView::status_text` exist; the usual bottom-left overlay does not.
- **The address bar during same-document navigation.** `notify_url_changed` is not
  implemented; the URL is currently taken from history changes, so `pushState`
  navigations can leave it stale.
- **Session restore.** Pure chrome work, no engine involvement: workspaces and
  tabs written out and read back at launch.

## Tier 2 — needs engine work, or is genuinely expensive

- **Find in page.** There is no find API on `WebView` in Servo 0.5.0 at all. This
  cannot be done from the embedder side and needs engine work first.
- **Web notifications** (`show_notification`), **protocol handlers**
  (`request_protocol_handler`), **media session** (`notify_media_session_event`).
- **Accessibility.** `notify_accessibility_tree_update` is not implemented, so
  VoiceOver sees nothing.
- **Devtools.** `show_console_message` is not implemented, so page console output
  is invisible.
- **Extensions, sync, profiles.** Not planned.

## A note on where fixes go

Downloads needed an engine patch. Find-in-page needs one. When the gap really is
in the engine, contribute it to [Servo][servo] directly — but note that Servo does
not accept AI-generated contributions, and check whether the work is already
assigned before starting.

Everything in Tier 0 and Tier 1 above is embedder work and belongs in this repo.

[downloads-issue]: https://github.com/servo/servo/issues/40210
[servo]: https://servo.org
