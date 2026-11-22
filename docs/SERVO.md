# Living with the Servo engine

Zervo builds against [Servo v0.8.0](https://github.com/servo/servo/releases/tag/v0.8.0)
with two downstream patches and an upstream HTTP/2 fix. `Cargo.toml` selects the release version and
`.cargo/config.toml` pins the exact revision on
[`Goddv/servo`](https://github.com/Goddv/servo/tree/codex/zervo-servo-0.8.0-http2).
Cargo resolves the Servo workspace from that fork, including the sibling
crates changed by the patches. A fresh clone uses the pin automatically.

## Patch status in Servo v0.8.0

- **File downloads.** Servo still lacks an embedder download API. Zervo's
  [download patch](../patches/servo/0001-embedder-file-downloads.patch) offers
  attachments, unrenderable responses, and links with a `download` attribute
  to the embedder. The browser asks before saving, and Servo streams the
  original response so cookies, authentication, and redirects still apply.
  Build with `--features engine-downloads` to enable Zervo's save prompt.
  [Upstream issue #40210](https://github.com/servo/servo/issues/40210).
- **Text-only web notifications.** Servo v0.8.0 calls the show step when
  its last image, icon or badge request completes. Notifications with no
  resources never reach that callback.
  [The notification patch](../patches/servo/0002-script-show-notification-without-resources.patch)
  calls the show step directly for those notifications. All Zervo builds pick
  up this fix.
- **HTTP/2 response headers.** The release's 16 KiB limit rejects responses
  from YouTube and other sites with larger headers. The
  [upstream fix](https://github.com/servo/servo/pull/49981) raises the limit
  to 64 KiB. The fork backports it to v0.8.0.

## Updating Servo

1. Compare the patch behavior against the upstream release tag. Remove a patch
   once the release itself covers its behavior.
2. Apply the remaining patches in order in a clean checkout of the upstream
   tag. The download patch uses `git apply`; the notification and HTTP/2
   patches use `git am`. Push the rebased commits to a versioned branch of the
   Zervo fork.
3. Update both Servo declarations in `Cargo.toml` and the fork revision in
   `.cargo/config.toml`. Compare the Rust toolchain and the macOS deployment
   target with upstream's settings. Refresh `Cargo.lock` with `cargo update -p servo`.
4. Check a default build and a build with downloads enabled. Run the
   [smoke checklist](TESTING.md), including redirects, attachments and a
   notification without an icon.

For local engine development, replace the fork entry in `.cargo/config.toml`
with `servo = { path = "/absolute/path/to/servo/components/servo" }`
temporarily. Keep downstream patches on the Zervo fork and follow Servo's
contribution policy when reporting upstream issues.
