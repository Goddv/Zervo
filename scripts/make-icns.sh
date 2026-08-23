#!/usr/bin/env bash
# Build assets/icon/Zervo.icns from the committed 1024x1024 PNG.
#
# The artwork is authored in Apple's Icon Composer (assets/icon/Zervo.icon);
# zervo-1024.png is that document's default variant rendered flat, committed so
# a release can be built with stock tools only — sips and iconutil, both of
# which ship with macOS — and no Xcode. Re-export it from Icon Composer after
# editing the layers.
set -euo pipefail

# shellcheck source=scripts/lib/common.sh
. "$(dirname -- "${BASH_SOURCE[0]}")/lib/common.sh"
cd "$(zervo_repo_root)"

need sips iconutil

SOURCE="assets/icon/zervo-1024.png"
OUTPUT="assets/icon/Zervo.icns"
WORK="$(mktemp -d)"
zervo_cleanup_on_exit "$WORK"
ICONSET="$WORK/Zervo.iconset"

[ -f "$SOURCE" ] || die "missing $SOURCE"

# `sips -z` scales to exactly the height and width it is given, so a source
# that is not square comes out distorted — in a valid .icns, with exit code 0,
# and nobody notices until the icon is on someone's dock.
WIDTH="$(sips -g pixelWidth "$SOURCE" | awk '/pixelWidth/ { print $2 }')"
if [ "$WIDTH" != "1024" ] || [ "$HEIGHT" != "1024" ]; then
    die "$SOURCE is ${WIDTH}x${HEIGHT}, expected 1024x1024"
fi

mkdir -p "$ICONSET"

# iconutil expects exactly these names; @2x entries are the next size up.
for spec in "16:icon_16x16" "32:icon_16x16@2x" "32:icon_32x32" "64:icon_32x32@2x" \
            "128:icon_128x128" "256:icon_128x128@2x" "256:icon_256x256" \
            "512:icon_256x256@2x" "512:icon_512x512" "1024:icon_512x512@2x"; do
    size="${spec%%:*}"
    name="${spec#*:}"
    sips -z "$size" "$size" "$SOURCE" --out "$ICONSET/$name.png" > /dev/null
done

iconutil -c icns "$ICONSET" -o "$OUTPUT"
say "wrote $OUTPUT"
