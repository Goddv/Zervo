#!/usr/bin/env bash
# Refresh the managed Servo checkout and apply Heion's patches.
set -euo pipefail
# shellcheck source=scripts/lib/common.sh
. "$(dirname -- "${BASH_SOURCE[0]}")/lib/common.sh"
cd "$(zervo_repo_root)"

fork=https://github.com/Goddv/servo.git
upstream=https://github.com/servo/servo.git

main_revision() {
    git ls-remote --exit-code "$1" refs/heads/main | cut -f1
}

if [ "${1:-}" = --resolve ]; then
    [ "$#" = 1 ] || die "usage: prepare-servo.sh --resolve"
    upstream_revision="$(main_revision "$upstream")"
    fork_revision="$(main_revision "$fork")"
    printf 'upstream=%s\nfork=%s\n' "$upstream_revision" "$fork_revision"
    exit
fi

mode=latest
if [ "${1:-}" = --pinned ]; then
    mode=pinned
    shift
fi
[ "$#" -le 1 ] || die "usage: prepare-servo.sh [--pinned] [checkout]"
destination="${1:-$(zervo_target_dir)/heion-servo}"

servo() { git -C "$destination" "$@"; }

if [ ! -e "$destination" ] && [ ! -L "$destination" ]; then
    mkdir -p "$(dirname "$destination")"
    mkdir "$destination"
    git init -q "$destination"
    servo config heion.managed true
fi
[ -d "$destination/.git" ] &&
    [ "$(servo config --local --get heion.managed || true)" = true ] ||
    die "$destination already exists and is not a Heion-managed Servo checkout"

# Only one preparation can change the checkout and its shallow history.
scratch="$destination/.git/heion-prepare.lock"
mkdir "$scratch" || die "Servo preparation is already running for $destination"
scratch="$(cd -- "$scratch" && pwd -P)"
zervo_cleanup_on_exit "$scratch"

fetch_revision() {
    local ref="refs/heion/$1" url="$2" revision="$3"
    if [ "$(servo rev-parse --verify -q "$ref" || true)" != "$revision" ]; then
        servo fetch --no-tags --depth=100 "$url" "+$revision:$ref" >&2
    fi
}

if [ "$mode" = pinned ]; then
    revision="$(cat patches/servo/upstream-revision)"
    if ! servo cat-file -e "$revision^{commit}" 2>/dev/null; then
        servo fetch --no-tags --depth=1 "$upstream" "$revision" >&2
    fi
    base="$revision"
    source_tree="$revision"
    say "preparing pinned Servo $revision"
else
    base="${HEION_SERVO_UPSTREAM:-$(main_revision "$upstream")}"
    fork_main="${HEION_SERVO_FORK:-$(main_revision "$fork")}"
    [[ "$base" =~ ^[0-9a-f]{40}$ && "$fork_main" =~ ^[0-9a-f]{40}$ ]] ||
        die "expected Git commit IDs for Servo upstream and fork"
    fetch_revision upstream "$upstream" "$base"
    if [ "$fork_main" != "$base" ]; then
        fetch_revision fork "$fork" "$fork_main"
        if ! servo merge-base "$base" "$fork_main" >/dev/null &&
            [ "$(servo rev-parse --is-shallow-repository)" = true ]; then
            say "fetching more history to find the Servo fork point"
            servo fetch --no-tags --unshallow "$upstream" "$base" >&2
            if ! servo merge-base "$base" "$fork_main" >/dev/null &&
                [ "$(servo rev-parse --is-shallow-repository)" = true ]; then
                servo fetch --no-tags --unshallow "$fork" "$fork_main" >&2
            fi
        fi
    fi
    say "preparing Servo upstream $base with fork main $fork_main"

    if servo merge-base --is-ancestor "$fork_main" "$base"; then
        source_tree="$base"
    elif servo merge-base --is-ancestor "$base" "$fork_main"; then
        source_tree="$fork_main"
    else
        if ! source_tree="$(servo merge-tree --write-tree "$base" "$fork_main")"; then
            printf '%s\n' "$source_tree" >&2
            die "could not merge Servo fork main with upstream main"
        fi
    fi
fi

# Prepare the patched tree in a separate index. Conflicts leave the checkout
# untouched, and an unchanged tree keeps all source mtimes for Cargo.
patched_tree="$(
    export GIT_INDEX_FILE="$scratch/index"
    servo read-tree "$source_tree"
    for patch in "$PWD"/patches/servo/*.patch; do
        if servo apply --cached --reverse --check "$patch" 2>/dev/null; then
            note "already applied: ${patch##*/}"
        elif servo apply --cached --check "$patch" 2>/dev/null; then
            servo apply --cached "$patch"
        else
            servo apply --cached --3way "$patch" >&2 || die "could not apply ${patch##*/}"
        fi
    done
    servo write-tree
)"
servo diff --quiet || die "remove local edits from the managed Servo checkout"
old_tree="$(servo write-tree)"
if [ "$old_tree" != "$patched_tree" ]; then
    servo read-tree -m -u "$old_tree" "$patched_tree"
fi
servo update-ref --no-deref HEAD "$base"
servo config heion.forkRevision "${fork_main:-$base}"
printf '%s\n' "$(cd -- "$destination" && pwd -P)"
