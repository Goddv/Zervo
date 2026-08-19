# Packaging and distribution

```bash
./scripts/bundle-macos.sh                      # target/Zervo.app
./scripts/bundle-macos.sh --dmg                # + target/Zervo-<version>.dmg
./scripts/bundle-macos.sh --features engine-downloads --dmg
```

`scripts/make-icns.sh` turns the committed `assets/icon/zervo-1024.png` into a
proper `.icns` using stock `sips` and `iconutil`, so no design tooling is
needed to build a release.

## Unsigned builds — what your users will see

Zervo releases are **not signed with an Apple Developer ID and not notarized**.
A signing certificate costs $99/year, and notarization requires uploading each
build to Apple.

The bundler applies an *ad-hoc* signature (`codesign -s -`). That is enough for
macOS to load the binary at all on Apple Silicon, but it is **not** a Developer
ID signature and does not satisfy Gatekeeper.

A user who downloads a `.dmg` from GitHub gets a quarantine flag on it, and
macOS will refuse to open the app, usually with *"Zervo is damaged and can't be
opened"* — which is misleading; the app is fine, it is simply unsigned.

They can clear the flag:

```bash
xattr -dr com.apple.quarantine /Applications/Zervo.app
```

or right-click the app → **Open** → **Open** (this path has been removed in
recent macOS versions for unsigned apps, so the `xattr` command is the reliable
one). Put whichever instruction applies in your release notes — users should
not be left staring at "damaged".

Building from source has no such problem: a locally built app is never
quarantined.

## If you later want signed releases

1. Join the Apple Developer Program ($99/yr) and create a *Developer ID
   Application* certificate.
2. Sign with a hardened runtime:
   `codesign --force --options runtime --sign "Developer ID Application: NAME (TEAMID)" Zervo.app`
3. Notarize: `xcrun notarytool submit Zervo.dmg --apple-id … --team-id … --wait`
4. Staple: `xcrun stapler staple Zervo.dmg`

In CI, store the certificate as a base64 secret, import it into a temporary
keychain, and use an app-specific password (or App Store Connect API key) for
notarytool. Do not commit any of it.

## Audio and video

`--features media` needs the official GStreamer distribution installed at
`/Library/Frameworks/GStreamer.framework`. Servo supports no other source for it
on macOS, and pins 1.22.3. Both packages are needed — the runtime and the
headers:

```bash
BASE=https://github.com/servo/servo-build-deps/releases/download/macOS
curl -L -o /tmp/gstreamer.pkg       $BASE/gstreamer-1.0-1.22.3-universal.pkg
curl -L -o /tmp/gstreamer-devel.pkg $BASE/gstreamer-1.0-devel-1.22.3-universal.pkg
sudo installer -pkg /tmp/gstreamer.pkg -target /
sudo installer -pkg /tmp/gstreamer-devel.pkg -target /
```

`scripts/bundle-gstreamer.py` then copies the libraries and plugins into
`Zervo.app/Contents/Frameworks` and rewrites their install names, so the bundle
runs on machines that have never heard of GStreamer. `bundle-macos.sh` calls it
for you. The plugins are loaded by name at runtime and never show up in `otool`
output, so they are listed explicitly in that script; if playback breaks after a
Servo update, check the list against `gstreamer_plugin_lists/` in the servo
crate.

Expect the bundle to grow by roughly 100 MB.
