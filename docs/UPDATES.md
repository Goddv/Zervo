# macOS updates

Open **Settings → About → Check for Updates** to check and install. **Release
Notes** opens `zervo://updates`, which shows the installed version, update
status, the size and notes of an available patch, and the bundled changelog.
Automatic updates are enabled in signed release builds and can be switched off
in About. A background update applies after you quit Zervo.

## What an update changes

Zervo publishes a separate signed feed for Apple Silicon and Intel. Sparkle
builds a binary delta from each supported prior app bundle. An update downloads
that delta, reconstructs the new bundle in staging, and verifies the result
against the installed app's Apple code-signing requirement. An atomic
filesystem swap replaces the app at its existing path. The browser profile
stays outside the bundle.

An absent, invalid, or interrupted patch leaves the installed app in place.
Zervo requires signatures on both the feed and patch, and disables Sparkle's
full-download and two-step installation fallbacks. If no delta matches, use
the GitHub installer linked from the Updates page. A regression discovered
after a successful installation can be recovered with the previous release's
DMG.

## Enable the GitHub release pipeline

1. Set the repository's Apple Developer ID Application certificate and
   notarization secrets listed in [packaging](PACKAGING.md). Use the same
   Team ID and bundle identifier for subsequent releases.
2. Use Sparkle's `generate_keys` to create a dedicated Zervo Ed25519 key.
   Save the public key as the repository variable `ZERVO_SPARKLE_PUBLIC_KEY`.
   Export the seed into the repository secret `SPARKLE_PRIVATE_ED_KEY` and keep
   an offline backup. Keep the seed out of source control and app bundles.
3. Add a versioned entry in [CHANGELOG.md](../CHANGELOG.md), update the Cargo
   version, and tag a new release. The first notarized updater-enabled release
   is installed from its DMG. Later releases can patch from up to three prior
   updater-enabled releases. Keep each published DMG intact so the next release
   can build deltas against its exact bytes.
4. Inspect the draft before publishing. Each architecture must have a verified
   appcast. Entries with eligible baselines also carry signed delta assets.
   Only the architecture-specific deltas and the signed feeds are used by
   in-app updates. The regular DMGs remain available for manual installation.

Apple certificates are available only to the macOS build jobs. The Ed25519
private key is exposed only to the step that signs update assets. Preview
builds and releases without an enrolled public key show the bundled release
history and offer the manual installer link.

## Smoke test a release

Install a copy of the previous signed Zervo in a disposable location. Check that
the Updates page reports the target release and patch size. Check manually and
also try the background install-on-quit path. After quitting, confirm the app
at the original path has the new bundle version and starts with the same
profile.

In a test feed with separate keys, modify the XML, corrupt a delta, and make the
delta URL unavailable in turn. Each check should preserve the old signed app.
An interrupted staging operation should also leave the installed path usable.
