# Smoke checklist

There is no automated UI test suite yet (contributions very welcome). After an
engine bump or a chrome change, walk this list.

## Chrome
- [ ] Window opens; sidebar, search pill and traffic lights are laid out sanely.
- [ ] Sidebar toggle collapses and restores; with autohide on, pushing the
      pointer to the left edge reveals it.
- [ ] Tabs: open (⌘T), close (⌘W), switch, right-click → Pin as Essential.
- [ ] Workspaces: create, switch, tab counts update.
- [ ] Address bar: ⌘L focuses; a URL navigates; a phrase searches.
- [ ] Settings (⌘,): every section renders; toggles/sliders/segments respond.
      Changing one does not make the chrome or the page jolt.
- [ ] Theme: Auto/Light/Dark switch cleanly, including the titlebar; accent
      colours retint the chrome; pages see `prefers-color-scheme`.
- [ ] New tab page: each of the eight backdrops; the animated ones do not pin
      the CPU when the window is idle. The page does not blink or flicker while
      it is simply sitting there.

## Material
- [ ] Solid and Frosted both apply to everything: the window, cards, menus,
      dropdowns, tooltips, the widget shelf, hover cards. Nothing is left an
      opaque slab in a window made of glass.
- [ ] A hover card (favourites, downloads) over a *web page* is frosted, not a
      flat wash — the page behind it is visibly softened. Over the new tab page
      and over Settings too, and with the widget shelf open, when the card hangs
      above the page rather than on it.
- [ ] Dark mode over a bright wallpaper or a white page: cards stay dark and
      their text stays readable. Light mode over a dark one: the mirror of it.
- [ ] Text on the new tab page — card rows, the greeting, the clock, the photo
      credit — is readable over the light and dark parts of the same
      photograph.
- [ ] Content card corners: with outline, shadow and halo all off, no seam,
      notch or pale patch at any of the four, on a web page and on an internal
      one. Worth a screen magnifier.
- [ ] Appearance → outline, shadow, halo: each toggles, the two amounts move
      what they say, and the halo's tint switches between accent and chrome.

## New tab page
- [ ] Customise: cards gain an edge, a drag mark, a ✕ and a resize corner;
      Done puts them back. Outside edit mode a card's own links and buttons
      work and the card does not move.
- [ ] Drag a card by its middle and by a far corner: it follows the pointer
      from the point it was grabbed, not from its centre, and lands where the
      outline says.
- [ ] Resize from the corner by whole cells; the preview matches the result.
- [ ] Add card: every kind can be added, lands somewhere free, and shows an
      empty state rather than a blank rectangle when it has nothing.
- [ ] Reset restores the default arrangement.
- [ ] Shrink the window until the grid gives way to the compact clock and
      search box, and widen it again.
- [ ] Fill more rows than fit: the page scrolls, and a card scrolled off the
      top does not spill over the header.
- [ ] Two cards of the same kind side by side: no egui "widget ID" banner.
- [ ] Wallpaper → each source fetches; the credit line names the picture, the
      photographer and the licence, and clicking it opens the source page.
- [ ] Wallpaper → Choose a file… takes a local picture.
- [ ] Pull the network cable and shuffle: the page keeps its backdrop and
      Settings → Wallpaper says what failed.
- [ ] With a photograph up, collapse the sidebar and drag the bar taller: the
      picture does not rescale, and drifts down more slowly than the page.
- [ ] Over a bright photograph in the light theme: the header pills, the
      credit line and every card are readable, and no card has a bright rim
      along its underside.
- [ ] The note and the to-do list survive a restart.

## The chrome's own animations

These only exist in the middle of themselves, so the harness drives them rather
than waiting to catch one. Debug builds only.

- `ZERVO_SHOT_PEEK=1` holds the sidebar's reveal open — it is otherwise opened
  by the pointer, which the harness does not have. `ZERVO_SHOT_PEEK=<pass>`
  lets go at that pass, so the closing half can be photographed too.
- `ZERVO_SHOT_CYCLE=<n>` presses ⌘S every `n` frames.
- `ZERVO_SHOT_NAV=<url>` navigates `ZERVO_SHOT_NAV_BEFORE` frames before the
  picture, for the page transitions.
- `ZERVO_SHOT_SETUP=<n>` opens the first run on that step. Every step after the
  first is reached by pressing a button, so without it only the welcome card
  can be photographed.

The two together are the fuzz that found the layer-id crash: a long run with
the reveal pinned open while the layout cycles underneath it puts the reveal and
the docked sidebar on screen at the same time, over and over, which by hand is a
matter of having the pointer in the right place at the right moment.

- [ ] Every preset × every starting layout, `ZERVO_SHOT_CYCLE=6
      ZERVO_SHOT_PEEK=1 ZERVO_SHOT_FRAME=260`, exits zero.

## Between two pages

Each seam has its own transition, and the point is that they are four different
motions rather than one. Settings → Appearance → Seam, then navigate.

- [ ] **Card** — the page recedes, drops a little and dims; the one arriving is
      already there behind it.
- [ ] **Frosted** — nothing translates. The old page defocuses and goes.
- [ ] **Edge to edge** — the old frame slides off to the left going forward and
      to the right going back, and does not leave a bright line along the edge
      it left by.
- [ ] **One surface** — the old page travels *up and under* the sidebar, and
      the sidebar does not move a pixel with it.
- [ ] Motion at zero in Appearance: navigation is a hard cut and no copy is
      taken. Motion at both extremes: the crossing is shorter and longer.
- [ ] Navigate twice quickly, resize the window mid-crossing, and change the
      seam mid-crossing. None of the three should leave a page fragment behind.

## Corners and the window's edge

- [ ] Full-page mode, every preset: the page reaches all four window edges with
      no gap, and its corner is one arc — the platform's, not a second one
      beside it. Move the corner scale to both ends; the window's corner must
      not move with it, and every other surface must.
- [ ] The framed layouts keep their gap: Zervo's eight points, Flat's six.
- [ ] Full-page mode, `zervo://settings`, `zervo://history` and
      `zervo://downloads`: each page's heading clears the close, minimise and
      zoom buttons. A web page is deliberately *not* inset.
- [ ] Appearance: the specimen's four corners are all round and all the same,
      and scrolling the page does not smear its bottom edge.

## When a page will not load

Each is reachable by typing its address, which is how to check them without an
engine failure to hand. Only `zervo://crashed` is constructed automatically:
there is no load-failure callback and no certificate hook in Servo 0.5.0, so
nothing can tell the embedder why an ordinary load ended. See PARITY.md.

- [ ] `zervo://unsupported` — the amber badge, the roadmap of bricks under the
      message, and a score that counts up as they are cleared. Press a brick
      directly; move the pointer into the panel and the paddle follows it and
      the ball plays. Move the pointer out and the ball parks rather than
      running in the background.
- [ ] `zervo://unsupported?host=watch.example.com&detail=Media%20Source%20Extensions`
      — the host is set in monospace, the missing feature in monospace amber,
      and the three buttons appear. "Open in Safari" opens Safari and not
      Zervo; "Try again" navigates *this* tab to the site.
- [ ] `zervo://offline` — no buttons at all, and the count of waiting tabs is
      the real one. Toolbar Reload is enabled and is the only way to try again.
- [ ] `zervo://certificate?host=mail.example.com&detail=*.cdn.net` — the danger
      colour, no glow on the card or the button, no game, and the way past is
      one line of plain text under the card. It is deliberately inert and says
      so on hover.
- [ ] `zervo://notfound?host=srvo.org` — with `servo.org` in the history, the
      page offers it by name with the visit count and a button that goes there.
      With no history it says so instead of offering nothing.
- [ ] `zervo://crashed?host=servo.org&detail=SIGSEGV` — the engine's own words
      are quoted in the danger colour, "Load it again" goes back to the host,
      and there is no game (a crash is not a wait). The real path: crash a
      content process and check the tab is rewritten in place, keeps its pin
      and its position, and that the dead page's last frame is gone rather than
      showing through.
- [ ] The address bar's own badge takes the page's icon and colour on all four
      — a padlock in red on the certificate page, not a globe.
- [ ] Pin one of them: the sidebar's tab row and the essentials grid show the
      same icon.
- [ ] Both themes, and a preset with no glow (Flat or Zervo): the pages are
      readable and nothing rings or glows where the arrangement says it should
      not.

## Navigation bar (sidebar collapsed)
- [ ] The address pill stays centred on the window as it is resized, and drags
      wider and narrower.
- [ ] Dragging the bar's bottom edge changes its height and uncovers the widget
      shelf; the webview card follows.
- [ ] Arrange mode (the wand): items drag within a side and across the pill, the
      caret shows where a drop lands, x moves an item to the tray, and the tray
      puts it back. Reset to defaults restores the bar *and* the sidebar width.
- [ ] Widget shelf: place a widget anywhere including an empty row, resize it
      from the corner, remove it. Positions survive resizing the window — cells
      narrow, the grid does not reflow.

## Cards that open on hover
- [ ] Favourites (the star) and downloads: the card opens on hover, stays open
      while the pointer is on it, and **clicking inside it does not reach the
      page underneath**.
- [ ] Favourites: rename commits on ✓ and on Enter, discards on ✗ and on Escape,
      and survives a restart. Remove works. Both list and tile views.
- [ ] Downloads: stop, start again, reveal, open. Right-click gives the menu.
      Hovering a row does not make its controls flicker.
- [ ] Corners: no doubled outline outside a rounded corner, no dark rim inside
      one, and the shadow falls off smoothly rather than in bands.

## History and logins
- [ ] History (sidebar or ⌘Y): search filters; rows group by day, week and month.
- [ ] Settings → Passwords: add, reveal, delete, export, import. Secrets live in
      the system keychain, not in `passwords.json` — check the file. Both it and
      the export are `rw-------`; check that too, since the export is every
      password in plaintext.
- [ ] HTTP basic auth **asks** before sending a saved login, naming the host
      that raised the challenge and the login on offer. It never sends one over
      plain `http://`. With logins saved for both `example.com` and
      `sub.example.com`, a challenge from `deep.sub.example.com` is offered the
      *more specific* one.

## Accessibility
- [ ] With VoiceOver on, the chrome is reachable: toggles, sliders and segmented
      controls announce what they are and what they are set to.
- [ ] A slider can be moved with the arrow keys once focused.

## Engine
- [ ] A content-heavy page renders and scrolls.
- [ ] Back/forward/reload behave.
- [ ] A link opening a new tab (`target=_blank`) is adopted as a tab.
- [ ] Favicons appear in tab rows.

## Notifications and permissions

A page is needed that raises them; `new Notification(...)` from the console is
not enough, because a notification only counts as user-initiated once the page
holds permission. Any local file with a few buttons on it does.

- [ ] `Notification.requestPermission()` raises a Zervo prompt naming the host
      rather than the whole URL. Denying is respected — the promise resolves to
      `denied` and nothing appears.
- [ ] Granting, then raising one: it grows **out of the bell** in the address
      bar rather than appearing beside it, and the text fades in partway
      through rather than being there from the first frame.
- [ ] The bell appears in the address bar only once something has been raised,
      carries a count from two upward, and reads `9+` past nine.
- [ ] After about six seconds the toast hides itself and the bell stays. Click
      the bell: the notification is still there. This is the whole reason the
      bell exists — one missed by six seconds is the one worth going back for.
- [ ] Three notifications sharing a `tag`: only ever one on screen, showing the
      most recent, and its six seconds restart each time it is replaced.
- [ ] `requireInteraction: true` does not time out, however long you leave it.
- [ ] Clicking a toast dismisses that one and leaves the rest. Dismissing the
      last one takes the bell away with it.
- [ ] With the tray open and more than one in it, a "Clear all" row sits under
      the stack and empties it.
- [ ] Raise twenty-five: the history caps at twenty and keeps the newest, and
      at most six are drawn at once.
- [ ] An empty title, a four-hundred-character title and mixed scripts with an
      emoji all stay inside the card rather than spilling out of it.
- [ ] **Idle after.** Once everything has lingered out, CPU falls back to
      near zero. A notification that keeps the event loop awake forever is the
      same bug the repaint scheduler already had once.
- [ ] Notifications are drawn in the same glass as menus: switch theme and
      translucency and they follow.

## Downloads (`--features engine-downloads`)
- [ ] Navigating to a `.zip` **asks** before saving, naming the host and the
      filename it will land under. Cancelling writes nothing at all — check the
      downloads folder, including for a `.part`.
- [ ] A saved file carries `com.apple.quarantine` (`xattr -l`), so macOS warns
      before opening it.
- [ ] The saved filename honours `Content-Disposition`.
- [ ] A same-origin `<a download="name.txt">` saves under that name.
- [ ] A **cross-origin** `<a download="x">` saves under the *URL's* name — the
      page must not be able to choose it.
- [ ] A `text/plain` link without `download` still renders.
- [ ] Downloading the same file twice produces `file (1).ext`.
- [ ] Cancel, reveal and open work from `zervo://downloads` and from the
      downloads card.

## Media (`--features media`)
- [ ] A local or progressive `.mp4` plays, with sound.
- [ ] The player widgets on the shelf show what is playing and drive it.

## Packaging
- [ ] macOS: the `.dmg` mounts, the app launches after
      `xattr -dr com.apple.quarantine`, and video still plays (the bundled
      GStreamer travelled with it).
- [ ] Linux: `.deb` and `.rpm` install and pull their dependencies. **Whether
      Zervo starts is untested — say either way.**
- [ ] Windows: the `.exe` runs from the unzipped folder with no console window
      behind it. **Also untested.**

## Performance
- [ ] Idle CPU near zero with a static page and no animated background. Check
      the **new tab page** with a non-animated backdrop as well as a web page —
      that one was nineteen per cent until the engine's frame-ready signal
      stopped asking for a repaint on behalf of a tab with no webview.
- [ ] An animated backdrop still animates: Aurora should sit around ten to
      fifteen per cent, not one or two. Both halves matter — the fix for one is
      how you break the other.
