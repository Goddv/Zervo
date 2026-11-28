# Living with the Servo engine

The macOS build refreshes [Goddv/servo main](https://github.com/Goddv/servo/tree/main)
and [Servo upstream main](https://github.com/servo/servo/tree/main) before
applying the patches below. It uses upstream directly when the fork is behind
and merges in fork commits when the branches have diverged. A merge or patch
conflict stops the build. The build stores its managed checkout under
`CARGO_TARGET_DIR` and leaves existing Servo checkouts alone.

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
- [Media Source Extensions](../patches/servo/0003-media-source-extensions.patch)
  connects Servo's media elements to the GStreamer MSE backend. Heion enables
  it with the optional `media-source` feature.
- The [larger HTTP/2 header limit](https://github.com/servo/servo/pull/49981)
  comes from upstream.

## Build from a recorded revision

GitHub resolves the two Servo revisions once per run. Both architectures build
the same patched tree and use the same resolved Cargo lockfile. Each release
includes `servo-revisions.txt` and `Heion-Cargo.lock`. To rebuild the release
from its source tarball, copy that lockfile to `Cargo.lock` and use the two
commit IDs from `servo-revisions.txt`.

```sh
HEION_SERVO_UPSTREAM='<upstream commit>' HEION_SERVO_FORK='<fork commit>' ZERVO_LOCKED=1 \
  ./scripts/bundle-macos.sh --dmg --features engine-downloads,media-source
```

For routine dependency checks, use the upstream baseline recorded in
[`upstream-revision`](../patches/servo/upstream-revision). Run
`scripts/prepare-servo.sh --pinned ../servo` if that sibling path is available,
or use `HEION_SERVO_MODE=pinned` with the macOS bundler to use its managed
checkout.

## Build the macOS streaming package

Install the [GStreamer 1.28.9 SDK](PACKAGING.md#audio-and-video). The bundler
downloads the matching source and builds the patched MSE library.
The [native patch](../patches/gstreamer/1.28.9-media-source.patch) fixes a
deadlock during separate audio and video initialization and corrects the
source-buffer update, abort, removal, and seek behavior needed for adaptive
playback. It gives compressed video a 128 MiB per-buffer limit and retains
the 16 MiB audio limit.
Keep `CARGO_TARGET_DIR` pointed to the same Heion target directory across
worktrees and rebuilds.

Use full Xcode for the native AI widget and Image Playground bridges. If the
SDK installs GStreamer plugins separately, set
`HEION_GSTREAMER_EXTRA_PLUGIN_DIR` to the directory containing its compatible
`libgstnice.dylib`.

```sh
export CARGO_TARGET_DIR="/absolute/path/to/existing/heion-target"
./scripts/bundle-macos.sh --dmg --features engine-downloads,media-source
```

This build targets window-based MSE with fragmented MP4 and WebM. Worker MSE,
`ManagedMediaSource`, and switching the MIME type of an existing source buffer
remain outside the implementation. On macOS, the MSE capability check
reports H.264 and VP9 when supported by the runtime. AV1 via MSE remains
disabled while the decoder path is unstable.
