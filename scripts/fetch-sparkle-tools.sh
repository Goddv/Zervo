#!/usr/bin/env bash
# Fetch Sparkle's pinned, unmodified release signing tools.
set -euo pipefail
# shellcheck source=scripts/lib/common.sh
. "$(dirname -- "${BASH_SOURCE[0]}")/lib/common.sh"

output="${1:?usage: fetch-sparkle-tools.sh OUTPUT_DIRECTORY}"
version=2.10.1
sha=1a856d2a9edb8e628de88121d7be04cf0e2167f8641650b78fca00b14244cf27
work="$(mktemp -d)"
zervo_cleanup_on_exit "$work"
curl --fail --location --silent --show-error --retry 3 \
    "https://github.com/sparkle-project/Sparkle/releases/download/$version/Sparkle-$version.tar.xz" \
    --output "$work/sparkle.tar.xz"
printf '%s  %s\n' "$sha" "$work/sparkle.tar.xz" | shasum -a 256 --check
mkdir -p "$output"
tar -xf "$work/sparkle.tar.xz" -C "$output" ./bin
