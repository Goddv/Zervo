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
