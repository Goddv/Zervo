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

Everything here was measured on macOS 27, not assumed.

Gatekeeper's verdict turns entirely on one thing: whether the app carries
`com.apple.quarantine`. Apple's own `gktool scan Zervo.app` says *"Scan
completed and software is allowed by system policy"* for an unquarantined build
of Zervo, and *"failed because the software is not signed by a distributor that
meets the system Gatekeeper requirements"* once the flag is on. The bundle is
fine either way; the flag is the whole story.

Do not use `spctl -a -t exec` to check this. It reports `rejected` for an
ad-hoc signed app whether or not it is quarantined, so it tells you nothing
about what a user will actually see.

The flag does not live on the app inside the disk image. The download stamps it
on the `.dmg`, the mounted volume then carries a `quarantine` mount flag, and
the attribute is written onto the copy when the app is dragged out. So the two
places worth clearing it are the `.dmg` before mounting, or the installed app
afterwards:

```bash
xattr -dr com.apple.quarantine /Applications/Zervo.app
```

Without that, macOS says *"Zervo is damaged and can't be opened"*, which reads
like a corrupt download and is not what has happened.

For anyone who would rather not use a terminal, open the app, let it be refused,
then go to **System Settings → Privacy & Security**, where an **Open Anyway**
button appears for it. Do not tell users to Control-click → Open: Apple removed
that bypass for improperly signed apps in macOS Sequoia ([Updates to runtime
protection in macOS Sequoia][sequoia]).

[sequoia]: https://developer.apple.com/news/?id=saqachfa

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

## Linux

```bash
./scripts/package-linux.sh --deb          # on Ubuntu/Debian
./scripts/package-linux.sh --rpm          # on Fedora
```

Build each package on the distribution it targets. A binary linked against
Ubuntu's libraries will not reliably run on Fedora, and `rpmbuild` derives the
package's requirements from the binary, which means those requirements have to
be Fedora's sonames. The CI workflow does this by building the `.deb` on an
Ubuntu runner and the `.rpm` inside a `fedora` container.

Neither package carries a hand-written dependency list: `dpkg-shlibdeps` and
`rpmbuild` each work them out from the binary. The build dependencies are
Servo's own lists, copied from `python/servo/platform/linux_packages/`.

GStreamer is an ordinary system package on Linux, so `--features media` needs no
bundling — unlike macOS, where the framework has to be copied into the app. The
package depends on the system GStreamer instead.

Nothing here is signed, which on Linux is not the obstacle it is on macOS: both
`dpkg -i` and `dnf install` will install an unsigned local package, with at most
a warning.

### What is missing on Linux

- **File uploads** shell out to `zenity`. Install it if `<input type=file>` does
  nothing. The proper fix is the XDG desktop portal over D-Bus.
- **No vibrancy.** The translucent chrome is an AppKit feature; the Linux build
  draws the same chrome without the frosted backdrop behind it.
- **Wayland vs X11** has had no testing at all.
