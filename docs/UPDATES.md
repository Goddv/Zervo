# macOS updates

Open **Settings → About → Check for Updates** to check and install. **Release
Notes** opens `zervo://updates`, which shows the installed version, update
status, the size and notes of an available patch, and the bundled changelog.
Updater-enabled builds check daily by default. In About you can choose hourly,
six-hourly, daily, or weekly checks and independently enable installation on
quit. Update notices appear inside Heion. To receive macOS alerts, enable
**Show macOS update notifications** and allow them when prompted. **Remind Me
Tomorrow** schedules a reminder for the available release.

## What an update changes

Heion publishes a separate signed feed for Apple Silicon and Intel. Sparkle
builds a binary delta from each supported prior app bundle. An update downloads
that delta and reconstructs the new bundle in staging. The signed delta
includes the expected SHA-256 of the staged executable. For ad-hoc builds,
Heion also verifies the sealed bundle, nested code, app identifier, target
version, and embedded update key. Developer ID builds verify the staged app
against the installed app's Apple code-signing requirement. An atomic
filesystem swap replaces the app at its existing path. The browser profile
stays outside the bundle.

The Heion branding update uses the existing executable name, bundle identifier,
profile directory, keychain service, and signed feeds. The updater installs
the new bundle at the existing app path.

An absent, invalid, or interrupted patch leaves the installed app in place.
Heion requires signatures on both the feed and patch, and disables Sparkle's
full-download and two-step installation fallbacks. If no delta matches, use
the GitHub installer linked from the Updates page. A regression discovered
after a successful installation can be recovered with the previous release's
DMG.

## Enable the GitHub release pipeline

1. Use Sparkle's `generate_keys` to create a dedicated Heion Ed25519 key.
   Save the public key as the repository variable `ZERVO_SPARKLE_PUBLIC_KEY`.
   Export the seed into the repository secret `SPARKLE_PRIVATE_ED_KEY` and keep
   an offline backup. If the key is lost, distributing a new trusted key
   requires a manually installed release.
2. Leave the Apple secrets empty to release ad-hoc builds. To ship notarized
   builds, configure the Developer ID Application certificate and notarization
   secrets listed in [packaging](PACKAGING.md). Switching signing modes
   requires installing a new baseline from its DMG.
3. Add a versioned entry in [CHANGELOG.md](../CHANGELOG.md), update the Cargo
   version, and tag a new release. Install the first updater-enabled release
   from its DMG. macOS evaluates that first download under its usual
   [Gatekeeper policy](https://support.apple.com/en-us/102445). Later releases
   can patch from up to three prior updater-enabled releases with the same
   signing mode. Keep each published DMG intact.
4. Inspect the draft before publishing. Each architecture must have a verified
   appcast. Entries with eligible baselines also carry signed delta assets.
   The regular DMGs remain available for manual installation.

The Ed25519 private key is exposed only to the step that signs update assets.
Preview builds and releases without an enrolled public key show the bundled
release history and offer the manual installer link.

## Follow a release from source to update

- In Keychain Access, the dedicated Sparkle account is `zervo-release`. The
  key tool's `-p` flag prints its public key. The export flag `-x` writes
  sensitive material, so keep exports off shared folders and out of Git.
- GitHub embeds its public variable in each app. The signing job alone gets
  the private secret, verifies the release bundles, and signs the appcasts and
  any deltas. A first release has two appcasts and zero deltas because it is
  the baseline.
- Push a version tag to build a GitHub draft. Review the two DMGs, the two
  architecture-specific appcasts, and `SHA256SUMS` before publishing. A
  manual dispatch with **publish** enabled also publishes after verification.
- Publishing makes the feeds reachable at the app's fixed GitHub URLs.
  When the following release is cut, the workflow uses the exact previous
  DMGs to calculate deltas and publishes them alongside the new feed.

## Smoke test a release

Install a copy of the previous updater-enabled Heion in a disposable location. Check that
the Updates page reports the target release and patch size. Check manually and
also try the background install-on-quit path. After quitting, confirm the app
at the original path has the new bundle version and starts with the same
profile.

In a test feed with separate keys, modify the XML, corrupt a delta, and make the
delta URL unavailable in turn. Each check should preserve the old signed app.
An interrupted staging operation should also leave the installed path usable.
