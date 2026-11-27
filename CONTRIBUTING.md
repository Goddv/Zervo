# Contributing

Thanks for looking. Heion is small and there is a lot of low-hanging fruit.

## Getting set up

At the start of a development cycle, check the latest Servo main revision and
refresh the pin as described in [docs/SERVO.md](docs/SERVO.md).

```bash
cargo build --profile dev-fast && ./target/dev-fast/heion
```

The first build compiles the Servo engine and takes about an hour. After that,
chrome-only changes rebuild in seconds. Read
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first — the GL-context sharing is
the one genuinely surprising thing in the codebase.

## Translate the interface

[The translation guide](docs/TRANSLATING.md) explains how to edit a catalogue,
preview it in a built copy of Heion, and open a pull request. Partial
translations work because missing messages fall back to English. Translation
pull requests do not need to include Rust changes.

## Ground rules

- **`cargo fmt` and `cargo clippy` clean.** CI enforces both — `cargo fmt
  --all --check` and `cargo clippy -p heion-core --all-targets -- -D warnings`
  run on every pull request, along with `cargo test -p heion-core` and
  `cargo deny`. The whole of that takes under a minute, because `zervo-core`
  holds the engine-independent half of the browser and does not need Servo.
  The half that *does* need it is linted and tested in the release build.
  Locally, `cargo clippy` over everything is a few seconds once the engine has
  been built once.
- **The packaging half is linted too.** `shellcheck -x`, `actionlint`,
  PSScriptAnalyzer, `desktop-file-validate`, `appstreamcli`, `mandoc -Tlint`
  and a render of both PKGBUILDs all run on every pull request, because almost
  none of that can be *run* on a developer's machine — and the alternative to
  a linter is finding the typo three hours into a release build.
- **No new dependencies without a reason in the PR description.** The engine is
  already enormous; the chrome should stay small.
- **`ui.rs` never touches the engine.** If you find yourself reaching for it
  there, add a `UiAction` instead. (It does write to `Settings` and
  `BrowserState` in place — see the note in
  [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md); the rule that holds is the one
  about the engine.)
- **Take colours from the `Palette`.** No literal colours outside `theme.rs`.
- **Put pure logic in `zervo-core`.** Anything that is arithmetic, colour,
  bytes or files rather than engine or window belongs there, and gets a unit
  test on every pull request instead of a line on a manual checklist.
- **Walk [docs/TESTING.md](docs/TESTING.md)** before opening a PR, and say in
  the PR which parts you exercised.

## Good first issues

[docs/TODO.md](docs/TODO.md) is the working queue and the better place to pick
from. The easiest starts on it:

- **Translate one part of the interface.** A partial Fluent catalogue is a
  valid contribution, and untranslated messages remain in English.
- A new widget for the shelf: `dashboard.rs` takes any grid-placed widget.

## Engine bugs

If a *page* renders wrong, that is almost always Servo, not Heion. Reproduce it
in `servoshell` from a Servo checkout, and if it still happens, report it to
[servo/servo](https://github.com/servo/servo/issues). Note that Servo does not
accept AI-generated contributions.

## Licence

By contributing you agree your work is licensed under MPL-2.0.
