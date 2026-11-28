# Living with the Servo engine

Heion uses the Servo main revision recorded in [upstream-revision](../patches/servo/upstream-revision).
The engine still identifies itself as Servo 0.8.0. The local build uses the
patched Servo checkout beside the Heion checkout. The path is set in
`.cargo/config.toml`.

## Patches carried by Heion

- [File downloads](../patches/servo/0001-embedder-file-downloads.patch) offers
  attachments, unrenderable responses, and eligible `download` links to
  Heion. After consent, Servo streams the original response so redirects,
  authentication, and cookies still apply. Heion saves in the background and
  publishes the completed file without overwriting an existing file. Cancelling
  stops the fetch and discards the partial file. Retry sends a fresh GET as a
  download, including when the browser would normally show the response inline.
  The same patch gives desktop servoshell a native Save dialog and a cancellable
  transfer. Build Heion with `--features
  engine-downloads`. The downstream patch carries the prototype discussed in
  [Servo issue #40210](https://github.com/servo/servo/issues/40210).
- [Text-only web notifications](../patches/servo/0002-script-show-notification-without-resources.patch)
  delivers notifications that have no images, icons, or badges to fetch.
- The [larger HTTP/2 header limit](https://github.com/servo/servo/pull/49981)
  is part of the upstream revision.

## Reproduce the pinned local build

Place a fresh Servo checkout at `../servo` relative to the Heion checkout,
then prepare it before running Cargo from Heion.

```sh
git -C ../servo switch --detach "$(cat patches/servo/upstream-revision)"
git -C ../servo apply "$PWD/patches/servo/0001-embedder-file-downloads.patch"
git -C ../servo apply "$PWD/patches/servo/0002-script-show-notification-without-resources.patch"
cargo build --locked --profile dev-fast --features engine-downloads,media
```

CI prepares the same sibling checkout with `scripts/prepare-servo.sh`. At the
start of each development cycle and before cutting a release, check the tip of
[`servo/servo` main](https://github.com/servo/servo/commits/main/). Pin the
latest revision in `patches/servo/upstream-revision`, review the local patches
against it, and refresh `Cargo.lock` in the same change. Check both feature
sets and run [the smoke checklist](TESTING.md), including redirects,
attachments, and a notification without an icon. The explicit pin keeps CI
and local builds reproducible.
