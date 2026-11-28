#!/usr/bin/env bash
# Build only the patched MSE library against an existing GStreamer 1.28.9 SDK.
set -euo pipefail

repo="$(cd "$(dirname -- "$0")/.." && pwd)"
source_dir="${1:-"--download"}"
output="${2:-${CARGO_TARGET_DIR:-$repo/target}/gstreamer-mse}"
archs="${3:-$(uname -m)}"
sdk="${HEION_GSTREAMER_ROOT:-/Library/Frameworks/GStreamer.framework/Versions/1.0}"
export PATH="$sdk/bin:$PATH"
export PKG_CONFIG_PATH="$sdk/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"

if [ "$(pkg-config --modversion gstreamer-mse-1.0)" != 1.28.9 ]; then
    echo "This MSE patch requires a GStreamer 1.28.9 SDK" >&2
    exit 1
fi

mkdir -p "$output"
output="$(cd "$output" && pwd)"
library="$output/libgstmse-1.0.0.dylib"
inputs="$output/build-inputs.sha256"
fingerprint=
if [ "$source_dir" = --download ]; then
    fingerprint="$(
        {
            shasum -a 256 "$repo/scripts/build-gstreamer-mse.sh" "$repo/patches/gstreamer/1.28.9-media-source.patch"
            printf '%s\n' "$sdk" "$archs"
            clang --version
            pkg-config --cflags --libs gstreamer-app-1.0
        } | shasum -a 256 | cut -d ' ' -f1
    )"
    if [ -f "$library" ] && [ -f "$inputs" ] && [ "$(cat "$inputs")" = "$fingerprint" ]; then
        printf '%s\n' "$library"
        exit
    fi
fi

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
relative="subprojects/gst-plugins-bad/gst-libs/gst/mse"
mkdir -p "$scratch/$relative"
if [ "$source_dir" = --download ]; then
    package=gst-plugins-bad-1.28.9
    archive="$output/$package.tar.xz"
    checksum=4a4e0b7bf84f868378c48f143a7ac3a98923b488a5b74f1a35ba46bc3c43f6b1
    if [ ! -f "$archive" ]; then
        curl -fLsS --retry 4 -o "$scratch/$package.tar.xz" \
            "https://gstreamer.freedesktop.org/src/gst-plugins-bad/$package.tar.xz"
        printf '%s  %s\n' "$checksum" "$scratch/$package.tar.xz" | shasum -a 256 -c >&2
        mv "$scratch/$package.tar.xz" "$archive"
    fi
    printf '%s  %s\n' "$checksum" "$archive" | shasum -a 256 -c >&2
    tar -xf "$archive" -C "$scratch" "$package/gst-libs/gst/mse"
    cp "$scratch/$package/gst-libs/gst/mse/"*.{c,h} "$scratch/$relative/"
else
    cp "$source_dir/$relative/"*.{c,h} "$scratch/$relative/"
fi
cd "$scratch"
git apply "$repo/patches/gstreamer/1.28.9-media-source.patch"
cd "$relative"

mkdir -p include/gst/mse
for header in *.h; do
    ln -s "../../../$header" "include/gst/mse/$header"
done
ln -s "$(pkg-config --variable=includedir gstreamer-mse-1.0)/gstreamer-1.0/gst/mse/mse-enumtypes.h" \
    include/gst/mse/mse-enumtypes.h

# The universal SDK's script can retain Cerbero's build-machine shebang.
mkenums="$(command -v glib-mkenums)"
python3 "$mkenums" --fhead '#include <gst/mse/mse-prelude.h>' \
    --vhead 'GST_MSE_PRIVATE GType @enum_name@_get_type (void);\n#define @ENUMPREFIX@_TYPE_@ENUMSHORT@ (@enum_name@_get_type ())' \
    gstmediasourcetrack-private.h > include/gst/mse/mse-enumtypes-private.h

make_enums() {
    local header="$1"
    shift
    python3 "$mkenums" --fhead "#include <$header>" --fprod '#include "@filename@"' \
        --vhead 'GType @enum_name@_get_type (void) { static gsize type; if (g_once_init_enter (&type)) { static const G@Type@Value values[] = {' \
        --vprod '{ @VALUENAME@, "@VALUENAME@", "@valuenick@" },' \
        --vtail '{0, NULL, NULL}}; GType new_type = g_@type@_register_static ("@EnumName@", values); g_once_init_leave (&type, new_type); } return type; }' \
        "$@"
}

make_enums gst/mse/mse.h gstmediasource.h gstmsesrc.h gstsourcebuffer.h gstsourcebufferlist.h \
    > mse-enumtypes-local.c
make_enums gst/mse/mse-enumtypes-private.h gstmediasourcetrack-private.h \
    > mse-enumtypes-private-local.c

arch_flags=()
for arch in $archs; do
    case "$arch" in arm64|x86_64) arch_flags+=(-arch "$arch") ;; *) exit 1 ;; esac
done
# pkg-config prints compiler and linker options as shell words.
# shellcheck disable=SC2046
clang "${arch_flags[@]}" -dynamiclib -mmacosx-version-min=11.0 \
    -DBUILDING_GST_MSE -DGST_USE_UNSTABLE_API '-DG_LOG_DOMAIN="GStreamer-MSE"' \
    -I "$PWD/include" $(pkg-config --cflags gstreamer-app-1.0) \
    -install_name "$library" -compatibility_version 2810.0.0 \
    ./*.c $(pkg-config --libs gstreamer-app-1.0) -o "$scratch/libgstmse-1.0.0.dylib"
mv "$scratch/libgstmse-1.0.0.dylib" "$library"
printf '%s\n' "$fingerprint" > "$scratch/build-inputs.sha256"
mv "$scratch/build-inputs.sha256" "$inputs"
printf '%s\n' "$library"
