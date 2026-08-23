#!/usr/bin/env bash
# Render the AUR packages from the templates in scripts/aur/.
#
#   ./scripts/make-aur.sh [--output <dir>] [--artifacts <dir>]
#                         [--sha256-x86_64 <sum>] [--sha256-aarch64 <sum>]
#                         [--sha256-src <sum>]
#
# --artifacts is where the release tarballs are looked for, so their checksums
# can be filled in. It defaults to the parent of --output, which is where
# package-linux.sh puts both.
#
# Produces target/linux/aur/{zervo,zervo-bin}/PKGBUILD, and a .SRCINFO beside
# each when makepkg is available (which means: on Arch, or in the Arch
# container CI uses).
#
# Publishing is a separate act and stays manual, because the AUR is a git
# remote over SSH tied to one person's account:
#
#   git clone ssh://aur@aur.archlinux.org/zervo-bin.git
#   cp .../PKGBUILD .../.SRCINFO zervo-bin/ && cd zervo-bin
#   git commit -am "zervo-bin 0.4.1" && git push
#
# The AUR only accepts fast-forward pushes to a branch called `master`, and the
# repository must be flat — it rejects any commit containing a subdirectory
# other than keys/ or LICENSES/.
set -euo pipefail

# shellcheck source=scripts/lib/common.sh
. "$(dirname -- "${BASH_SOURCE[0]}")/lib/common.sh"
cd "$(zervo_repo_root)"

OUTPUT="$(zervo_target_dir)/linux/aur"
ARTIFACTS=""
SHA_X86_64=""
SHA_SRC=""

while [ $# -gt 0 ]; do
    case "$1" in
        --output)         OUTPUT="${2:?--output needs a value}"; shift 2 ;;
        --artifacts)      ARTIFACTS="${2:?--artifacts needs a value}"; shift 2 ;;
        --sha256-x86_64)  SHA_X86_64="${2:?}"; shift 2 ;;
        --sha256-src)     SHA_SRC="${2:?}"; shift 2 ;;
        -h|--help) zervo_usage; exit 0 ;;
        *) die "unknown option: $1" ;;
    esac
done

VERSION="$(zervo_version)"

# If the release artefacts happen to be sitting in target/linux already, use
# their real digests. Otherwise emit SKIP, which is a legal placeholder makepkg
# understands and `updpkgsums` replaces — but which must never be pushed to the
# AUR, so say so loudly.
digest_of() {
    local path="$1"
    [ -f "$path" ] || return 1
    zervo_sha256 "$path" | cut -d' ' -f1
}

# Where the tarballs are. package-linux.sh calls this with --output
# "<its own output>/aur", so the parent is the right default; on its own it is
# the usual target/linux.
LINUX_DIR="${ARTIFACTS:-$(dirname -- "$OUTPUT")}"
[ -n "$SHA_X86_64" ]  || SHA_X86_64="$(digest_of "$LINUX_DIR/zervo-$VERSION-x86_64-linux-gnu.tar.gz" || echo SKIP)"
[ -n "$SHA_SRC" ]     || SHA_SRC="$(digest_of "$LINUX_DIR/zervo-$VERSION-src.tar.gz" || echo SKIP)"

case "$SHA_X86_64$SHA_SRC" in
    *SKIP*) warn "some checksums are SKIP; run again once the release artefacts exist, or pass them in" ;;
esac

render() {
    local template="$1" dir="$2"
    mkdir -p "$dir"
    sed -e "s/@VERSION@/$VERSION/g" \
        -e "s/@SHA256_X86_64@/$SHA_X86_64/g" \
        -e "s/@SHA256_SRC@/$SHA_SRC/g" \
        "$template" > "$dir/PKGBUILD"

    # bash is not makepkg, but it catches the syntax errors that matter and
    # runs anywhere, which is the point of checking here rather than only in
    # the Arch job.
    bash -n "$dir/PKGBUILD" || die "$dir/PKGBUILD is not valid shell"

    # Removed before anything else. The AUR serves .SRCINFO, not PKGBUILD, so a
    # stale one left beside a freshly rendered PKGBUILD is the version users
    # would actually see — and a truncated one from a makepkg that failed
    # halfway is worse still.
    rm -f "$dir/.SRCINFO"
    if [ "$(id -u)" = 0 ]; then
        # makepkg refuses to run as root before it gets as far as looking at
        # --printsrcinfo, so there is nothing to attempt. Containers are root;
        # CI generates it as an unprivileged user.
        note "$dir/PKGBUILD (running as root, so no .SRCINFO)"
    elif command -v makepkg >/dev/null 2>&1; then
        ( cd "$dir" && makepkg --printsrcinfo > .SRCINFO.new && mv .SRCINFO.new .SRCINFO )
        note "$dir/PKGBUILD + .SRCINFO"
    else
        note "$dir/PKGBUILD (no makepkg here, so no .SRCINFO)"
    fi
}

say "AUR packages for $VERSION"
render scripts/aur/PKGBUILD.in     "$OUTPUT/zervo"
render scripts/aur/PKGBUILD-bin.in "$OUTPUT/zervo-bin"
