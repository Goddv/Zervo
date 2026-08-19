#!/usr/bin/env bash
# Package Zervo as a macOS .app (and optionally a .dmg).
#
#   ./scripts/bundle-macos.sh [--profile <name>] [--dmg] [--features <list>]
#
# The result is UNSIGNED. See docs/PACKAGING.md for what that means for users.
set -euo pipefail

cd "$(dirname "$0")/.."

PROFILE="release"
FEATURES=""
MAKE_DMG=0
while [ $# -gt 0 ]; do
    case "$1" in
        --profile) PROFILE="$2"; shift 2 ;;
        --features) FEATURES="$2"; shift 2 ;;
        --dmg) MAKE_DMG=1; shift ;;
        *) echo "unknown option: $1" >&2; exit 1 ;;
    esac
done

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
APP="target/Zervo.app"
BIN_DIR="$APP/Contents/MacOS"
RES_DIR="$APP/Contents/Resources"

# The GStreamer framework ships its own pkg-config that knows where the
# framework is, so putting its bin first is all the build needs to find it.
GSTREAMER_ROOT="/Library/Frameworks/GStreamer.framework/Versions/1.0"
case ",$FEATURES," in
    *,media,*)
        [ -d "$GSTREAMER_ROOT" ] || {
            echo "--features media needs GStreamer at $GSTREAMER_ROOT" >&2
            echo "see docs/PACKAGING.md" >&2
            exit 1
        }
        export PATH="$GSTREAMER_ROOT/bin:$PATH"
        WITH_MEDIA=1 ;;
    *) WITH_MEDIA=0 ;;
esac

echo "==> building (profile: $PROFILE${FEATURES:+, features: $FEATURES})"
if [ -n "$FEATURES" ]; then
    cargo build --profile "$PROFILE" --features "$FEATURES"
else
    cargo build --profile "$PROFILE"
fi

# `--profile release` puts artefacts in target/release, not target/relese.
BUILT="target/$PROFILE/zervo"
[ "$PROFILE" = "dev" ] && BUILT="target/debug/zervo"
[ -x "$BUILT" ] || { echo "no binary at $BUILT" >&2; exit 1; }

echo "==> icon"
./scripts/make-icns.sh

echo "==> assembling $APP"
rm -rf "$APP"
mkdir -p "$BIN_DIR" "$RES_DIR"
cp "$BUILT" "$BIN_DIR/Zervo"
cp assets/icon/Zervo.icns "$RES_DIR/Zervo.icns"

if [ "$WITH_MEDIA" = "1" ]; then
    echo "==> bundling GStreamer"
    ./scripts/bundle-gstreamer.py "$APP"
fi

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>                  <string>Zervo</string>
    <key>CFBundleDisplayName</key>           <string>Zervo</string>
    <key>CFBundleIdentifier</key>            <string>app.zervo.browser</string>
    <key>CFBundleExecutable</key>            <string>Zervo</string>
    <key>CFBundleIconFile</key>              <string>Zervo</string>
    <key>CFBundlePackageType</key>           <string>APPL</string>
    <key>CFBundleShortVersionString</key>    <string>$VERSION</string>
    <key>CFBundleVersion</key>               <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>        <string>13.0</string>
    <key>NSHighResolutionCapable</key>       <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key> <true/>
    <!-- Lets macOS offer Zervo as a browser for http(s) links. -->
    <key>CFBundleURLTypes</key>
    <array>
        <dict>
            <key>CFBundleURLName</key>       <string>Web site URL</string>
            <key>CFBundleTypeRole</key>      <string>Viewer</string>
            <key>CFBundleURLSchemes</key>
            <array>
                <string>http</string>
                <string>https</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
PLIST

# Ad-hoc signature: does not satisfy Gatekeeper, but keeps the bundle from
# being rejected outright by newer macOS for having no signature at all.
codesign --force --deep --sign - "$APP" 2>/dev/null || \
    echo "    (codesign unavailable; continuing unsigned)"

echo "==> $APP"

if [ "$MAKE_DMG" = "1" ]; then
    DMG="target/Zervo-$VERSION.dmg"
    STAGE="$(mktemp -d)"
    cp -R "$APP" "$STAGE/"
    ln -s /Applications "$STAGE/Applications"

    # Anything downloaded from the internet is quarantined, and macOS refuses to
    # open a quarantined app that is not signed by a paid Developer ID with the
    # singularly unhelpful "Zervo is damaged and can't be opened". Say so here,
    # where someone who has just mounted the disk image will see it, rather than
    # only in release notes they may never have read.
    # Quoted heredoc, so the version goes in afterwards: an unquoted one would
    # also try to expand the "$99/year" below.
    cat > "$STAGE/READ ME FIRST.txt" <<'README'
Zervo @VERSION@
==============

1. Drag Zervo onto the Applications folder here.

2. Open Terminal and run this line:

       xattr -dr com.apple.quarantine /Applications/Zervo.app

3. Open Zervo normally.

Why step 2?
-----------
macOS quarantines everything downloaded from the internet, and refuses to
open quarantined apps unless they are signed with a paid Apple Developer ID
($99/year) and notarised by Apple. Zervo is not, so without step 2 macOS
tells you the app "is damaged and can't be opened", which is not true —
it just means Apple has not been paid to vouch for it.

The command removes the download flag. It does not change the app.

Requirements
------------
- An Apple Silicon Mac (M1 or later). This build will not run on Intel.
- macOS 11 or later.

Zervo runs on Servo, an independent web engine that is still young, so
expect sites to render badly or not at all. It is something to try, not a
browser to depend on.

https://github.com/Goddv/Zervo
README
    sed -i '' "s/@VERSION@/$VERSION/" "$STAGE/READ ME FIRST.txt"

    rm -f "$DMG"
    hdiutil create -volname "Zervo" -srcfolder "$STAGE" -ov -format UDZO "$DMG" > /dev/null
    rm -rf "$STAGE"
    echo "==> $DMG"
fi
