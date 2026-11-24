#!/usr/bin/env bash
# Stage exact shipped DMGs for patch generation and record the signed candidate's build number.
set -euo pipefail
# shellcheck source=scripts/lib/common.sh
. "$(dirname -- "${BASH_SOURCE[0]}")/lib/common.sh"

tag="${1:?missing release tag}"
arch="${2:?missing archive architecture}"
candidate="${3:?missing candidate DMG}"
stage="${4:?missing staging directory}"
: "${ZERVO_SPARKLE_PUBLIC_KEY:?missing Sparkle public key}"
case "$arch" in arm64|x86_64) ;; *) die "unsupported architecture: $arch" ;; esac
mkdir -p "$stage"
mount="$(mktemp -d)"
trap 'hdiutil detach "$mount" >/dev/null 2>&1 || true; rmdir "$mount"' EXIT
app="$mount/Zervo.app"
plist="$app/Contents/Info.plist"
verify_mounted_app() {
    codesign --verify --deep --strict "$@" "$app"
    xcrun stapler validate "$app" >&2
    [ "$(lipo -archs "$app/Contents/MacOS/Zervo")" = "$arch" ] ||
        die "unexpected app architecture"
    [ "$(plutil -extract SUPublicEDKey raw "$plist")" = "$ZERVO_SPARKLE_PUBLIC_KEY" ] ||
        die "app does not contain the configured Sparkle key"
    [ "$(plutil -extract SUFeedURL raw "$plist")" = "https://github.com/Goddv/Zervo/releases/latest/download/zervo-appcast-$arch.xml" ] ||
        die "app contains an unexpected feed URL"
}

hdiutil attach -readonly -nobrowse -mountpoint "$mount" "$candidate" >&2
verify_mounted_app
requirement="$(codesign -dr - "$app" 2>&1 | sed -n 's/^designated => //p')"
[ -n "$requirement" ] || die "candidate is missing a designated requirement"
plutil -extract CFBundleVersion raw "$plist"
hdiutil detach "$mount" >&2
cp "$candidate" "$stage/Zervo-${tag#v}-$arch-sparkle.dmg"

# Only published releases with a feed had updater-enabled installers.
previous_tags="$(gh release list --limit 20 --exclude-drafts --exclude-pre-releases \
    --json tagName --jq '.[].tagName')"
count=0
while IFS= read -r prior; do
    [[ "$prior" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(\.[0-9]+)?$ && "$prior" != "$tag" ]] || continue
    names="$(gh release view "$prior" --json assets --jq '.assets[].name')"
    grep -Fxq "zervo-appcast-$arch.xml" <<<"$names" || continue
    archive="Zervo-${prior#v}-$arch.dmg"
    baseline="$stage/Zervo-${prior#v}-$arch-sparkle.dmg"
    gh release download "$prior" --pattern "$archive" --output "$baseline" >&2
    hdiutil attach -readonly -nobrowse -mountpoint "$mount" "$baseline" >&2
    verify_mounted_app --test-requirement "$requirement"
    hdiutil detach "$mount" >&2
    count=$((count + 1))
    [ "$count" -lt 3 ] || break
done <<<"$previous_tags"
