#!/usr/bin/env bash
# Prepare the pinned Servo source in the CI checkout's sibling directory.
set -euo pipefail
# shellcheck source=scripts/lib/common.sh
. "$(dirname -- "${BASH_SOURCE[0]}")/lib/common.sh"
cd "$(zervo_repo_root)"

destination=../servo
revision="$(cat patches/servo/upstream-revision)"
[ ! -e "$destination" ] || die "$destination already exists; use a fresh CI checkout"
git init -q "$destination"
git -C "$destination" fetch --depth=1 https://github.com/servo/servo.git "$revision"
git -C "$destination" checkout --detach FETCH_HEAD
[ "$(git -C "$destination" rev-parse HEAD)" = "$revision" ] || die "unexpected Servo commit"
git -C "$destination" apply "$PWD"/patches/servo/*.patch
