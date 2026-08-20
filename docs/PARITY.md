# Parity

What Zervo is missing before it behaves like a browser people can use, roughly in
the order the gaps hurt. Most of it is embedder work that belongs here rather than
in the engine — the delegate methods exist and are simply not implemented yet.

Nothing below is a Servo limitation unless it says so.

## Already there

Navigation and history, tabs and workspaces, favicons, page titles, cursors,
`window.open` (as a new tab), clipboard, copy/cut/paste, ⌘Q, and the theme
propagated into `prefers-color-scheme`.

As of 0.3.0, also: `alert`/`confirm`/`prompt`, `<select>` popups, `<input
type=color>`, `<input type=file>` through the system open panel, the context
menu, input methods, cookies and logins surviving a restart, `file://` URLs,
page console output to the log, and — in released builds — file downloads and
audio and video playback.

Since then: HTTP authentication (`request_authentication`), page zoom on ⌘+, ⌘-
and ⌘0, the address bar following same-document navigation
(`notify_url_changed`, so `pushState` no longer leaves it stale), media session
metadata and playback actions (`notify_media_session_event`, which is what the
player widgets drive), a searchable history, favourites, and a login vault kept
in the system keychain.

Tier 0 is done. What is left is the list below.

## Streaming video does not work

Local and progressive `<video>` plays. YouTube, Netflix, Twitch and most other
video sites do not, and report "this browser can't play video". Two engine-side
reasons, measured from a page in Zervo:

- **`MediaSource` is undefined.** Servo has no Media Source Extensions, and
  adaptive streaming is built on it. This alone is enough for YouTube to refuse
  before it tries anything.
- **`canPlayType('video/mp4')` returns `""`**, so H.264 is advertised as
  unsupported and sites that feature-detect will not offer it — even though the
  bundled GStreamer decodes H.264 perfectly well, which is easy to confirm with
  a local `.mp4`. WebM/VP9, Ogg/Theora and Vorbis all report `probably`.

Neither is fixable from the embedder side. `navigator.requestMediaKeySystemAccess`
is also absent, so anything DRM-protected is out regardless.

## Tier 1 — expected behaviour, and the API already exists

Each of these is a delegate method or a `WebView` call away.

- **Crash handling.** `notify_crashed` is not implemented, so a crashed content
  process is a frozen page with no explanation.
- **Fullscreen.** `notify_fullscreen_state_changed` is not implemented, so the
  fullscreen button on a video player does nothing useful.
  `WebView::exit_fullscreen` is there for the way back out.
- **Permissions.** `request_permission` is not implemented. Geolocation,
  notifications, camera and microphone requests never reach the user.
- **The link target on hover.** `notify_status_text_changed` and
  `WebView::status_text` exist; the usual bottom-left overlay does not.
- **Clearing browsing data.** `Servo::site_data_manager` offers `clear_cookies`,
  `clear_session_cookies` and `clear_site_data`, and nothing in Settings calls
  them. Now that cookies persist, there is no way to get rid of them.
- **Dropped files.** Dragging a file onto the window should open it.
- **Extensions.** The button in the navigation bar is deliberately dead: the
  engine has no extension support at all.
- **`screen.availWidth`/`availHeight`** report the whole display rather than
  subtracting the menu bar and Dock.
- **Filling passwords into web forms.** There is no embedder hook for a
  submitted form and no way to write into a page's fields, so saved logins can
  only be used for HTTP authentication. If Servo grows a credential API, that is
  where filling would attach.
- **Session restore.** Pure chrome work, no engine involvement: workspaces and
  tabs written out and read back at launch.
- **Favicons for history and favourites.** They are fetched per live webview and
  kept in memory, so a saved page has no icon until it is open. A small on-disk
  icon store would fix both lists at once.

## Tier 2 — needs engine work, or is genuinely expensive

- **Find in page.** There is no find API on `WebView` in Servo 0.5.0 at all.
  This cannot be done from the embedder side and needs engine work first.
- **Pausing and resuming a download.** The transfer is an HTTP response being
  streamed to disk with no way to suspend it and no range-request restart, so
  the downloads card offers stop and start-again and nothing between. Real
  pause/resume means teaching the fetch layer about it.
- **Web notifications** (`show_notification`) and **protocol handlers**
  (`request_protocol_handler`).
- **Accessibility.** `notify_accessibility_tree_update` is not implemented, so
  VoiceOver sees nothing.
- **Devtools.** Console output goes to Zervo's log and nowhere the user can see;
  there is no inspector, network panel or JavaScript console in the browser.
- **Extensions, sync, profiles.** Not planned.

## Known rough edges

- **Cookies are only written when Servo shuts down cleanly.** That is how Servo
  works: the jar is flushed when the constellation is told to exit, and there is
  no incremental save and no flush API. A force quit or a crash loses the
  session.
- **Two dialogs at once.** Only the most recent page-initiated control is drawn;
  anything queued behind it waits. A page cannot reasonably need two answers at
  once, and stacked modals are worse than a queue.
- **Linux and Windows have never been run.** The packages build and install;
  whether the window opens is unknown. See the platform table in the README.

## A note on where fixes go

Downloads needed an engine patch, and find-in-page needs one. When the gap really
is in the engine, contribute it to [Servo][servo] directly — but note that Servo
does not accept AI-generated contributions, and check whether the work is already
assigned before starting.

Released builds compile against [a fork][fork] carrying the download patch,
pinned to a revision in `.github/workflows/macos.yml`. See [SERVO.md](SERVO.md)
for building against it yourself.

[fork]: https://github.com/Goddv/servo/tree/zervo-downloads
[servo]: https://servo.org
