<div align="center">

<img src="assets/icon/zervo-1024.png" width="128" alt="Zervo">

# Zervo

**A calm, workspace-oriented browser built on the [Servo][servo] engine.**

[![Release](https://img.shields.io/github/v/tag/Goddv/Zervo?label=release&sort=semver)](https://github.com/Goddv/Zervo/releases)
[![License](https://img.shields.io/badge/license-MPL--2.0-blue)](LICENSE)
[![Engine](https://img.shields.io/badge/engine-Servo-informational)](https://servo.org)
![Rust](https://img.shields.io/badge/rust-1.97-orange)

**macOS today — Linux and Windows build from the same tag and are untested.**

<table>
<tr>
<td width="50%" valign="top">

<a href="assets/screenshots/zervo-light.png">
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/screenshots/zervo-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="assets/screenshots/zervo-light.png">
  <img alt="The Zervo window with the sidebar open: workspaces and tabs in a column on the left, and a new tab page showing the time, one search field, the session you left and the sites you use" src="assets/screenshots/zervo-light.png" width="100%">
</picture>
</a>

</td>
<td width="50%" valign="top">

<a href="assets/screenshots/zervo-light-bar.png">
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/screenshots/zervo-dark-bar.png">
  <source media="(prefers-color-scheme: light)" srcset="assets/screenshots/zervo-light-bar.png">
  <img alt="The same window with the sidebar collapsed: a slim bar across the top with the address pill centred, over a web page rendered by Servo" src="assets/screenshots/zervo-light-bar.png" width="100%">
</picture>
</a>

</td>
</tr>
<tr>
<td align="center"><sub><b>Sidebar</b> — navigation, workspaces and tabs in one column<br>
<a href="assets/screenshots/zervo-light.png">light</a> · <a href="assets/screenshots/zervo-dark.png">dark</a>, full size</sub></td>
<td align="center"><sub><b>Collapsed</b> — navigation moves into a bar across the top<br>
<a href="assets/screenshots/zervo-light-bar.png">light</a> · <a href="assets/screenshots/zervo-dark-bar.png">dark</a>, full size</sub></td>
</tr>
</table>

<br>

<a href="assets/screenshots/zervo-appearance.png">
<img alt="The Appearance settings page: a live specimen of the material pinned under the title, the preset row, and controls for the theme, the accent and how far it reaches into the chrome" src="assets/screenshots/zervo-appearance.png" width="82%">
</a>

<sub><b>Appearance</b> — every value the chrome is built from is a control, and the<br>
specimen at the top is the real material rather than a picture of one:<br>
it is wrong exactly when the chrome is wrong.</sub>

</div>

---

Zervo is a browser chrome — sidebar, workspaces, tabs, settings — wrapped around
Servo, Mozilla's independent web engine written in Rust. There is no Chromium
and no Gecko underneath: the page you are looking at is rendered by Servo.

It is an experiment, and an honest one: Servo is not yet a complete web engine,
so some sites will not work. See [Limitations](#limitations).

## What it looks like

### The window

- **Sidebar-first, or bar-first.** Everything lives in one collapsible sidebar:
  navigation, the address bar, pinned "essentials", workspaces and tabs. Collapse
  it and navigation moves into a bar across the top, with the address pill
  centred on the window and the sidebar left holding only tabs.
- **A bar you arrange.** Drag its buttons between the two sides, take them off,
  put them back. Drag the bar itself taller and it uncovers a shelf of widgets —
  a clock, now playing and transport controls — laid out on a grid, dropped
  where you like and resized by the corner.
- **Workspaces.** Tabs are grouped into named spaces, each with its own colour.
- **Favourites and history.** The star saves a page and hovering it opens the
  list; history is searchable and grouped by how long ago.
- **Saved logins**, kept in the system keychain rather than in Zervo's own files.

### The new tab page

- **It greets you rather than briefing you.** The time, one search field, the
  session you actually left, and the three sites you use. A new tab is opened
  to go somewhere.
- **Or a board of thirteen cards on a twelve-column grid** — clock, world
  clocks, search, pinned tabs, most-visited, recent, favourites, downloads, now
  playing, a note, a to-do list, workspaces, the mark. One press away, and
  every card can be dragged, resized and taken off.
- **Eight backdrops**, chosen from the page's own header: Plain, Gradient, Grid,
  Mesh, Aurora, Waves, Particles, or a photograph fetched from Wikimedia Commons
  or Openverse and credited as its licence asks.
- **Text that reads either way.** Cards, the greeting, the clock and the photo
  credit each ask about their own patch of the picture rather than taking one
  answer for the page — a photograph is dark sky at the top and bright water at
  the bottom, and pale text set for the sky disappears into the water.

### How it is put together

- **Every surface is drawn through one material.** Corner radii, fills, sheen,
  shadow reach and how far glass frosts are decided in one place and reach the
  whole application without a call site knowing a number. Surfaces have a class
  — card, menu, input — the way an element has one in CSS.
- **And every number in it is a setting.** Settings → Appearance has one
  control per named field: the fill, the sheen, the blur, the edge, the corner
  scale, the glow, the settle time, and how far the accent reaches into the
  chrome. Save an arrangement under a name, or copy it out as the Rust `const`
  you would otherwise have written by hand.
- **Zervo's own material is the finished one.** Four more presets — Candy,
  Flat, Liquid Glass, Material — came out of the same design study and are
  offered beside it, but they are experiments rather than settled work and are
  still being tuned.
- **The seam between the chrome and the page is one setting with four steps**,
  and each step brings the page transition its own geometry implies: a card
  recedes, a tint dissolves in place, a frame slides, a page under floating
  glass travels up while the glass stays put.
- **Frosted, over anything.** The chrome sits on real macOS vibrancy, and
  everything floating over a page frosts against a blurred copy of whatever that
  page is showing — so a menu over a website is the same glass as a card over a
  photograph. Two steps, Solid and Frosted, drive the window and everything on
  it together.
- **Themed.** Light/dark/auto following the system, ten accent presets plus a
  colour of your own, and the accent is propagated into the engine so pages see
  the matching `prefers-color-scheme`.

The material system is the seam a theme for another platform is written
against — Windows, GTK, Android Material, Liquid Glass. Five arrangements ship,
and every value one is built from is a control on `zervo://settings` →
Appearance: the fill, the sheen, the blur, the edge, the corner scale, the
glow, the settle time, how far the accent reaches, and where the chrome ends
and the page begins. It is documented in [docs/THEMING.md](docs/THEMING.md),
including what is still not there — palettes, and somewhere to save an
arrangement under a name of your own.

## Building

Zervo builds the Servo engine from source, so the first build is long
(roughly an hour) and needs a lot of disk. Subsequent builds are incremental.

```bash
git clone https://github.com/goddv/zervo
cd zervo
cargo build --profile dev-fast
./target/dev-fast/zervo
```

The toolchain is pinned in `rust-toolchain.toml` and matches Servo's own.
`rustup` will install it automatically; if it complains about unavailable
components, install it explicitly:

```bash
rustup toolchain install 1.97.1 --profile minimal --component clippy,rustfmt
```

### Downloads

File downloads need a small patch to the engine, because Servo has no download
support of its own yet ([servo#40210][downloads-issue]). See
[docs/SERVO.md](docs/SERVO.md) — it is one `git apply` and one Cargo line, and
then:

```bash
cargo build --profile dev-fast --features engine-downloads
```

Without the feature Zervo runs perfectly well; responses the engine cannot
render simply are not offered for saving.

### Packaging

```bash
./scripts/bundle-macos.sh --dmg                  # macOS, this machine's architecture
./scripts/package-linux.sh --all                 # .deb, .rpm, AppImage, tarball, PKGBUILD
pwsh scripts/package-windows.ps1 -Installer      # Windows, .zip and a setup .exe
```

A release tag produces all of it: two `.dmg`s, one per Mac architecture; a
`.deb`; an `.rpm` for Fedora 44; an AppImage; a portable tarball; `PKGBUILD`s
for the AUR; a Windows zip and installer; and a `SHA256SUMS` covering the lot.

Nothing is signed. On macOS that means macOS will refuse to open the app until
the quarantine flag is cleared, and on Windows it means SmartScreen shows a
warning on first run. [docs/PACKAGING.md](docs/PACKAGING.md) covers exactly what
a user sees on each platform, what signing would cost, and the traps — a
link-time one on macOS if you have GStreamer installed, and the reason there is
no Windows ARM64 build.

## Layout

| Path | What lives there |
|------|------------------|
| `src/main.rs` | winit shell, the frame pipeline, input routing, shortcuts |
| `src/app.rs` | engine glue — owns Servo, implements `WebViewDelegate` |
| `src/state.rs` | workspaces, tabs, chrome state |
| `src/ui.rs` | the whole chrome, drawn with egui; emits `UiAction`s |
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
including how the chrome and the engine share one GL context, and
[docs/THEMING.md](docs/THEMING.md) describes the material system a theme for
another platform would be written against.

## Platforms

Every release tag builds all of these. Only the first has been run.

| Platform | Package | State |
| --- | --- | --- |
| **macOS** 11+, Apple Silicon | `.dmg` (arm64), GStreamer bundled | ✅ Used daily by its author |
| **macOS** 11+, Intel | `.dmg` (x86_64), GStreamer bundled | ⚠️ Builds — never started |
| **Ubuntu 26.04+** x86_64 | `.deb` | ⚠️ Builds and installs — never started |
| **Fedora 44** x86_64 | `.rpm` | ⚠️ Builds and installs — never started |
| **Any glibc 2.35+** x86_64 | `.AppImage` | ⚠️ Never run |
| **Arch** | `PKGBUILD` for the AUR, not yet published | ⚠️ Validated, never built |
| **Windows** x64 | `.zip` and an installer | ⚠️ Builds — startup crash fixed in 0.4.1, unconfirmed |
| **Windows** ARM64 | — | ⛔ Blocked upstream; the x64 build runs under Prism |

macOS ships as two downloads, one per architecture, each built natively. They
could be one universal bundle — `lipo` will fuse them and the bundler takes
`--universal` — but each of these is roughly half the size, because a
single-architecture bundle has its GStreamer libraries thinned to match, and a
download that says which Mac it is for is worth more than one that runs on
both. Both need macOS 11 or later rather than 13.

Linux and Windows are x86_64 only. There is no technical obstacle to arm64 on
Linux; there is simply nobody asking for it yet.

The `.deb` is built on Ubuntu 26.04 and so needs 26.04 or newer — glibc runs
old binaries on new systems and never the reverse. Anything older is what the
AppImage is for: it is built against glibc 2.35 so that it reaches back
further than any distribution package can.

Windows on ARM has no build because Servo does not have one: a `float16_t`
collision between MSVC's `<arm_fp16.h>` and the vendored `glsl-optimizer`, whose
fix is still unmerged both upstream and in Servo. The x64 build runs on those
machines through Prism, which emulates x64 user-mode code and does not emulate
the GPU path. See [docs/PACKAGING.md](docs/PACKAGING.md).

**Still a macOS release first.** Every package comes from the same tag, and
the material system itself is portable — it is drawn by egui and nothing else.
But the window's frosted backdrop is an AppKit feature, and the corner
compositing built on top of it has been tried nowhere else.

0.4.1 fixes the one thing Windows was known to do: die on startup, before any
window, reading `GL_VERSION` from a context surfman's WGL backend had never
made current. Windows now composites through ANGLE — EGL over Direct3D 11 —
which is what Servo's own Windows builds do. That is a fix made from the crash
log; nobody has watched it start.

Nobody has started Zervo on Linux at all either. The packages compile, package
and install; whether a window opens is untested, and X11 versus Wayland and the
GL setup have never been exercised. **A report that it does not start is as
useful as one that says it does** —
[open an issue](https://github.com/Goddv/Zervo/issues) either way.

Two things macOS keeps that the others do not: the frosted vibrancy behind the
chrome, which is AppKit's, and bundled media — GStreamer is an ordinary package
on Linux and a piece of work not yet done on Windows.

## Limitations

- **Servo is young.** Expect broken layouts, missing APIs, and sites that
  refuse the user agent. This is a property of the engine, not of Zervo — and
  Zervo says so where it happens rather than in this list: a page the engine
  cannot render gets `zervo://unsupported`, which names what is missing and
  hands the page to another browser, and a content process that falls over
  gets `zervo://crashed` rather than a frozen picture of itself.
- **Streaming video does not work.** YouTube and the rest need Media Source
  Extensions, which Servo does not have. Local and progressive video plays.
- **No extensions.** The engine has none, so neither does the button.
- **Passwords cannot be filled into web forms.** The engine offers no hook for
  a submitted form, so saved logins are a vault plus HTTP authentication.
- **No sync, no profiles.** Not yet.
- **Palettes are not themeable.** How a surface is *built* is yours to set, and
  so is how far the accent reaches into it — and an arrangement can be saved
  under a name of your own. The colours the two themes start from are still
  constants. See [docs/THEMING.md](docs/THEMING.md).
- **0.5.0 is a redesign, and it wants testing.** The chrome, the new tab page,
  the error pages and the first run all changed. 0.6.0 is the polish pass;
  until then, [open an issue](https://github.com/Goddv/Zervo/issues) for
  anything that reads wrong — and say which preset, theme and layout you were
  in, because most of what has been fixed so far only happened in one of them.
- **Unsigned builds.** See [docs/PACKAGING.md](docs/PACKAGING.md).

## Contributing

Yes please — see [CONTRIBUTING.md](CONTRIBUTING.md).
[docs/TODO.md](docs/TODO.md) is what is being worked on next;
[docs/PARITY.md](docs/PARITY.md) is the full list of what still stands between
Zervo and a browser you could use daily, in the order the gaps hurt, with where
in the code each one lives. Most of it is embedder work, not engine work.

If you want to improve the *engine*, contribute to [Servo][servo] directly.
Note that Servo does not accept AI-generated contributions.

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
[downloads-issue]: https://github.com/servo/servo/issues/40210
