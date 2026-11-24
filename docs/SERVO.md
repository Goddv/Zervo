# Living with the Servo engine

Zervo 0.6.1.2 uses [Servo main at `3f21bf4ee33`](https://github.com/servo/servo/commit/3f21bf4ee339d4423873317aa6c9a5e14316c077).
The engine still identifies itself as Servo 0.8.0. The local build uses the
patched Servo checkout beside the Zervo checkout. The path is set in
`.cargo/config.toml`.

## Patches carried by Zervo

- [File downloads](../patches/servo/0001-embedder-file-downloads.patch) offers
  attachments, unrenderable responses, and `download` links to Zervo. After
  consent, Servo streams the original response so redirects, authentication,
  and cookies still apply. Build with `--features engine-downloads`.
- [Text-only web notifications](../patches/servo/0002-script-show-notification-without-resources.patch)
  delivers notifications that have no images, icons, or badges to fetch.
- The [larger HTTP/2 header limit](https://github.com/servo/servo/pull/49981)
  is part of the upstream revision.

## Reproduce the pinned local build

Place a fresh Servo checkout at `../servo` relative to the Zervo checkout,
then prepare it before running Cargo from Zervo.

```sh
git -C ../servo switch --detach 3f21bf4ee339d4423873317aa6c9a5e14316c077
git -C ../servo apply "$PWD/patches/servo/0001-embedder-file-downloads.patch"
git -C ../servo apply "$PWD/patches/servo/0002-script-show-notification-without-resources.patch"
cargo build --locked --profile dev-fast --features engine-downloads,media
```

CI prepares the same sibling checkout with `scripts/prepare-servo.sh`. Update
`patches/servo/upstream-revision` and refresh `Cargo.lock` together when advancing
the engine. Check both feature sets and run [the smoke checklist](TESTING.md),
including redirects, attachments, and a notification without an icon.
