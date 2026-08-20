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
- [ ] New tab page: each of the seven backgrounds; animated ones do not pin
      the CPU when the window is idle.

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
- [ ] Settings → Passwords: add, reveal, delete, export, import. A saved login
      is offered for HTTP basic auth. Secrets live in the system keychain, not
      in `passwords.json` — check the file.

## Engine
- [ ] A content-heavy page renders and scrolls.
- [ ] Back/forward/reload behave.
- [ ] A link opening a new tab (`target=_blank`) is adopted as a tab.
- [ ] Favicons appear in tab rows.

## Downloads (`--features engine-downloads`)
- [ ] Navigating to a `.zip` saves it instead of showing "Unknown content type".
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
- [ ] Idle CPU near zero with a static page and no animated background.
