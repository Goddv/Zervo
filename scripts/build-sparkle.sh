#!/usr/bin/env bash
# Build Zervo's patched Sparkle framework with full Xcode.
set -euo pipefail
# shellcheck source=scripts/lib/common.sh
. "$(dirname -- "${BASH_SOURCE[0]}")/lib/common.sh"
cd "$(zervo_repo_root)"

output="${1:?usage: build-sparkle.sh OUTPUT_DIRECTORY ARCH}"
arch="${2:?missing architecture}"
case "$arch" in arm64|x86_64) ;; *) die "unsupported architecture: $arch" ;; esac
revision=ab7d71291238df6e0492143b120c9b696a35f603 # Sparkle 2.10.1
work="$(mktemp -d)"
zervo_cleanup_on_exit "$work"
git init -q "$work/src"
git -C "$work/src" fetch --depth=1 https://github.com/sparkle-project/Sparkle.git "$revision"
git -C "$work/src" checkout --detach FETCH_HEAD
[ "$(git -C "$work/src" rev-parse HEAD)" = "$revision" ] || die "unexpected Sparkle revision"
git -C "$work/src" apply "$PWD"/patches/sparkle/*.patch
xcodebuild -project "$work/src/Sparkle.xcodeproj" -scheme Sparkle \
    -configuration Release -derivedDataPath "$work/build" \
    CODE_SIGNING_ALLOWED=NO ONLY_ACTIVE_ARCH=NO "ARCHS=$arch" \
    SPARKLE_BUILD_PACKAGE_SUPPORT=0 \
    SPARKLE_EMBED_INSTALLER_LAUNCHER_XPC_SERVICE=0 SPARKLE_EMBED_DOWNLOADER_XPC_SERVICE=0 \
    COMPILER_INDEX_STORE_ENABLE=NO build >&2
mkdir -p "$output"
[ ! -e "$output/Sparkle.framework" ] || die "Sparkle.framework already exists in $output"
ditto "$work/build/Build/Products/Release/Sparkle.framework" "$output/Sparkle.framework"
cp "$work/src/LICENSE" "$output/Sparkle-LICENSE"
