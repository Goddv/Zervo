# Contributing

Thanks for looking. Zervo is small and there is a lot of low-hanging fruit.

## Getting set up

```bash
cargo build --profile dev-fast && ./target/dev-fast/zervo
```

The first build compiles the Servo engine and takes about an hour. After that,
chrome-only changes rebuild in seconds. Read
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first — the GL-context sharing is
the one genuinely surprising thing in the codebase.

## Ground rules

- **`cargo fmt` and `cargo clippy` clean.** CI enforces both.
- **No new dependencies without a reason in the PR description.** The engine is
  already enormous; the chrome should stay small.
- **`ui.rs` stays pure.** It draws from state and returns `UiAction`s. If you
  find yourself reaching for the engine there, add an action instead.
- **Take colours from the `Palette`.** No literal colours outside `theme.rs`.
- **Walk [docs/TESTING.md](docs/TESTING.md)** before opening a PR, and say in
  the PR which parts you exercised.

## Good first issues

[docs/TODO.md](docs/TODO.md) is the working queue and the better place to pick
from. The easiest starts on it:

- **Try the Linux or Windows build and say what happened.** They have never been
  run. No Rust required.
- Session restore (workspaces and tabs across launches).
- Clearing cookies and site data from Settings — the engine calls exist.
- A favicon cache on disk, so history and favourites show icons.
- The link target on hover, in the usual bottom-left overlay.
- Tab drag-to-reorder, and dragging between workspaces.
- A new widget for the shelf: `dashboard.rs` takes any grid-placed widget.

## Engine bugs

If a *page* renders wrong, that is almost always Servo, not Zervo. Reproduce it
in `servoshell` from a Servo checkout, and if it still happens, report it to
[servo/servo](https://github.com/servo/servo/issues). Note that Servo does not
accept AI-generated contributions.

## Licence

By contributing you agree your work is licensed under MPL-2.0.
