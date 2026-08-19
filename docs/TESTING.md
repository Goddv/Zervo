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
- [ ] Theme: Auto/Light/Dark switch cleanly, including the titlebar; accent
      colours retint the chrome; pages see `prefers-color-scheme`.
- [ ] New tab page: each of the seven backgrounds; animated ones do not pin
      the CPU when the window is idle.

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
- [ ] Cancel, reveal and open work from `zervo://downloads`.

## Performance
- [ ] Idle CPU near zero with a static page and no animated background.
