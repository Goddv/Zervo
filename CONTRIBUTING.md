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

- Session restore (workspaces and tabs across launches).
- Tab drag-to-reorder, and dragging between workspaces.
- `alert()` / `confirm()` / `prompt()` dialogs — the engine already offers them
  through `show_embedder_control`; Zervo currently auto-answers them.
- IME support (`EmbedderControl::InputMethod`).
- Linux and Windows: everything except `vibrancy.rs`, the Dock icon and the
  bundling scripts is portable.
- Find-in-page, zoom UI, history.

## Engine bugs

If a *page* renders wrong, that is almost always Servo, not Zervo. Reproduce it
in `servoshell` from a Servo checkout, and if it still happens, report it to
[servo/servo](https://github.com/servo/servo/issues). Note that Servo does not
accept AI-generated contributions.

## Licence

By contributing you agree your work is licensed under MPL-2.0.
