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
