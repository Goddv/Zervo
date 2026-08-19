#!/usr/bin/env bash
# Build assets/icon/Zervo.icns from the committed 1024x1024 PNG.
#
# The artwork is authored in Apple's Icon Composer (assets/icon/Zervo.icon);
# zervo-1024.png is that document's default variant rendered flat, committed
# so a release can be built with stock tools only (sips, iconutil) and no
# Xcode. Regenerate it with `cargo run --bin render-icon` after editing the
# layers in Icon Composer.
set -euo pipefail

cd "$(dirname "$0")/.."
SOURCE="assets/icon/zervo-1024.png"
ICONSET="$(mktemp -d)/Zervo.iconset"
OUTPUT="assets/icon/Zervo.icns"

[ -f "$SOURCE" ] || { echo "missing $SOURCE" >&2; exit 1; }
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
rm -rf "$(dirname "$ICONSET")"
echo "wrote $OUTPUT"
