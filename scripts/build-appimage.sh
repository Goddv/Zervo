#!/usr/bin/env bash
# Build a Zervo AppImage.
#
#   ./scripts/build-appimage.sh [--profile <name>] [--features <list>]
#                               [--target <triple>] [--output <dir>]
#                               [--no-build] [--validate-appstream]
#
# An AppImage is a single executable file that runs on any distribution new
# enough for the glibc it was built against, so build it on the OLDEST
# distribution you intend to support — glibc is backwards compatible and not
# forwards. CI does this in an ubuntu:22.04 container.
#
# What is deliberately NOT bundled:
#
#   The graphics and display stack — libGL, libEGL, libGLdispatch, libglapi,
#   libgbm, libdrm, libX11, libxcb, libwayland-client — because those are
#   coupled to the host's video driver and its compositor. AppImage's own
#   excludelist says so, and the failure mode is not subtle: a bundled
#   libwayland-client against a newer host Mesa makes eglGetDisplay return
#   EGL_BAD_PARAMETER and the browser never draws a frame. linuxdeploy applies
#   that excludelist; the check at the end of this script confirms it did.
#
#   GStreamer. Servo refuses to bootstrap GStreamer on Linux at all, the
#   linuxdeploy plugin for it describes itself as experimental, and bundling it
#   drags in glib, gio, gobject and libwayland-client — precisely the set above.
#   Audio and video therefore come from the host's own GStreamer, and
#   GST_PLUGIN_SYSTEM_PATH_1_0 is left alone: pointing it at a directory that
#   does not exist disables the host's plugin search and corrupts the user's
#   GStreamer registry for every other application on the machine.
set -euo pipefail

# shellcheck source=scripts/lib/common.sh
. "$(dirname -- "${BASH_SOURCE[0]}")/lib/common.sh"
cd "$(zervo_repo_root)"

PROFILE="release"
FEATURES=""
TRIPLE=""
BUILD=1
VALIDATE_APPSTREAM=0
OUTPUT="$(zervo_target_dir)/linux"

while [ $# -gt 0 ]; do
    case "$1" in
        --profile)  PROFILE="${2:?--profile needs a value}"; shift 2 ;;
        --features) FEATURES="${2:?--features needs a value}"; shift 2 ;;
        --target)   TRIPLE="${2:?--target needs a value}"; shift 2 ;;
        --output)   OUTPUT="${2:?--output needs a value}"; shift 2 ;;
        --no-build) BUILD=0; shift ;;
        --validate-appstream) VALIDATE_APPSTREAM=1; shift ;;
        -h|--help) zervo_usage; exit 0 ;;
        *) die "unknown option: $1" ;;
    esac
done

need file
[ "$(uname -s)" = "Linux" ] || die "AppImages are built on Linux"

VERSION="$(zervo_version)"
ARCH="$(zervo_appimage_arch "${TRIPLE%%-*}")"
SOURCE_DATE_EPOCH="$(zervo_source_date_epoch)"
export SOURCE_DATE_EPOCH

# appimagetool and linuxdeploy are themselves AppImages, and an AppImage needs
# FUSE to mount itself. Containers almost never have it. This makes them
# unpack into a temp directory and exec from there instead, which is the
# documented answer and costs a second.
export APPIMAGE_EXTRACT_AND_RUN=1
# linuxdeploy copies a copyright file for every library it bundles, which it
# looks up with dpkg-query. Harmless where dpkg exists and noisy where it does
# not, and none of it ends up mattering to a user.
export DISABLE_COPYRIGHT_FILES_DEPLOYMENT=1

# Neither linuxdeploy nor its appimage plugin publishes anything but a rolling
# `continuous` tag, so that is what gets downloaded. Override to pin.
LINUXDEPLOY_TAG="${ZERVO_LINUXDEPLOY_TAG:-continuous}"
RUNTIME_TAG="${ZERVO_APPIMAGE_RUNTIME_TAG:-continuous}"
TOOLS="${ZERVO_APPIMAGE_TOOLS:-$(zervo_target_dir)/appimage-tools}"

fetch() {
    local url="$1" dest="$2"
    if [ -s "$dest" ]; then
        note "cached $(basename -- "$dest")"
        return 0
    fi
    note "fetching $url"
    # --retry, because a network flake three hours into a release build is the
    # most expensive kind.
    curl --fail --location --silent --show-error \
         --retry 5 --retry-delay 2 --retry-all-errors \
         -o "$dest.part" "$url"
    mv -- "$dest.part" "$dest"
}

say "tools"
mkdir -p "$TOOLS"
LINUXDEPLOY="$TOOLS/linuxdeploy-$ARCH.AppImage"
PLUGIN="$TOOLS/linuxdeploy-plugin-appimage-$ARCH.AppImage"
RUNTIME="$TOOLS/runtime-$ARCH"
fetch "https://github.com/linuxdeploy/linuxdeploy/releases/download/$LINUXDEPLOY_TAG/linuxdeploy-$ARCH.AppImage" "$LINUXDEPLOY"
fetch "https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/$LINUXDEPLOY_TAG/linuxdeploy-plugin-appimage-$ARCH.AppImage" "$PLUGIN"
# Pinning the runtime file keeps appimagetool from reaching out to github in
# the middle of the build, and makes the output reproducible.
fetch "https://github.com/AppImage/type2-runtime/releases/download/$RUNTIME_TAG/runtime-$ARCH" "$RUNTIME"
chmod +x "$LINUXDEPLOY" "$PLUGIN"

if [ "$BUILD" = 1 ]; then
    zervo_cargo_build "$PROFILE" "$FEATURES" "$TRIPLE"
fi
BUILT="$(zervo_binary_path "$PROFILE" "$TRIPLE")"
[ -x "$BUILT" ] || die "no binary at $BUILT"

# ── AppDir ────────────────────────────────────────────────────────────────
WORK="$(mktemp -d)"
zervo_cleanup_on_exit "$WORK"
APPDIR="$WORK/AppDir"
zervo_stage_tree "$APPDIR" "$BUILT"

# appimagetool finds AppStream metadata by taking the desktop file's name and
# replacing .desktop with .appdata.xml — a `.metainfo.xml` is invisible to it.
# Both names are installed so the file is found by the AppImage tooling and by
# anything following the current freedesktop convention.
cp "$APPDIR/usr/share/metainfo/app.zervo.Zervo.metainfo.xml" \
   "$APPDIR/usr/share/metainfo/app.zervo.Zervo.appdata.xml"

say "AppDir"
"$LINUXDEPLOY" --appdir "$APPDIR" \
    --executable "$APPDIR/usr/bin/zervo" \
    --desktop-file "$APPDIR/usr/share/applications/app.zervo.Zervo.desktop" \
    --icon-file "$APPDIR/usr/share/icons/hicolor/256x256/apps/zervo.png" \
    --icon-filename zervo

# ── The bundled-library check ─────────────────────────────────────────────
# linuxdeploy applies AppImage's excludelist, and this confirms it. The
# libraries below are coupled to the host's video driver or display server;
# shipping a copy of any of them produces a bundle that runs on the build
# machine and fails on someone else's, which is the one failure this format is
# supposed to prevent.
say "checking what was bundled"
FORBIDDEN="libGL.so libEGL.so libGLdispatch.so libGLX.so libOpenGL.so libglapi.so
libgbm.so libdrm.so libX11.so libX11-xcb.so libxcb.so libwayland-client.so
libc.so.6 libstdc++.so libgcc_s.so"
BAD=0
for lib in $FORBIDDEN; do
    if find "$APPDIR" -name "$lib*" -print -quit | grep -q .; then
        warn "bundled a library that must come from the host: $lib"
        BAD=1
    fi
done
[ "$BAD" = 0 ] || die "refusing to ship an AppImage carrying host-coupled libraries"

# ── The AppImage ──────────────────────────────────────────────────────────
say "AppImage"
mkdir -p "$OUTPUT"
OUT="$OUTPUT/Zervo-$VERSION-$ARCH.AppImage"

# zsync update information, so `AppImageUpdate` and appimaged can offer an
# in-place delta update from the latest GitHub release. The glob has to match
# the .zsync file this run produces, and the .zsync has to be uploaded to the
# release beside the AppImage.
export LDAI_UPDATE_INFORMATION="gh-releases-zsync|Goddv|Zervo|latest|Zervo-*-$ARCH.AppImage.zsync"
export LDAI_RUNTIME_FILE="$RUNTIME"
export LDAI_OUTPUT="$OUT"
export LDAI_VERBOSE=1
export VERSION
export ARCH
# AppStream validation is done on every pull request by check.yml, where a
# failure costs seconds. Doing it again here would let an appstreamcli version
# difference fail a release build that has already spent three hours compiling
# the engine.
[ "$VALIDATE_APPSTREAM" = 1 ] || export LDAI_NO_APPSTREAM=1

"$PLUGIN" --appdir "$APPDIR"

[ -f "$OUT" ] || die "the appimage plugin produced no file at $OUT"
chmod +x "$OUT"
zervo_sha256 "$OUT" >&2
note "$OUT"
ls -la "$OUTPUT"
