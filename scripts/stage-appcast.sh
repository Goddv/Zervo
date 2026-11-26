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
executable="$app/Contents/MacOS/Zervo"
team_requirement=""
signature_kind() {
    local info
    info="$(codesign -dv --verbose=2 "$app" 2>&1)"
    if grep -Fxq 'Signature=adhoc' <<<"$info"; then
        printf '%s\n' adhoc
    elif grep -q '^Authority=Developer ID Application:' <<<"$info"; then
        printf '%s\n' developer-id
    else
        die "expected an ad-hoc or Developer ID Application signature"
    fi
}

verify_mounted_app() {
    codesign --verify --deep --strict "$app"
    if [ -n "$team_requirement" ]; then
        codesign --verify --strict --test-requirement "$team_requirement" "$app"
    fi
    if [ "$mode" = developer-id ]; then
        xcrun stapler validate "$app" >&2
    fi
    [ "$(lipo -archs "$executable")" = "$arch" ] || die "unexpected app architecture"
    [ "$(plutil -extract CFBundleIdentifier raw "$plist")" = app.zervo.browser ] ||
        die "unexpected app bundle identifier"
    [ "$(plutil -extract SUPublicEDKey raw "$plist")" = "$ZERVO_SPARKLE_PUBLIC_KEY" ] ||
        die "app does not contain the configured Sparkle key"
    [ "$(plutil -extract SUFeedURL raw "$plist")" = "https://github.com/Goddv/$1/releases/latest/download/zervo-appcast-$arch.xml" ] ||
        die "app contains an unexpected feed URL"
}

hdiutil attach -readonly -nobrowse -mountpoint "$mount" "$candidate" >&2
mode="$(signature_kind)"
verify_mounted_app Heion
if [ "$mode" = developer-id ]; then
    team_requirement="$(codesign -dr - "$app" 2>&1 | sed -n 's/^designated => //p')"
    [ -n "$team_requirement" ] || die "candidate is missing a designated requirement"
fi
shasum -a 256 "$executable" | cut -d ' ' -f 1 > "$stage/target-executable.sha256"
plutil -extract CFBundleVersion raw "$plist"
hdiutil detach "$mount" >&2
# Keep the feed's installer name identical to the asset that gets uploaded.
cp "$candidate" "$stage/Heion-${tag#heion-v}-$arch.dmg"

# Retain exact, signed Zervo bundles for users who skip the first Heion release.
for version in 0.6.1.2 0.6.1.3; do
    baseline="$stage/Zervo-$version-$arch-sparkle.dmg"
    if [ "$tag" = heion-v0.1.0 ]; then
        gh release download "v$version" --pattern "Zervo-$version-$arch.dmg" --output "$baseline" >&2
    else
        gh release download heion-v0.1.0 --pattern "Legacy-Zervo-$version-$arch.dmg" --output "$baseline" >&2
    fi
    hdiutil attach -readonly -nobrowse -mountpoint "$mount" "$baseline" >&2
    [ "$(signature_kind)" = "$mode" ] || die "Zervo $version uses a different signature type"
    verify_mounted_app Zervo
    case "$version" in
        0.6.1.2) expected=0.6.102 ;;
        0.6.1.3) expected=0.6.103 ;;
    esac
    [ "$(plutil -extract CFBundleVersion raw "$plist")" = "$expected" ] || die "unexpected Zervo $version build number"
    hdiutil detach "$mount" >&2
done

previous_tags="$(gh release list --limit 20 --exclude-drafts --exclude-pre-releases \
    --json tagName --jq '.[].tagName')"
count=0
while IFS= read -r prior; do
    [[ "$prior" =~ ^heion-v[0-9]+\.[0-9]+\.[0-9]+(\.[0-9]+)?$ && "$prior" != "$tag" ]] || continue
    names="$(gh release view "$prior" --json assets --jq '.assets[].name')"
    grep -Fxq "zervo-appcast-$arch.xml" <<<"$names" || continue
    version="${prior#heion-v}"
    baseline="$stage/Zervo-$version-$arch-sparkle.dmg"
    gh release download "$prior" --pattern "Heion-$version-$arch.dmg" --output "$baseline" >&2
    hdiutil attach -readonly -nobrowse -mountpoint "$mount" "$baseline" >&2
    if [ "$(signature_kind)" != "$mode" ]; then
        hdiutil detach "$mount" >&2
        rm "$baseline"
        continue
    fi
    verify_mounted_app Heion
    hdiutil detach "$mount" >&2
    count=$((count + 1))
    [ "$count" -lt 3 ] || break
done <<<"$previous_tags"
