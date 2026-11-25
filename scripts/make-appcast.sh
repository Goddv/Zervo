#!/usr/bin/env bash
# Generate a signed Sparkle feed from the current and prior shipped DMGs.
set -euo pipefail
# shellcheck source=scripts/lib/common.sh
. "$(dirname -- "${BASH_SOURCE[0]}")/lib/common.sh"

tools="${1:?missing Sparkle tools directory}"
staged="${2:?missing directory with current and prior DMGs}"
output="${3:?missing output directory}"
arch="${4:?missing archive architecture}"
version="${5:?missing release version}"
build="${6:?missing CFBundleVersion}"
prefix="${7:?missing release asset URL prefix}"
: "${SPARKLE_PRIVATE_ED_KEY:?missing Sparkle signing key}"
: "${ZERVO_SPARKLE_PUBLIC_KEY:?missing Sparkle public key}"
case "$arch" in arm64|x86_64) ;; *) die "unsupported architecture: $arch" ;; esac
[[ "$prefix" == https://*/ ]] || die "release asset prefix must be an HTTPS directory"
root="$(zervo_repo_root)"
archive="Zervo-$version-$arch-sparkle.dmg"
[ -s "$staged/$archive" ] || die "missing candidate $archive"
feed="zervo-appcast-$arch.xml"
mkdir -p "$output"
[ ! -e "$output/$feed" ] || die "appcast already exists in $output"
# Embed the tagged changelog as signed plain text, including wrapped bullets.
zervo_release_notes "$version" > "$staged/${archive%.dmg}.txt"

# Avoid leaving the private seed in the signing tool's environment.
signing_key="$SPARKLE_PRIVATE_ED_KEY"
unset SPARKLE_PRIVATE_ED_KEY
printf '%s\n' "$signing_key" | "$tools/generate_appcast" --ed-key-file - \
    --versions "$build" --maximum-versions 1 --maximum-deltas 5 \
    --delta-compression lzfse --embed-release-notes \
    --download-url-prefix "$prefix" -o "$output/$feed" "$staged"
# Bind each patch to the signed target executable and write architecture-specific URLs.
printf '%s\n' "$signing_key" | python3 "$root/scripts/lib/prepare-appcast.py" \
    "$output/$feed" "$staged" "$arch" "$version" "$prefix" "$tools/sign_update"
printf '%s\n' "$signing_key" | "$tools/sign_update" --ed-key-file - "$output/$feed"
unset signing_key

# Verify the generated files against the public key actually embedded in Zervo.
# Copy only referenced deltas. The appcast's placeholder DMG is never shipped.
xcrun swift "$root/scripts/lib/verify-appcast.swift" \
    "$output/$feed" "$staged" "$archive" "$build" "$prefix" "$ZERVO_SPARKLE_PUBLIC_KEY" "$output"
