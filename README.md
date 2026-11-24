<div align="center">

<img src="assets/icon/zervo-1024.png" width="128" alt="Zervo">

# Zervo

**A calm, workspace-oriented browser built on the [Servo][servo] engine.**

[![Release](https://img.shields.io/github/v/tag/Goddv/Zervo?label=release&sort=semver)](https://github.com/Goddv/Zervo/releases)
[![License](https://img.shields.io/badge/license-MPL--2.0-blue)](LICENSE)
[![Engine](https://img.shields.io/badge/engine-Servo-informational)](https://servo.org)
![Rust](https://img.shields.io/badge/rust-1.100-orange)

[macOS downloads](https://github.com/Goddv/Zervo/releases) · [Latest preview builds](https://github.com/Goddv/Zervo/actions/workflows/release.yml)
· [Changelog](CHANGELOG.md)

</div>

---

Zervo is a macOS browser powered by Servo, an independent web engine written
in Rust. Organise tabs with Spaces and folders in a docked sidebar or browse
in Full Page mode with the sidebar available on hover.

## What it looks like

### The window

- **Spaces.** Each Space has its own tabs and folders. Swipe across the sidebar
  to switch Spaces. Right-click a tab to pin it or move it. Favourites are
  shared across Spaces.
- **Command bar.** Press ⌘T for a new tab or ⌘L to edit the address. Search tabs,
  favourites, and history, or start with `>` to find browser actions.
- **Full Page.** Press ⌘S to dock or hide the sidebar. Move the pointer to the
  left edge to reveal it temporarily.
- **Native macOS chrome.** The sidebar, page surround, popovers, and dialogs
  use AppKit Liquid Glass on macOS 26 and later. Zervo follows the macOS glass
  appearance and Reduce Transparency settings.
- **Customizable chrome.** Open Settings → Layout or click the sliders in the
  sidebar. Show or hide sections and reorder them by dragging or using the
  arrow buttons. Toggle sidebar tools and the widget shelf, or restore the
  sidebar defaults.
- **Favourites and history.** The star saves a page and hovering it opens the
  list; history is searchable and grouped by how long ago.
- **Saved logins**, kept in the system keychain rather than in Zervo's own files.

### The new tab page

- **New Tab.** The search page and optional icon card use the same Zervo artwork
  as first-run setup and About.
- **A customizable board of thirteen cards on a twelve-column grid** — clock, world
  clocks, search, pinned tabs, most-visited, recent, favourites, downloads, now
  playing, a note, a to-do list, spaces, and the Zervo icon. One press away, and
  every card can be dragged, resized and taken off.
- **Eight backdrops**, chosen from the page's own header: Plain, Gradient, Grid,
  Mesh, Aurora, Waves, Particles, or a photograph fetched from Wikimedia Commons
  or Openverse and credited as its licence asks.
- **Text that reads either way.** Cards, the greeting, the clock and the photo
  credit each ask about their own patch of the picture rather than taking one
  answer for the page — a photograph is dark sky at the top and bright water at
  the bottom, and pale text set for the sky disappears into the water.

### How it is put together

- **Portable surfaces share a material system.** Corner radii, fills, sheen,
  shadow reach and how far glass frosts are decided in one place and reach the
  whole application without a call site knowing a number. Surfaces have a class
  — card, menu, input — the way an element has one in CSS.
- **Appearance.** Choose a theme and accent, or let the active Space provide the
  accent. New profiles start with classic Apple blue at 0.10 tint strength.
- **Page frame.** In Sidebar mode, inset the page in a rounded card or place it
  flush against the window edges. Outline and shadow are configurable.

See [docs/THEMING.md](docs/THEMING.md) for the material system.

## Building

Zervo builds the Servo engine from source, so the first build is long
(roughly an hour) and needs a lot of disk. Subsequent builds are incremental.

```bash
git clone https://github.com/goddv/zervo
cd zervo
# Prepare the sibling Servo checkout as described in docs/SERVO.md.
cargo build --profile dev-fast
./target/dev-fast/zervo
```

The toolchain is pinned in `rust-toolchain.toml` for local builds and CI.
`rustup` will install it automatically; if it complains about unavailable
components, install it explicitly:

```bash
rustup toolchain install 1.100.0 --profile minimal --component clippy,rustfmt
```

### Downloads

Zervo's patched Servo engine includes support for file downloads. Prepare
the engine as described in [docs/SERVO.md](docs/SERVO.md), then enable the
download prompt when building:

```bash
cargo build --profile dev-fast --features engine-downloads
```

See [docs/SERVO.md](docs/SERVO.md) for the engine pin and patch status.

### Packaging

```bash
./scripts/bundle-macos.sh --dmg --features engine-downloads,media
```

GitHub Actions builds Apple Silicon and Intel disk images from `main`.
Tagged releases also include the source archive and checksums.
See [Packaging and distribution](docs/PACKAGING.md).

## Layout

| Path | What lives there |
|------|------------------|
| `src/main.rs` | winit shell, the frame pipeline, input routing, shortcuts |
| `src/app.rs` | engine glue — owns Servo, implements `WebViewDelegate` |
| `src/state.rs` | workspaces, tabs, chrome state |
| `src/ui/` | the whole chrome, drawn with egui; emits `UiAction`s |
| `zervo-core/src/theme.rs` | palettes, accents, and the material every surface is built from |
| `zervo-core/src/glass.rs` | the one place a surface is drawn |
| `src/backdrop.rs` | the blurred copy surfaces frost against, and the corner cut |
| `src/newtab.rs` | the new tab page: cards, grid, backdrops |
| `src/wallpaper.rs`, `zervo-core/src/net.rs` | wallpapers from Commons and Openverse |
| `src/widgets.rs` | toggles, sliders, segmented controls |
| `src/icons.rs`, `src/phosphor.rs` | [Phosphor][phosphor] icon rendering |
| `src/downloads.rs` | download manager |
| `src/vibrancy.rs` | macOS `NSVisualEffectView` backdrop |
| `patches/servo/` | engine patches Zervo can build against |

There is a longer tour in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md),
including how the chrome and the engine share a graphics context.

## Platform support

Active builds target macOS on Apple Silicon and Intel. Native Liquid Glass
requires macOS 26 or later. Linux and Windows builds are paused.

## Limitations

- **Servo is young.** Expect broken layouts, missing APIs, and sites that
  refuse the user agent. This is a property of the engine, not of Zervo — and
  Zervo says so where it happens rather than in this list: a page the engine
  cannot render gets `zervo://unsupported`, which names what is missing and
  hands the page to another browser, and a content process that falls over
  gets `zervo://crashed` rather than a frozen picture of itself.
- **Streaming video support varies.** Some services may fail to play.
- **No extensions.** The engine has none, so neither does the button.
- **Saved logins.** Use the browser's fill action to enter a saved login.
- **No sync, no profiles.** Not yet.
- **Palettes are not themeable.** How a surface is *built* is yours to set, and
  so is how far the accent reaches into it — and an arrangement can be saved
  under a name of your own. The colours the two themes start from are still
  constants. See [docs/THEMING.md](docs/THEMING.md).
- **Feedback.** [Open an issue](https://github.com/Goddv/Zervo/issues) with
  the macOS version, theme, and sidebar layout you were using.
- **Unsigned builds.** See [docs/PACKAGING.md](docs/PACKAGING.md).

## Contributing

Yes please — see [CONTRIBUTING.md](CONTRIBUTING.md).
[docs/TODO.md](docs/TODO.md) is what is being worked on next;
[docs/PARITY.md](docs/PARITY.md) is the full list of what still stands between
Zervo and a browser you could use daily, in the order the gaps hurt, with where
in the code each one lives. Most of it is embedder work, not engine work.

If you want to improve the *engine*, contribute to [Servo][servo] directly.
Note that Servo does not accept AI-generated contributions.

### Help translate Zervo

The interface uses Fluent catalogues. Copy the English catalogue, translate as
much as you can, and open a pull request — incomplete catalogues fall back to
English, so a few corrected messages are useful too. The
[translation guide](docs/TRANSLATING.md) covers language tags, placeholders,
local previewing, checks, and the pull-request checklist.

## Credits

- [Servo][servo] — the engine, MPL-2.0.
- [Phosphor Icons][phosphor] — the icon set, MIT. Vendored in `assets/fonts/`.
- [Zen Browser][zen] — the interaction design Zervo's chrome is modelled on.
  Zervo shares no code with Zen; it is a separate implementation on a
  different engine, and is not affiliated with or endorsed by it.

## License

[MPL-2.0](LICENSE), the same licence as Servo.

[servo]: https://servo.org
[zen]: https://zen-browser.app
[phosphor]: https://phosphoricons.com
