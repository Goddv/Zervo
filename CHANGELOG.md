# Changelog

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
