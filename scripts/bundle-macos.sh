#!/usr/bin/env bash
# Package Heion as a macOS .app, and optionally a .dmg.
#
#   ./scripts/bundle-macos.sh [options]
#
# Options
#   --profile <name>     cargo profile, default release
#   --features <list>    cargo features, comma separated
#   --target <triple>    build for one architecture
#   --universal          build both and lipo them into one binary
#   --binary <path>      use an already-built binary; repeat once per
#                        architecture and they are merged. Implies --no-build.
#   --no-build           do not run cargo; the binary already exists
#   --dmg                also produce a disk image
#   --sign <identity>    codesign with this identity instead of ad-hoc.
#                        Use the full "Developer ID Application: NAME (TEAM)".
#   --notarize           submit the .dmg to Apple and staple the ticket.
#                        Needs --sign, and APPLE_ID / APPLE_TEAM_ID /
#                        APPLE_APP_PASSWORD in the environment.
#   --updates <dir>     embed the patched Sparkle framework built by
#                        scripts/build-sparkle.sh. Set ZERVO_SPARKLE_PUBLIC_KEY
#                        to the public Ed25519 key. Supports ad-hoc signing
#                        and notarized Developer ID Application signing.
#   --output <dir>       where the .app and .dmg go, default the target dir
#
# Without --sign the result is UNSIGNED beyond an ad-hoc signature, and macOS
# will refuse to open it until the quarantine flag is cleared. See
# docs/PACKAGING.md for exactly what a user sees and why.
set -euo pipefail

# shellcheck source=scripts/lib/common.sh
. "$(dirname -- "${BASH_SOURCE[0]}")/lib/common.sh"
cd "$(zervo_repo_root)"

[ "$(uname -s)" = "Darwin" ] || die "this builds a macOS bundle, on macOS"

PROFILE="release"
FEATURES=""
TRIPLE=""
UNIVERSAL=0
BUILD=1
MAKE_DMG=0
IDENTITY="-"
NOTARIZE=0
UPDATER_FRAMEWORK=""
OUTPUT="$(zervo_target_dir)"
PREBUILT=()

while [ $# -gt 0 ]; do
    case "$1" in
        --profile)   PROFILE="${2:?--profile needs a value}"; shift 2 ;;
        --features)  FEATURES="${2:?--features needs a value}"; shift 2 ;;
        --target)    TRIPLE="${2:?--target needs a value}"; shift 2 ;;
        --output)    OUTPUT="${2:?--output needs a value}"; shift 2 ;;
        --sign)      IDENTITY="${2:?--sign needs an identity}"; shift 2 ;;
        --binary)    PREBUILT+=("${2:?--binary needs a path}"); BUILD=0; shift 2 ;;
        --universal) UNIVERSAL=1; shift ;;
        --no-build)  BUILD=0; shift ;;
        --dmg)       MAKE_DMG=1; shift ;;
        --notarize)  NOTARIZE=1; shift ;;
        --updates)   UPDATER_FRAMEWORK="${2:?--updates needs a framework directory}"; shift 2 ;;
        -h|--help) zervo_usage; exit 0 ;;
        *) die "unknown option: $1 (try --help)" ;;
    esac
done

[ "$NOTARIZE" = 1 ] && [ "$IDENTITY" = "-" ] && \
    die "--notarize needs --sign: Apple will not notarise an ad-hoc signature"
# Notarisation happens on the disk image, in the --dmg block. Without one there
# is nothing to submit, and the flag used to be accepted and then quietly do
# nothing at all.
[ "$NOTARIZE" = 1 ] && [ "$MAKE_DMG" = 0 ] && \
    die "--notarize needs --dmg: the disk image is what is submitted and stapled"

if [ -n "$UPDATER_FRAMEWORK" ]; then
    [[ "$IDENTITY" == "-" || ( "$IDENTITY" == "Developer ID Application: "* && "$NOTARIZE" = 1 ) ]] ||
        die "--updates supports ad-hoc or notarized Developer ID Application signing"
    : "${ZERVO_SPARKLE_PUBLIC_KEY:?--updates needs a Sparkle public key}"
    [[ "$ZERVO_SPARKLE_PUBLIC_KEY" =~ ^[A-Za-z0-9+/]{43}=$ ]] ||
        die "Sparkle public key must be a base64-encoded 32-byte key"
    [ -f "$UPDATER_FRAMEWORK/Sparkle.framework/Versions/Current/Sparkle" ] ||
        die "patched Sparkle framework missing from $UPDATER_FRAMEWORK"
fi

VERSION="$(zervo_version)"
RELEASE_VERSION="${VERSION%%+*}"
ARTIFACT_VERSION="${VERSION/+/.}"
# macOS expects numeric bundle versions. Reserve two digits for rebuilds
# within a patch version so the build number also increases on the next patch.
REVISION=0
if [[ "$VERSION" =~ \+([0-9]{1,2})$ ]]; then
    REVISION="${BASH_REMATCH[1]}"
fi
# Increment the public major component so the Heion version reset sorts
# above existing Zervo builds in Sparkle.
IFS=. read -r MAJOR MINOR PATCH <<< "$RELEASE_VERSION"
BUILD_VERSION="$((MAJOR + 1)).$MINOR.$((PATCH * 100 + 10#$REVISION))"
# Read out of .cargo/config.toml rather than written down a second time. The
# bundle's LSMinimumSystemVersion and the compiler's deployment target have to
# be the same number, and the way that goes wrong is somebody changing one of
# two copies.
MIN_MACOS="$(sed -n 's/^MACOSX_DEPLOYMENT_TARGET[^"]*"\([0-9.]*\)".*/\1/p' .cargo/config.toml | head -1)"
[ -n "$MIN_MACOS" ] || die "no MACOSX_DEPLOYMENT_TARGET in .cargo/config.toml"
APP="$OUTPUT/Heion.app"
BIN_DIR="$APP/Contents/MacOS"
RES_DIR="$APP/Contents/Resources"
APP_DISPLAY_NAME=Heion
APP_BUNDLE_ID=app.zervo.browser
WIDGET_TEST=0
case " $(printf '%s' "$FEATURES" | tr ',' ' ') " in
    *" local-widget-test "*)
        WIDGET_TEST=1
        APP_DISPLAY_NAME="Heion Widget Test"
        APP_BUNDLE_ID=app.heion.widgettest
        [ -z "$UPDATER_FRAMEWORK" ] || die "the widget test uses a separate app and profile"
        ;;
esac

# The GStreamer framework ships its own pkg-config that knows where the
# framework is, so putting its bin first is all the build needs to find it.
GSTREAMER_ROOT="/Library/Frameworks/GStreamer.framework/Versions/1.0"
# Normalised, because cargo accepts both `--features a,b` and `--features "a b"`
# and the old comma-only match quietly did not see the second form.
case " $(printf '%s' "$FEATURES" | tr ',' ' ') " in
    *" media "*)
        [ -d "$GSTREAMER_ROOT" ] || die "--features media needs GStreamer at $GSTREAMER_ROOT; see docs/PACKAGING.md"
        export PATH="$GSTREAMER_ROOT/bin:$PATH"
        # glib-sys and gstreamer-sys refuse to run pkg-config for a target that
        # is not the host, and fail the build with "pkg-config has not been
        # configured to support cross-compilation" — which is what building the
        # Intel half on an Apple Silicon machine is. The flag is normally a
        # loaded gun, because one set of .pc files usually describes one
        # architecture. Here it does not: Servo pins the *universal* GStreamer
        # package, and every dylib in the framework carries both slices, so the
        # single set of .pc files is correct for either target.
        export PKG_CONFIG_ALLOW_CROSS=1
        WITH_MEDIA=1 ;;
    *) WITH_MEDIA=0 ;;
esac

# ── The binary, for one architecture or two ───────────────────────────────
if [ "$UNIVERSAL" = 1 ] && [ -n "$TRIPLE" ]; then
    die "--universal and --target are mutually exclusive"
fi
if [ "$UNIVERSAL" = 1 ] && [ "${#PREBUILT[@]}" -gt 0 ]; then
    die "--universal and --binary are mutually exclusive: pass one --binary per architecture and they are merged"
fi

if [ "${#PREBUILT[@]}" -gt 0 ]; then
    for path in "${PREBUILT[@]}"; do
        [ -f "$path" ] || die "no binary at $path"
    done
elif [ "$UNIVERSAL" = 1 ]; then
    for triple in aarch64-apple-darwin x86_64-apple-darwin; do
        if [ "$BUILD" = 1 ]; then
            rustup target add "$triple" >/dev/null 2>&1 || true
            zervo_cargo_build "$PROFILE" "$FEATURES" "$triple"
        fi
        PREBUILT+=("$(zervo_binary_path "$PROFILE" "$triple")")
    done
else
    [ "$BUILD" = 0 ] || zervo_cargo_build "$PROFILE" "$FEATURES" "$TRIPLE"
    PREBUILT+=("$(zervo_binary_path "$PROFILE" "$TRIPLE")")
fi

for path in "${PREBUILT[@]}"; do
    [ -x "$path" ] || die "no binary at $path"
done

say "assembling $APP"
rm -rf "$APP"
mkdir -p "$BIN_DIR" "$RES_DIR"

if [ "${#PREBUILT[@]}" -gt 1 ]; then
    say "lipo: ${#PREBUILT[@]} architectures into one binary"
    lipo -create -output "$BIN_DIR/Heion" "${PREBUILT[@]}"
else
    cp "${PREBUILT[0]}" "$BIN_DIR/Heion"
fi
chmod 755 "$BIN_DIR/Heion"
note "$(lipo -archs "$BIN_DIR/Heion")"

say "icon"
./scripts/make-icns.sh
cp assets/icon/Zervo.icns "$RES_DIR/Zervo.icns"
cp assets/licenses/NotoSansJP-OFL.txt "$RES_DIR/"

if [ "$WITH_MEDIA" = 1 ]; then
    say "bundling GStreamer"
    ./scripts/bundle-gstreamer.py "$APP"

    # The GStreamer framework is universal, so every dylib copied out of it
    # carries both architectures. In a universal bundle that is exactly right.
    # In a single-architecture one it is 81 MB of code that can never run —
    # roughly a quarter of the whole bundle — so it is thinned to match the
    # binary beside it.
    BUNDLE_ARCHS="$(lipo -archs "$BIN_DIR/Heion")"
    case "$BUNDLE_ARCHS" in
        *" "*) : ;;
        *)
            say "thinning bundled libraries to $BUNDLE_ARCHS"
            find "$BIN_DIR/lib" -type f -name '*.dylib' -print0 \
                | while IFS= read -r -d '' lib; do
                      # -thin fails on a file that is already thin, which a
                      # few of them are; nothing is lost when it does.
                      if lipo -thin "$BUNDLE_ARCHS" "$lib" -output "$lib.thin" 2>/dev/null; then
                          mv -f "$lib.thin" "$lib"
                      else
                          rm -f "$lib.thin"
                      fi
                  done
            du -sh "$BIN_DIR/lib" >&2
            ;;
    esac
fi

# ── Info.plist ────────────────────────────────────────────────────────────
# Beyond the obvious keys, three things here matter and were missing:
#
# The NS*UsageDescription strings. macOS kills a process outright — no dialog,
# no log line naming the cause — the moment it touches the camera, microphone
# or location services without a matching string in its Info.plist. A browser
# reaches all three the first time a page calls getUserMedia or geolocation, so
# without these Zervo simply disappears on sites that ask.
#
# CFBundleDocumentTypes, which is what makes "Open With > Zervo" appear for an
# .html file, and LSHandlerRank so it does not claim to be the system's answer
# for HTML on installation.
#
# LSMinimumSystemVersion, derived from the same number the compiler was given
# rather than typed in beside it.
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>                  <string>$APP_DISPLAY_NAME</string>
    <key>CFBundleDisplayName</key>           <string>$APP_DISPLAY_NAME</string>
    <key>CFBundleIdentifier</key>            <string>$APP_BUNDLE_ID</string>
    <key>CFBundleExecutable</key>            <string>Heion</string>
    <key>CFBundleIconFile</key>              <string>Zervo</string>
    <key>CFBundlePackageType</key>           <string>APPL</string>
    <key>CFBundleShortVersionString</key>    <string>$RELEASE_VERSION</string>
    <key>CFBundleVersion</key>               <string>$BUILD_VERSION</string>
    <key>CFBundleInfoDictionaryVersion</key> <string>6.0</string>
    <key>LSApplicationCategoryType</key>     <string>public.app-category.utilities</string>
    <key>LSMinimumSystemVersion</key>        <string>$MIN_MACOS</string>
    <key>NSHighResolutionCapable</key>       <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key> <true/>
    <key>NSHumanReadableCopyright</key>      <string>Mozilla Public License 2.0</string>
    <key>ITSAppUsesNonExemptEncryption</key> <false/>

    <!-- Without these three, macOS terminates the process the first time a
         page asks for the device rather than showing a permission prompt. -->
    <key>NSCameraUsageDescription</key>
    <string>A web page has asked to use your camera.</string>
    <key>NSMicrophoneUsageDescription</key>
    <string>A web page has asked to use your microphone.</string>
    <key>NSLocationWhenInUseUsageDescription</key>
    <string>A web page has asked for your location.</string>

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

    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>      <string>HTML document</string>
            <key>CFBundleTypeRole</key>      <string>Viewer</string>
            <key>LSHandlerRank</key>         <string>Alternate</string>
            <key>LSItemContentTypes</key>
            <array>
                <string>public.html</string>
                <string>public.xhtml</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
PLIST

if [ "$WIDGET_TEST" = 1 ]; then
    plutil -remove CFBundleURLTypes "$APP/Contents/Info.plist"
    plutil -remove CFBundleDocumentTypes "$APP/Contents/Info.plist"
fi

# macOS can render the layered icon in its selected appearance when the
# active Xcode toolchain supports Icon Composer. Keep the flat ICNS fallback.
if xcrun --find actool >/dev/null 2>&1; then
    ICON_BUILD="$(mktemp -d)"
    if xcrun --sdk macosx actool assets/icon/Heion.icon \
        --compile "$ICON_BUILD" --app-icon Heion --platform macosx \
        --target-device mac --minimum-deployment-target 26.0 \
        --output-partial-info-plist "$ICON_BUILD/Icon.plist" --errors --warnings \
        && [ -s "$ICON_BUILD/Assets.car" ]; then
        cp "$ICON_BUILD/Assets.car" "$RES_DIR/Assets.car"
        plutil -insert CFBundleIconName -string Heion "$APP/Contents/Info.plist"
    else
        note "Icon Composer compilation unavailable. Using Zervo.icns."
    fi
    rm -rf "$ICON_BUILD"
fi

# Point zlib at the system copy.
#
# If GStreamer is installed, its `-sys` build scripts put the framework's lib
# directory on the link path whether or not `--features media` asked for it, and
# `-lz` finds the zlib in there before the system one. That copy's install name
# is `@rpath/libz.1.dylib`, and nothing adds an rpath — so the app died at
# launch with "Library not loaded", on the developer's machine only, and only
# once they had installed GStreamer for something else.
#
# Done here rather than with a linker flag because a flag in `.cargo/config.toml`
# is silently dropped when RUSTFLAGS is set in the environment, which is exactly
# what CI does. This is unconditional, and a no-op when the link was already
# right.
BIN="$BIN_DIR/Heion"
if otool -L "$BIN" | grep -q "@rpath/libz.1.dylib"; then
    say "repointing zlib at the system copy"
    install_name_tool -change @rpath/libz.1.dylib /usr/lib/libz.1.dylib "$BIN"
fi

# ── Signing ───────────────────────────────────────────────────────────────
# Nested code first, then the bundle. `codesign --deep` would do both in one
# call, and Apple has deprecated it for years: it signs everything with the
# same options, which is wrong for a bundle whose nested libraries want
# different treatment, and its behaviour on re-signing is not what anyone
# expects. Signing inside-out is the documented replacement.
say "signing (${IDENTITY})"
# --sign before --options, always: codesign silently ignores --options when it
# comes first, and a signature that quietly lost its hardened runtime fails
# notarisation with an error that says nothing about argument order.
SIGN_ARGS=(--force --sign "$IDENTITY")
BUNDLE_SIGN_ARGS=(--force --sign "$IDENTITY")
if [ "$IDENTITY" != "-" ]; then
    # Gatekeeper will not accept a Developer ID signature without the hardened
    # runtime, and notarisation rejects it outright.
    SIGN_ARGS+=(--options runtime --timestamp)
    # ...and the hardened runtime forbids writing to memory and then executing
    # it, which is what a JavaScript JIT is. Without these entitlements a
    # signed build crashes on the first page that runs a script. They go on the
    # bundle only: the nested libraries are libraries and do not JIT anything.
    BUNDLE_SIGN_ARGS+=(--options runtime --timestamp
                       --entitlements assets/macos/entitlements.plist)
fi

# Updater builds always require intact signatures on the app and its helpers.
# Keep best-effort signing for local bundles built without an updater.
STRICT_SIGNATURES=1
if [ "$IDENTITY" = "-" ] && [ -z "$UPDATER_FRAMEWORK" ]; then
    STRICT_SIGNATURES=0
fi
if [ "$STRICT_SIGNATURES" = 0 ]; then
    sign_or_fail() { codesign "${SIGN_ARGS[@]}" "$1" || warn "codesign failed on $(basename -- "$1")"; }
else
    sign_or_fail() { codesign "${SIGN_ARGS[@]}" "$1" || die "codesign failed on $1"; }
fi

if [ -d "$BIN_DIR/lib" ]; then
    # Process substitution rather than a pipeline: `find | while` puts the loop
    # in a subshell, where `set -e` cannot reach it and `die` would exit the
    # subshell and nothing else. -print0 and `read -d ''` so a path with a
    # space in it survives.
    while IFS= read -r -d '' lib; do
        sign_or_fail "$lib"
    done < <(find "$BIN_DIR/lib" -type f \( -name '*.dylib' -o -name '*.so' \) -print0)
fi
if [ -n "$UPDATER_FRAMEWORK" ]; then
    UPDATE_ARCH="$(lipo -archs "$BIN")"
    case "$UPDATE_ARCH" in
        arm64|x86_64) ;;
        *) die "updater-enabled releases require a single architecture" ;;
    esac
    # shellcheck source=scripts/lib/bundle-updater.sh
    . scripts/lib/bundle-updater.sh
    UPDATE_FEED="https://github.com/Goddv/Heion/releases/latest/download/zervo-appcast-$UPDATE_ARCH.xml"
    zervo_bundle_updater "$UPDATER_FRAMEWORK" "$APP" "$UPDATE_FEED" "$ZERVO_SPARKLE_PUBLIC_KEY" "$UPDATE_ARCH"
fi

# Image Playground is an optional, Apple Silicon-only framework on macOS 15.1
# and newer. Load this bridge on demand so the browser's deployment floor stays
# independent of the wallpaper generator.
case " $(lipo -archs "$BIN") " in
    *" arm64 "*)
        SDK="$(xcrun --sdk macosx --show-sdk-path)"
        if [ -d "$SDK/System/Library/Frameworks/ImagePlayground.framework" ]; then
            mkdir -p "$APP/Contents/Frameworks"
            PLAYGROUND="$APP/Contents/Frameworks/HeionImagePlayground.dylib"
            xcrun --sdk macosx swiftc -sdk "$SDK" -target arm64-apple-macosx15.1 \
                -parse-as-library -emit-library -O -framework AppKit -framework ImagePlayground \
                -Xlinker -install_name -Xlinker '@rpath/HeionImagePlayground.dylib' \
                platform/macos/HeionImagePlayground.swift -o "$PLAYGROUND"
            sign_or_fail "$PLAYGROUND"
        fi
        # The widget composer loads its bridge only where Foundation Models is
        # available. The browser executable retains its own deployment target.
        if [ -d "$SDK/System/Library/Frameworks/FoundationModels.framework" ]; then
            mkdir -p "$APP/Contents/Frameworks"
            WIDGET_PLAYGROUND="$APP/Contents/Frameworks/HeionWidgetPlayground.dylib"
            xcrun --sdk macosx swiftc -sdk "$SDK" -target arm64-apple-macosx26.0 \
                -parse-as-library -emit-library -O -framework AVFoundation -framework Speech \
                -framework FoundationModels \
                -Xlinker -install_name -Xlinker '@rpath/HeionWidgetPlayground.dylib' \
                platform/macos/HeionWidgetPlayground.swift -o "$WIDGET_PLAYGROUND"
            sign_or_fail "$WIDGET_PLAYGROUND"
            plutil -replace NSMicrophoneUsageDescription -string "Heion uses the microphone when you dictate a New Tab widget prompt or give a website access." "$APP/Contents/Info.plist"
            plutil -insert NSSpeechRecognitionUsageDescription -string "Heion transcribes your New Tab widget prompt on this Mac when you choose Speak." "$APP/Contents/Info.plist"
        elif [ "$WIDGET_TEST" = 1 ]; then
            die "the widget test needs an SDK with Foundation Models"
        fi
        ;;
esac

if [ "$STRICT_SIGNATURES" = 0 ]; then
    codesign "${BUNDLE_SIGN_ARGS[@]}" "$APP" || warn "codesign failed on the bundle"
else
    codesign "${BUNDLE_SIGN_ARGS[@]}" "$APP" || die "codesign failed on $APP"
fi

if ! codesign --verify --deep --strict "$APP"; then
    if [ "$STRICT_SIGNATURES" = 0 ]; then
        warn "the bundle does not verify"
    else
        die "the bundle does not verify after signing with $IDENTITY"
    fi
fi

say "$APP"
du -sh "$APP" >&2

# ── Disk image ────────────────────────────────────────────────────────────
if [ "$MAKE_DMG" = 1 ]; then
    # "arm64", "x86_64", or "universal" when it carries both — nobody wants to
    # work out which of Zervo-0.4.1-x86_64-arm64.dmg to download.
    ARCH_LIST="$(lipo -archs "$BIN_DIR/Heion")"
    case "$ARCH_LIST" in
        *" "*) ARCH_SLUG="universal" ;;
        *)     ARCH_SLUG="$ARCH_LIST" ;;
    esac
    # "Intel and Apple Silicon", via a placeholder so the substitution for the
    # separator cannot fire inside a name it has already written.
    ARCH_HUMAN="$(printf '%s' "$ARCH_LIST" | sed -e 's/ /+/g' -e 's/arm64/Apple Silicon/g' -e 's/x86_64/Intel/g' -e 's/+/ and /g')"
    DMG="$OUTPUT/Heion-$ARTIFACT_VERSION-$ARCH_SLUG.dmg"
    STAGE="$(mktemp -d)"
    zervo_cleanup_on_exit "$STAGE"

    if [ "$NOTARIZE" = 1 ]; then
        : "${APPLE_ID:?--notarize needs APPLE_ID}"
        : "${APPLE_TEAM_ID:?--notarize needs APPLE_TEAM_ID}"
        : "${APPLE_APP_PASSWORD:?--notarize needs APPLE_APP_PASSWORD}"
        notarize() {
            xcrun notarytool submit "$1" --apple-id "$APPLE_ID" \
                --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" --wait
        }
        # Delta recipients only receive the reconstructed .app. Staple its own
        # notarization ticket before making the disk image used as a baseline.
        if [ -n "$UPDATER_FRAMEWORK" ]; then
            NOTARY_STAGE="$(mktemp -d)"
            zervo_cleanup_on_exit "$NOTARY_STAGE"
            ditto -c -k --keepParent "$APP" "$NOTARY_STAGE/Heion.zip"
            notarize "$NOTARY_STAGE/Heion.zip"
            xcrun stapler staple "$APP"
        fi
    fi
    cp -R "$APP" "$STAGE/"
    ln -s /Applications "$STAGE/Applications"

    # Anything downloaded from the internet is quarantined, and macOS refuses to
    # open a quarantined app that is not signed by a paid Developer ID with the
    # singularly unhelpful "Zervo is damaged and can't be opened". Say so here,
    # where someone who has just mounted the disk image will see it, rather than
    # only in release notes they may never have read.
    # Quoted heredoc, so the substitutions happen afterwards: an unquoted one
    # would also try to expand the "$99/year" below.
    if [ "$NOTARIZE" = 1 ]; then
        cat > "$STAGE/READ ME FIRST.txt" <<'README_SIGNED'
Heion @VERSION@

Drag the app into Applications, then open it normally.
Requires macOS @MINMACOS@ or later and runs on @ARCHS@.
README_SIGNED
    else
        cat > "$STAGE/READ ME FIRST.txt" <<'README'
Heion @VERSION@
==============

1. Drag the app onto the Applications folder here.

2. Open Terminal and run:

       xattr -dr com.apple.quarantine /Applications/Heion.app

3. Open Heion normally.

Why step 2?
-----------
macOS quarantines everything downloaded from the internet, and refuses to
open a quarantined app unless it is signed with a paid Apple Developer ID
($99/year) and notarised by Apple. This build is ad-hoc signed, so without step 2 macOS
tells you the app "is damaged and can't be opened", which is not true —
it just means Apple has not been paid to vouch for it.

The command removes the download flag. It does not change the app.

There is no way round it in the interface. On macOS 26 and later, an app
signed the way this one is does not get an "Open Anyway" button in
System Settings; it is offered to the Trash instead. Control-click > Open
stopped working in Sequoia. The command above is the only thing that does.

Requirements
------------
- macOS @MINMACOS@ or later.
- This build runs on @ARCHS@.

Heion runs on Servo, an independent web engine that is still young, so
expect sites to render badly or not at all. It is something to try, not a
browser to depend on.

https://github.com/Goddv/Heion
README
    fi
    MIN_MACOS="$(plutil -extract LSMinimumSystemVersion raw "$APP/Contents/Info.plist")"
    sed -i '' -e "s/@VERSION@/$ARTIFACT_VERSION/" \
              -e "s/@MINMACOS@/$MIN_MACOS/" \
              -e "s/@ARCHS@/$ARCH_HUMAN/" \
              "$STAGE/READ ME FIRST.txt"

    rm -f "$DMG"
    # ULFO is LZFSE, and on a bundle this size it is both smaller and faster
    # than the UDZO default — measured at 110 MB in 10s against UDZO's 123 MB
    # in 22s on a 338 MB Zervo.app. It still mounts in the kernel, unlike ULMO,
    # which saves another 27 MB and costs two minutes and a helper process. It
    # needs macOS 10.11, comfortably below this bundle's own floor.
    if ! hdiutil create -volname "Heion $ARTIFACT_VERSION" -srcfolder "$STAGE" -ov \
            -format ULFO "$DMG" > /dev/null 2>&1; then
        warn "ULFO unavailable; falling back to UDZO"
        hdiutil create -volname "Heion $ARTIFACT_VERSION" -srcfolder "$STAGE" -ov \
            -format UDZO "$DMG" > /dev/null
    fi

    if [ "$IDENTITY" != "-" ]; then
        codesign --force --sign "$IDENTITY" --timestamp "$DMG"
    fi

    if [ "$NOTARIZE" = 1 ]; then
        say "notarising"
        notarize "$DMG"
        xcrun stapler staple "$DMG"
        note "stapled; this image opens without any quarantine step"
    fi

    zervo_sha256 "$DMG" >&2
    say "$DMG"
fi
