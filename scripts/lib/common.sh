# shellcheck shell=bash
# Helpers shared by every packaging script. Sourced, never executed.
#
#   . "$(dirname "$0")/lib/common.sh"
#
# Everything here has to work on both GNU userland and the bash 3.2 / BSD
# userland that ships with macOS, because bundle-macos.sh runs on a developer's
# Mac and package-linux.sh runs in a container. No associative arrays, no
# `${var,,}`, no `readarray`, no GNU-only flags outside the Linux-only helpers.

# ── Output ────────────────────────────────────────────────────────────────
# Sent to stderr so a helper that prints a value on stdout can still be used in
# a command substitution while narrating what it is doing.
say()  { printf '==> %s\n' "$*" >&2; }
note() { printf '    %s\n' "$*" >&2; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die()  { printf 'error: %s\n' "$*" >&2; exit 1; }

# Print a script's leading comment block as its usage message: every line after
# the shebang, up to the first that is not a comment. The scripts used to do
# this with `sed -n '2,26p'`, which meant every edit above the cut silently
# moved it — and which spilled `set -euo pipefail` into --help output more than
# once.
zervo_usage() {
    # Takes no argument: the caller has already cd'd to the repository root, so
    # a path relative to wherever the user was standing no longer resolves.
    # BASH_SOURCE[1] is the script that called this, and every one of them
    # lives in scripts/.
    awk 'NR == 1 { next } /^#/ { sub(/^# ?/, ""); print; next } { exit }' \
        "scripts/$(basename -- "${BASH_SOURCE[1]}")"
}

need() {
    for _cmd in "$@"; do
        command -v "$_cmd" >/dev/null 2>&1 || die "$_cmd not found on PATH"
    done
}

# ── The repository ────────────────────────────────────────────────────────
# Every script cds to the repository root first, so paths below are relative to
# it. Resolved from this file's own location rather than $0, so sourcing works
# the same however the caller was invoked.
zervo_repo_root() {
    CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd
}

# The version of the `zervo` binary package.
#
# Parsed out of the [package] section specifically. The old one-line
# `sed -n 's/^version = "\(.*\)"/\1/p' | head -1` took the first `version =` at
# column zero anywhere in the file, which is the right answer today only
# because nothing precedes [package]. Adding a [workspace.package] block — the
# ordinary way to share a version across a workspace — would have silently
# stamped the wrong number onto every package.
zervo_version() {
    local version
    version="$(sed -n '/^\[package\]/,/^\[[a-z]/{
        s/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p
    }' Cargo.toml | head -1)"
    [ -n "$version" ] || die "no version in the [package] section of Cargo.toml"
    printf '%s\n' "$version"
}

# Where cargo puts its output. Honours CARGO_TARGET_DIR, which CI sets on
# Windows to keep paths short.
zervo_target_dir() {
    printf '%s\n' "${CARGO_TARGET_DIR:-target}"
}

# The directory a given profile builds into.
#
# Cargo's rule: `dev` and `test` land in debug/, `release` and `bench` land in
# release/, and any custom profile lands in a directory of its own name. The
# old scripts special-cased `dev` and got `test` and `bench` wrong — which
# mattered less than it might have, but a packaging script that looks for a
# binary in the wrong place fails with "no binary at ...", not with anything
# that names the profile.
zervo_profile_dir() {
    case "$1" in
        dev|test) printf 'debug\n' ;;
        release|bench) printf 'release\n' ;;
        *) printf '%s\n' "$1" ;;
    esac
}

# The built binary for a profile, and optionally a --target triple.
zervo_binary_path() {
    local profile="$1" triple="${2:-}" dir
    dir="$(zervo_target_dir)"
    [ -n "$triple" ] && dir="$dir/$triple"
    printf '%s/%s/zervo%s\n' "$dir" "$(zervo_profile_dir "$profile")" "${3:-}"
}

# ── Building ──────────────────────────────────────────────────────────────
# One place that knows how to invoke cargo, so every package is built the same
# way. --locked is the point: a release must resolve the dependency graph the
# committed Cargo.lock describes, and nothing else. Cargo will fail rather than
# quietly update the lockfile.
zervo_cargo_build() {
    local profile="$1" features="$2" triple="${3:-}"
    local args
    args=(build --profile "$profile")
    [ "${ZERVO_LOCKED:-1}" = "1" ] && args+=(--locked)
    [ -n "$features" ] && args+=(--features "$features")
    [ -n "$triple" ] && args+=(--target "$triple")
    if [ -n "${ZERVO_CARGO_ARGS:-}" ]; then
        # Deliberate word splitting: this is a caller-supplied argument list.
        # shellcheck disable=SC2206
        local extra=(${ZERVO_CARGO_ARGS})
        # bash 3.2 — the /bin/bash on every Mac — treats "${empty[@]}" as an
        # unbound variable under `set -u`, and ZERVO_CARGO_ARGS=" " splits to
        # exactly that.
        if [ "${#extra[@]}" -gt 0 ]; then
            args+=("${extra[@]}")
        fi
    fi

    say "cargo ${args[*]}"
    cargo "${args[@]}"
}

# Remove a working directory when the script ends, however it ends.
#
# Three traps rather than one on `EXIT INT TERM`: a signal handler runs and then
# execution *resumes* where it was interrupted, so a single combined trap
# deletes the working tree and then lets the rest of the script carry on using
# it. Exiting from the signal handlers is what reaches the EXIT trap, which is
# the one that actually cleans up. 130 and 143 are the conventional statuses
# for SIGINT and SIGTERM.
zervo_cleanup_on_exit() {
    ZERVO_TMPDIR="$1"
    trap 'rm -rf -- "$ZERVO_TMPDIR"' EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
}

# ── Reproducibility ───────────────────────────────────────────────────────
# dpkg-deb, rpmbuild, tar and mksquashfs all honour SOURCE_DATE_EPOCH and
# stamp it instead of the wall clock, so two builds of the same commit produce
# byte-identical archives. Derived from the commit itself; falls back to the
# packaging script's own start time outside a git checkout.
zervo_source_date_epoch() {
    if [ -n "${SOURCE_DATE_EPOCH:-}" ]; then
        printf '%s\n' "$SOURCE_DATE_EPOCH"
    elif git rev-parse --git-dir >/dev/null 2>&1; then
        git log -1 --pretty=%ct
    else
        date +%s
    fi
}

# RFC-2822-ish date for an RPM %changelog entry, in the C locale rpm insists
# on. rpmbuild checks the weekday against the date and fails the build if they
# disagree, which is exactly what a hand-written changelog line drifts into.
zervo_rpm_changelog_date() {
    local epoch="$1"
    if date -u -r "$epoch" '+%a %b %d %Y' 2>/dev/null; then :   # BSD
    else LC_ALL=C date -u -d "@$epoch" '+%a %b %d %Y'; fi       # GNU
}

# ── Architecture ──────────────────────────────────────────────────────────
# uname's name for the machine, mapped into each packaging system's own.
# Each of these refuses to guess. dpkg-deb does not validate the Architecture
# field and rpmbuild is not much better, so passing an unrecognised name
# straight through produced a package that built cleanly, exited zero, and
# could not be installed anywhere.
zervo_deb_arch() {
    case "${1:-$(uname -m)}" in
        x86_64|amd64) printf 'amd64\n' ;;
        aarch64|arm64) printf 'arm64\n' ;;
        armv7l|armhf) printf 'armhf\n' ;;
        i686|i386) printf 'i386\n' ;;
        *) die "no Debian architecture name for '${1:-$(uname -m)}'" ;;
    esac
}

zervo_rpm_arch() {
    case "${1:-$(uname -m)}" in
        x86_64|amd64) printf 'x86_64\n' ;;
        aarch64|arm64) printf 'aarch64\n' ;;
        *) die "no RPM architecture name for '${1:-$(uname -m)}'" ;;
    esac
}

# The name AppImage's runtime is published under, which is uname's spelling.
# Only these two are published for the runtime and for linuxdeploy.
zervo_appimage_arch() {
    case "${1:-$(uname -m)}" in
        x86_64|amd64) printf 'x86_64\n' ;;
        aarch64|arm64) printf 'aarch64\n' ;;
        *) die "no AppImage runtime is published for '${1:-$(uname -m)}'" ;;
    esac
}

# ── Metadata every package repeats ────────────────────────────────────────
# Read by the scripts that source this file, not by this file itself.
# shellcheck disable=SC2034
ZERVO_SUMMARY="A sidebar-first browser built on the Servo engine"
# shellcheck disable=SC2034
ZERVO_HOMEPAGE="https://github.com/Goddv/Zervo"
# shellcheck disable=SC2034
ZERVO_MAINTAINER="Goddv <habitat.cristal@gmail.com>"
# shellcheck disable=SC2034
ZERVO_LICENSE="MPL-2.0"
# shellcheck disable=SC2034
ZERVO_APPID="app.zervo.Zervo"
# shellcheck disable=SC2034
ZERVO_DESCRIPTION="Zervo is a browser chrome — sidebar, workspaces, tabs, settings —
wrapped around Servo, the independent web engine written in Rust. There is no
Chromium and no Gecko underneath.

Servo is not yet a complete web engine, so some sites will not work. This is an
experiment rather than a browser to depend on."

# ── Checksums ─────────────────────────────────────────────────────────────
# sha256sum is GNU, shasum is everywhere else. Prints the usual
# "<digest>  <basename>" so the file can be verified from its own directory.
zervo_sha256() {
    local file="$1" dir base
    dir="$(dirname -- "$file")"
    base="$(basename -- "$file")"
    if command -v sha256sum >/dev/null 2>&1; then
        (cd "$dir" && sha256sum "$base")
    else
        (cd "$dir" && shasum -a 256 "$base")
    fi
}

# ── The install tree ──────────────────────────────────────────────────────
# One staging tree, laid out exactly as it lands on disk, shared by the .deb,
# the .rpm, the tarball and the AppDir. New formats reuse this rather than
# rebuilding the layout, which is what keeps them from drifting apart.
#
# GNU coreutils only (`install -D`), so Linux packaging only.
#
#   zervo_stage_tree <destdir> <binary> [prefix]
zervo_stage_tree() {
    local stage="$1" binary="$2" prefix="${3:-/usr}"

    install -Dm755 "$binary" "$stage$prefix/bin/zervo"
    # Stripped, because that is what a distribution package is. rpmbuild does
    # it already — brp-strip runs whenever __debug_package is undefined, which
    # `%global debug_package %{nil}` leaves it — so without this the .deb and
    # the .rpm built from one tree disagreed, and lintian said so. There is
    # very little to remove either way: the release profile carries no debug
    # info, so this is a symbol table, not symbols.
    if command -v strip >/dev/null 2>&1; then
        strip --strip-unneeded "$stage$prefix/bin/zervo" 2>/dev/null || true
    fi
    install -Dm644 assets/linux/app.zervo.Zervo.desktop \
        "$stage$prefix/share/applications/app.zervo.Zervo.desktop"
    # Scalable, so there is nothing to resize and no image tooling to depend on.
    install -Dm644 assets/icon/zervo-icon-default.svg \
        "$stage$prefix/share/icons/hicolor/scalable/apps/zervo.svg"
    # ...but a software centre wants a raster it can show without a renderer,
    # and appimagetool wants one at 256x256. The directory names in hicolor are
    # contractual — every icon cache takes them at their word — so each of
    # these really is the size it says. They are rendered from the same 1024px
    # master by scripts/make-icons.py.
    # 128, 256 and 512 only: hicolor's index.theme stops at 512, so a
    # 1024x1024 directory is one no package owns and no icon cache reads.
    for size in 128 256 512; do
        install -Dm644 "assets/icon/zervo-$size.png" \
            "$stage$prefix/share/icons/hicolor/${size}x${size}/apps/zervo.png"
    done
    install -Dm644 assets/linux/app.zervo.Zervo.metainfo.xml \
        "$stage$prefix/share/metainfo/app.zervo.Zervo.metainfo.xml"
    install -Dm644 assets/linux/zervo.1 "$stage$prefix/share/man/man1/zervo.1"
    gzip -9n -f "$stage$prefix/share/man/man1/zervo.1"
    install -Dm644 LICENSE "$stage$prefix/share/licenses/zervo/LICENSE"
    # Debian looks for the licence at a different path, and a package without
    # one is a policy violation lintian reports at error level. Same file,
    # both places — the .rpm ignores the second and the .deb ignores the first.
    install -Dm644 LICENSE "$stage$prefix/share/doc/zervo/copyright"
    install -Dm644 README.md "$stage$prefix/share/doc/zervo/README.md"
    install -Dm644 CHANGELOG.md "$stage$prefix/share/doc/zervo/CHANGELOG.md"
    # Debian requires a changelog at this exact name, gzipped, and reports its
    # absence as an error. Same content, spelled the way policy asks.
    install -Dm644 CHANGELOG.md "$stage$prefix/share/doc/zervo/changelog"
    gzip -9n -f "$stage$prefix/share/doc/zervo/changelog"
}

# What Zervo shells out to at runtime on Linux, per packaging system.
#
#   zenity      the file chooser for <input type=file>  (src/platform.rs)
#   xdg-open    opening a finished download              (src/downloads.rs)
#   secret-tool storing a password in the keyring        (src/passwords.rs)
#
# None of these are linked, so no dependency generator finds them; without them
# the feature silently does nothing, which is the worst way to be missing.
# shellcheck disable=SC2034
ZERVO_DEB_RECOMMENDS="zenity, xdg-utils, libsecret-tools, hicolor-icon-theme, desktop-file-utils"
# shellcheck disable=SC2034
ZERVO_RPM_RECOMMENDS="zenity xdg-utils libsecret hicolor-icon-theme"

# And the GStreamer plugins, when the build has media in it. These are dlopened
# by name at runtime, so nothing links them and neither dpkg-shlibdeps nor
# rpmbuild can see them: a package built with --features media, installed on a
# machine without them, plays nothing and says nothing about why. The engine's
# own library does get picked up by both generators; only the plugins are
# invisible.
# shellcheck disable=SC2034
ZERVO_DEB_MEDIA_RECOMMENDS="gstreamer1.0-plugins-base, gstreamer1.0-plugins-good, gstreamer1.0-plugins-bad, gstreamer1.0-plugins-ugly, gstreamer1.0-libav"
# Fedora's free repositories only. gstreamer1-libav lives in RPM Fusion, and a
# weak dependency on something no enabled repository provides is skipped in
# silence, which is worse than not asking.
# shellcheck disable=SC2034
ZERVO_RPM_MEDIA_RECOMMENDS="gstreamer1-plugins-base gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-plugins-ugly-free"
