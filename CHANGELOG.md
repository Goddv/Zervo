# Changelog

## 0.3.0 — 19 August 2026

The release where it starts behaving like a browser rather than a viewer.

### Pages can ask you things now

`show_embedder_control` wrote `alert()` to the log and dropped everything else on
the floor. Dropping one of these controls takes its default, which is "the user
cancelled", so a whole category of ordinary web behaviour quietly did nothing at
all. That one function was the reason:

- `<select>` dropdowns never opened
- file uploads were impossible
- `confirm()` was always false and `prompt()` always null
- `<input type=color>` did nothing
- right-click did nothing, anywhere
- there was no IME, so no way to type Chinese, Japanese or Korean

All of those work now. Dialogs come up over the page labelled with the origin
that raised them, because the text in them is written by the site and shouldn't
read like Zervo asking. `<select>` handles optgroups and multi-select. The file
picker is the real macOS open panel rather than something homegrown, since that's
one thing the OS does better than we would. The context menu is built from the
items Servo hands over, so back/forward/reload, copy link, open in new tab and
the rest all work off one code path. Input methods follow servoshell's approach,
including the fiddly part where dismissing an IME because focus moved must not be
reported to the page, or it blurs the element that just got focus.

### It remembers you

Cookies, logins, localStorage and the HSTS list are kept between launches. Two
separate things were stopping that, and fixing either one alone would have
changed nothing.

Servo writes its cookie jar into a config directory and skips writing entirely
when it hasn't been given one, which it hadn't. And it only writes at all when
the engine is told to shut down, which happens when the Servo handle is dropped —
except every webview holds a reference to the shared delegate, which is a
reference back to the thing that owns the engine, so nothing was ever dropped.
Both ends fixed.

One caveat worth knowing: that flush only happens on a clean quit. Servo has no
incremental save and no flush API, so a force quit still loses the session.

⌘Q also exists now. There's no menu bar to carry the standard Quit item, so
before this there was no way to quit that ran any of the shutdown at all.

### Downloads, in the builds you actually download

Servo has no download support, so this needs a patched engine, and until now that
meant building one yourself. Released builds are now compiled against a
[fork](https://github.com/Goddv/servo/tree/zervo-downloads) carrying the patch,
pinned to a revision. A plain `git clone` still builds against the published
crate with no setup, because CI appends the patch line rather than it being
committed.

### Sites stop turning you away

Servo's user agent already claims Firefox 140, but it keeps a `Servo/0.5.0` token
and leaves out `Gecko/20100101`, and enough sites match on exactly those to serve
a "browser not supported" page. Zervo now presents the plain Firefox string. It's
a setting if you'd rather be honest about it.

### Audio and video

`<video>` and `<audio>` play, in the builds you download. This needs GStreamer,
which on macOS means the official framework rather than anything you can `brew
install`, so `scripts/bundle-gstreamer.py` copies the 92 libraries and 38 plugins
it needs into the .app and rewrites their install names. The bundle grows by
about 150 MB, which is the price of not requiring everyone to install a framework
first.

Three separate things had to be right before a single frame decoded, and each one
hid the next: the binary needed an rpath of its own, `@rpath/…` references had to
be rewritten rather than left alone, and the rewritten paths had to be *shorter*
than the originals or `install_name_tool` refuses to write them at all. Build it
with `--features media`.

### Local files open

`file://` URLs went through the "looks like a hostname" branch of the address bar
and came out as `https://file:///…`, so opening a local file was impossible —
from the address bar or the command line. Absolute paths and `~/` work too.

### Page console output

There are no devtools, so `console.log` went nowhere at all. It goes to the
terminal now, which makes debugging a page in Zervo enormously less annoying.

### Commits

```
17e4900  app: show page console output
28978fc  ui: open local files
17cce01  media: make the bundled GStreamer actually load
d7357f3  media: bundle GStreamer so audio and video work
9f9d8a8  ui: let a popup survive its own opening click
51823c9  ui: wire up input methods
a07646e  ci: build releases against the patched engine
58bf8a8  ui: answer what the page asks for
636ac09  engine: keep cookies, and present as plain Firefox
8b55b80  docs: write down what is missing before this is a usable browser
184bba1  README: show the screenshots properly
```

## 0.2.0 — 19 August 2026

A round of fixes to the chrome. The autohide sidebar was the big one: it looked
fine and was completely unusable.

### Sidebar autohide

The hidden sidebar slid out when you put the pointer against the left edge, then
went away again the moment you moved toward anything in it. You could see it, you
just couldn't click it. The reveal was worked out fresh every frame from "is the
cursor within 14px of the window edge", so walking the mouse over to a tab took
you out of that strip and the sidebar was swapped straight back out for the
collapsed handle.

Now the edge only opens it. What keeps it open is the pointer being anywhere over
the sidebar itself, with a bit of slack past its right edge and a quarter second
of grace on the way out. It also won't vanish halfway through a drag, or while a
menu is open. If it's already sliding away when you go back for it, you get it
back.

Two more things fell out of that one:

- The reveal used to be a real panel, so it took up layout space. Every peek
  shrank the content area and resized the webview, which means Servo relaid out
  the entire page, just because the pointer brushed the window edge. It floats
  over the content now and nothing underneath it moves.
- Clicks and scrolls over the revealed sidebar were going through to the page as
  well, since "is the cursor over web content" was a plain rectangle test against
  the content card, and the sidebar sits inside that rectangle.

The width it opens at is whatever you last dragged the sidebar to. That's saved
between runs now (`sidebar_width` in settings.json) rather than resetting to 248
points on every launch.

⌘L did nothing at all with the sidebar hidden. The address box lives in the
sidebar, so there was nothing on screen to focus, and worse, the pending focus
request sat around and fired later at whatever random moment you next happened to
brush the window edge. It opens the sidebar now.

There was also no toggle for autohide anywhere in Settings, despite it being on
by default. It's under Appearance.

### The content card

- The shadow around the card was three concentric outlines. Each one has a hard
  edge, so together they read as bands rather than as a shadow. It's a single
  mesh with a proper falloff now.
- The rounded corners looked stair-stepped. Two things were doing it: the arcs
  were a fixed 12 segments, which works out to roughly two physical pixels per
  facet on a Retina display, and epaint doesn't antialias meshes at all. The
  segment count follows DPI now, and each arc gets retraced with an antialiased
  line on top.
- With translucent chrome you could just make out the card's square bounding box
  running past the rounded corner. The corner masks have to be fully opaque,
  since hiding the square corners of the page underneath is their whole job, so
  against 85% chrome they left a faint solid patch with a straight edge on it.
  The chrome now ramps up to opaque along the card's rounded edge and fades back
  over the next few pixels, which puts the transition on the curve where it
  belongs instead of on the box.

### Commits

```
3474c85  ui: make a revealed sidebar usable
1494d45  ui: hide the content card's square bounding box
992700c  ui: stop the content card's corners looking stair-stepped
a5430f2  ui: smooth the card shadow
```

## 0.1.0

First release. Sidebar-first chrome on top of Servo 0.5.0, with workspaces,
pinned essentials, an internal settings page and a download manager. macOS only.
