#!/usr/bin/env bash
# Build a single-file Zervo Flatpak for the native Linux architecture.
#
#   ./scripts/build-flatpak.sh [--profile <name>] [--features <list>]
#                              [--output <dir>] [--binary <existing ELF>]
#
# Requires Flatpak, flatpak-builder and the Flathub remote. CI reuses the
# AppImage build's unmodified ELF. Build or supply a binary whose required
# sonames and symbol versions are available in the GNOME runtime.
set -euo pipefail

# shellcheck source=scripts/lib/common.sh
. "$(dirname -- "${BASH_SOURCE[0]}")/lib/common.sh"
cd "$(zervo_repo_root)"

PROFILE=release
FEATURES=""
BINARY=""
OUTPUT="$(zervo_target_dir)/linux"

while [ $# -gt 0 ]; do
    case "$1" in
        --profile) PROFILE="${2:?--profile needs a value}"; shift 2 ;;
        --features) FEATURES="${2:?--features needs a value}"; shift 2 ;;
        --binary) BINARY="${2:?--binary needs a value}"; shift 2 ;;
        --output) OUTPUT="${2:?--output needs a value}"; shift 2 ;;
        -h|--help) zervo_usage; exit 0 ;;
        *) die "unknown option: $1" ;;
    esac
done

[ "$(uname -s)" = Linux ] || die "Flatpaks are built on Linux"
need flatpak flatpak-builder
ARCH="$(flatpak --default-arch)"
VERSION="$(zervo_version)"
if [ -z "$BINARY" ]; then
    zervo_cargo_build "$PROFILE" "$FEATURES" ""
    BINARY="$(zervo_binary_path "$PROFILE" "")"
fi
[ -x "$BINARY" ] || die "no executable at $BINARY"

mkdir -p "$OUTPUT"
WORK="$(mktemp -d)"
zervo_cleanup_on_exit "$WORK"
zervo_stage_tree "$WORK/stage" "$BINARY" /app
mv -- "$WORK/stage/app" "$WORK/app"
APP="$WORK/app"
sed -i "s/^Icon=zervo$/Icon=$ZERVO_APPID/" "$APP/share/applications/$ZERVO_APPID.desktop"
for icon in "$APP"/share/icons/hicolor/*/apps/zervo.*; do
    mv -- "$icon" "${icon%/*}/$ZERVO_APPID.${icon##*.}"
done
cp "scripts/flatpak/$ZERVO_APPID.yml" "$WORK/$ZERVO_APPID.yml"

say "Building Flatpak for $ARCH"
flatpak-builder --user --install-deps-from=flathub --disable-updates \
    --state-dir="$WORK/state" --repo="$WORK/repo" \
    "$WORK/build" "$WORK/$ZERVO_APPID.yml"

OUT="$OUTPUT/Zervo-$VERSION-$ARCH.flatpak"
flatpak build-bundle --arch="$ARCH" \
    --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    "$WORK/repo" "$OUT" "$ZERVO_APPID"
zervo_sha256 "$OUT" >&2
note "$OUT"
