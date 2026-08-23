#!/usr/bin/env bash
# Package Zervo for Linux.
#
#   ./scripts/package-linux.sh [options] <format>...
#
# Formats
#   --deb        Debian/Ubuntu package        (needs dpkg-deb)
#   --rpm        Fedora/RHEL package          (needs rpmbuild)
#   --tarball    portable .tar.gz             (what the AUR -bin package eats)
#   --appimage   single-file AppImage         (delegates to build-appimage.sh)
#   --pkgbuild   PKGBUILD + .SRCINFO for the AUR
#   --all        everything the machine has the tools for
#
# Options
#   --profile <name>    cargo profile, default release
#   --features <list>   cargo features, comma separated
#   --target <triple>   cross-compile, and look for the binary under it
#   --no-build          package a binary that is already built
#   --output <dir>      where the packages go, default target/linux
#   --version-suffix <s>  append "+<s>" to the .deb version, so packages built
#                       against different distributions can sit in one release
#                       without colliding. The .rpm needs no equivalent: its
#                       %{?dist} macro already puts .fc44 in the filename.
#
# Build each package on the distribution it targets. A binary linked against
# Ubuntu's libraries will not reliably run on Fedora, and both dpkg-shlibdeps
# and rpmbuild derive the package's dependencies from the binary, which means
# those dependencies have to be that distribution's sonames. CI builds the .deb
# on Ubuntu and the .rpm in a Fedora container rather than producing both from
# one build.
set -euo pipefail

# shellcheck source=scripts/lib/common.sh
. "$(dirname -- "${BASH_SOURCE[0]}")/lib/common.sh"
cd "$(zervo_repo_root)"

PROFILE="release"
FEATURES=""
TRIPLE=""
BUILD=1
VERSION_SUFFIX=""
OUTPUT="$(zervo_target_dir)/linux"
WANT_DEB=0 WANT_RPM=0 WANT_TARBALL=0 WANT_APPIMAGE=0 WANT_PKGBUILD=0

while [ $# -gt 0 ]; do
    case "$1" in
        --profile)  PROFILE="${2:?--profile needs a value}"; shift 2 ;;
        --features) FEATURES="${2:?--features needs a value}"; shift 2 ;;
        --target)   TRIPLE="${2:?--target needs a value}"; shift 2 ;;
        --output)   OUTPUT="${2:?--output needs a value}"; shift 2 ;;
        --version-suffix) VERSION_SUFFIX="${2:?--version-suffix needs a value}"; shift 2 ;;
        --no-build) BUILD=0; shift ;;
        --deb)      WANT_DEB=1; shift ;;
        --rpm)      WANT_RPM=1; shift ;;
        --tarball)  WANT_TARBALL=1; shift ;;
        --appimage) WANT_APPIMAGE=1; shift ;;
        --pkgbuild) WANT_PKGBUILD=1; shift ;;
        --all)
            WANT_TARBALL=1
            WANT_PKGBUILD=1
            command -v dpkg-deb >/dev/null 2>&1 && WANT_DEB=1
            command -v rpmbuild >/dev/null 2>&1 && WANT_RPM=1
            # The AppImage needs no local tool — build-appimage.sh downloads
            # linuxdeploy and the runtime — but it does need Linux and a
            # network, so it is in only when this is Linux.
            [ "$(uname -s)" = "Linux" ] && WANT_APPIMAGE=1
            shift ;;
        -h|--help) zervo_usage; exit 0 ;;
        *) die "unknown option: $1 (try --help)" ;;
    esac
done

if [ "$((WANT_DEB + WANT_RPM + WANT_TARBALL + WANT_APPIMAGE + WANT_PKGBUILD))" = 0 ]; then
    die "nothing to do: pass at least one of --deb --rpm --tarball --appimage --pkgbuild --all"
fi

# --pkgbuild renders two text files from templates. It reads no binary and no
# staging tree, so on its own it needs neither — and demanding an hour of Servo
# build to produce a PKGBUILD was absurd.
NEEDS_BINARY=$((WANT_DEB + WANT_RPM + WANT_TARBALL + WANT_APPIMAGE))

# Whether this build has media in it, which decides whether the packages ask
# for the GStreamer plugins. Normalised, because cargo takes both
# `--features a,b` and `--features "a b"`.
case " $(printf '%s' "$FEATURES" | tr ',' ' ') " in
    *" media "*) WITH_MEDIA=1 ;;
    *) WITH_MEDIA=0 ;;
esac

VERSION="$(zervo_version)"
DEB_VERSION="$VERSION${VERSION_SUFFIX:++$VERSION_SUFFIX}"
# rpm splits Name-Version-Release on hyphens, so a version containing one — a
# `0.5.0-rc.1`, say — is not representable. Caught here rather than five
# minutes into rpmbuild, and only when an .rpm was actually asked for.
if [ "$WANT_RPM" = 1 ]; then
    case "$VERSION" in
        *-*) die "rpm cannot express a version containing a hyphen: $VERSION" ;;
    esac
fi

UNAME_ARCH="$(uname -m)"
# A --target triple names the architecture more reliably than the build host
# does, so prefer it when there is one.
[ -n "$TRIPLE" ] && UNAME_ARCH="${TRIPLE%%-*}"
SOURCE_DATE_EPOCH="$(zervo_source_date_epoch)"
export SOURCE_DATE_EPOCH

say "Zervo $VERSION for $UNAME_ARCH (profile: $PROFILE${FEATURES:+, features: $FEATURES})"

if [ "$NEEDS_BINARY" -gt 0 ]; then
    if [ "$BUILD" = 1 ]; then
        zervo_cargo_build "$PROFILE" "$FEATURES" "$TRIPLE"
    fi
    BUILT="$(zervo_binary_path "$PROFILE" "$TRIPLE")"
    [ -x "$BUILT" ] || die "no binary at $BUILT — build it first, or drop --no-build"
fi

# ── The install tree, shared by every format below ────────────────────────
# One temp root, cleaned up the moment it exists — including when a CI job is
# cancelled, which arrives as SIGTERM, and when someone presses Ctrl-C.
TMPROOT="$(mktemp -d)"
zervo_cleanup_on_exit "$TMPROOT"
STAGE="$TMPROOT/stage"
WORK="$TMPROOT/work"
mkdir -p "$STAGE" "$WORK"
[ "$NEEDS_BINARY" -gt 0 ] && zervo_stage_tree "$STAGE" "$BUILT"
mkdir -p "$OUTPUT"

# ── .deb ──────────────────────────────────────────────────────────────────
if [ "$WANT_DEB" = 1 ]; then
    # Also a check: refuses an architecture dpkg has no name for, rather than
    # writing it into the control file and producing something that installs
    # nowhere. Assigned rather than inlined, because `die` inside a command
    # substitution in argument position kills only the subshell.
    DEB_ARCH="$(zervo_deb_arch "$UNAME_ARCH")"
    say ".deb ($DEB_ARCH)"
    need dpkg-deb

    DEBROOT="$WORK/deb"
    mkdir -p "$DEBROOT/DEBIAN"
    cp -a "$STAGE/." "$DEBROOT/"

    # Let dpkg work the library dependencies out from the binary itself rather
    # than maintaining a hand-written list that silently rots. It needs a
    # debian/ directory to run in, hence the stub.
    DEPENDS=""
    if command -v dpkg-shlibdeps >/dev/null 2>&1; then
        SHLIBDIR="$WORK/shlibs"
        mkdir -p "$SHLIBDIR/debian"
        # dpkg-shlibdeps opens debian/control unconditionally, even with -O and
        # even for a single prebuilt binary, and dies confusingly without it.
        # Build-Depends is deliberately absent: dpkg-shlibdeps raises the libc6
        # floor to whatever the declared build dependency provides, so declaring
        # nothing gives the tightest, most portable answer.
        printf 'Source: zervo\n' > "$SHLIBDIR/debian/control"
        cp "$STAGE/usr/bin/zervo" "$SHLIBDIR/zervo"
        # `|| true`, and stderr to a file rather than /dev/null. dpkg-shlibdeps
        # exits non-zero for ordinary conditions, and under `set -e` with
        # `pipefail` that killed the whole script — silently, because the
        # warning below could never be reached and the error was discarded.
        DEPENDS="$( cd "$SHLIBDIR" \
            && { dpkg-shlibdeps -O --ignore-missing-info ./zervo 2>"$WORK/shlibdeps.err" || true; } \
            | sed -n 's/^shlibs:Depends=//p' )"
    fi
    if [ -z "$DEPENDS" ]; then
        warn "dpkg-shlibdeps produced nothing; shipping without Depends"
        if [ -s "$WORK/shlibdeps.err" ]; then
            sed 's/^/    /' "$WORK/shlibdeps.err" >&2
        fi
    fi

    # Debian rounds Installed-Size to whole kibibytes and apt shows it before
    # downloading. du -sk is what dh_gencontrol uses.
    INSTALLED_SIZE="$(du -sk "$STAGE" | cut -f1)"

    {
        echo "Package: zervo"
        echo "Version: $DEB_VERSION"
        echo "Architecture: $DEB_ARCH"
        echo "Maintainer: $ZERVO_MAINTAINER"
        echo "Homepage: $ZERVO_HOMEPAGE"
        echo "Section: web"
        echo "Priority: optional"
        echo "Installed-Size: $INSTALLED_SIZE"
        [ -n "$DEPENDS" ] && echo "Depends: $DEPENDS"
        DEB_RECOMMENDS="$ZERVO_DEB_RECOMMENDS"
        [ "$WITH_MEDIA" = 1 ] && DEB_RECOMMENDS="$DEB_RECOMMENDS, $ZERVO_DEB_MEDIA_RECOMMENDS"
        echo "Recommends: $DEB_RECOMMENDS"
        echo "Provides: www-browser"
        echo "Description: $ZERVO_SUMMARY"
        printf '%s\n' "$ZERVO_DESCRIPTION" | sed 's/^$/./; s/^/ /'
    } > "$DEBROOT/DEBIAN/control"

    # Register with the x-www-browser alternative, which is how a Debian system
    # is told a browser exists. Priority 40 puts Zervo below the established
    # graphical browsers, which is where something this young belongs.
    cat > "$DEBROOT/DEBIAN/postinst" <<'POSTINST'
#!/bin/sh
set -e
if [ "$1" = configure ]; then
    update-alternatives --install /usr/bin/x-www-browser x-www-browser /usr/bin/zervo 40
fi
POSTINST
    cat > "$DEBROOT/DEBIAN/prerm" <<'PRERM'
#!/bin/sh
set -e
if [ "$1" = remove ] || [ "$1" = deconfigure ]; then
    update-alternatives --remove x-www-browser /usr/bin/zervo
fi
PRERM
    chmod 755 "$DEBROOT/DEBIAN/postinst" "$DEBROOT/DEBIAN/prerm"

    # The desktop database, icon cache and MIME cache are refreshed by dpkg
    # triggers that desktop-file-utils, hicolor-icon-theme and shared-mime-info
    # own; a maintainer script that calls them by hand is redundant and lintian
    # says so. Hence Recommends on the first two above.

    DEB="$OUTPUT/zervo_${DEB_VERSION}_${DEB_ARCH}.deb"
    dpkg-deb --build --root-owner-group "$DEBROOT" "$DEB" > /dev/null
    note "$DEB"
fi

# ── .rpm ──────────────────────────────────────────────────────────────────
if [ "$WANT_RPM" = 1 ]; then
    # Assigned rather than inlined into the message. `die` inside a command
    # substitution in argument position kills the subshell and nothing else —
    # the error is printed, the exit status is thrown away, and the build
    # carries on to produce the uninstallable package the check exists to
    # prevent. An assignment's status is the substitution's, so `set -e` sees
    # it.
    RPM_ARCH="$(zervo_rpm_arch "$UNAME_ARCH")"
    say ".rpm ($RPM_ARCH)"
    need rpmbuild

    TOP="$WORK/rpm"
    mkdir -p "$TOP"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
    mkdir -p "$TOP/SOURCES/tree"
    cp -a "$STAGE/." "$TOP/SOURCES/tree/"

    # rpmbuild checks the weekday against the date and fails the build when they
    # disagree, so it is derived rather than typed. The date is the commit's,
    # not today's, so rebuilding a tag gives the same spec.
    CHANGELOG_DATE="$(zervo_rpm_changelog_date "$SOURCE_DATE_EPOCH")"

    RPM_RECOMMENDS="$ZERVO_RPM_RECOMMENDS"
    [ "$WITH_MEDIA" = 1 ] && RPM_RECOMMENDS="$RPM_RECOMMENDS $ZERVO_RPM_MEDIA_RECOMMENDS"
    RPM_RECOMMENDS_LINES=""
    for pkg in $RPM_RECOMMENDS; do
        RPM_RECOMMENDS_LINES="${RPM_RECOMMENDS_LINES}Recommends:     $pkg
"
    done

    cat > "$TOP/SPECS/zervo.spec" <<SPEC
Name:           zervo
Version:        $VERSION
Release:        1%{?dist}
Summary:        $ZERVO_SUMMARY
License:        $ZERVO_LICENSE
URL:            $ZERVO_HOMEPAGE

# Built ahead of time by cargo; rpmbuild only assembles the package. It still
# works the library requirements out from the binary on its own.
#
# ExclusiveArch rather than BuildArch: BuildArch pins the package's
# architecture, which is what you want for a noarch package and wrong for one
# containing an ELF binary. rpmbuild derives the arch itself; this only says
# which arches are worth attempting.
ExclusiveArch:  x86_64 aarch64

# No -debuginfo subpackage: the binary comes from cargo, not from a %build
# section, so there are no sources to point the debug symbols at. Note the side
# effect — with __debug_package undefined, redhat-rpm-config runs brp-strip, so
# the shipped binary is stripped.
%global debug_package %{nil}

# Owns the icon directories and carries the file trigger that rebuilds the icon
# cache; desktop-file-utils and shared-mime-info carry the equivalents for the
# desktop and MIME databases. That is why there are no %%post scriptlets here.
Requires:       hicolor-icon-theme
$RPM_RECOMMENDS_LINES
%description
$ZERVO_DESCRIPTION

%install
cp -a %{_sourcedir}/tree/. %{buildroot}/

%check
# Both are validated on every pull request by .github/workflows/check.yml,
# where a failure costs seconds. Here they are a second opinion, not a gate —
# an appstream-util a version out from the one that passed should not fail a
# package that is otherwise correct. There is deliberately no BuildRequires
# for either: rpmbuild enforces those, and a validator that is only ever
# advisory must not be able to stop the build.
desktop-file-validate %{buildroot}%{_datadir}/applications/app.zervo.Zervo.desktop || :
appstream-util validate-relax --nonet \\
    %{buildroot}%{_metainfodir}/app.zervo.Zervo.metainfo.xml || :

%files
%{_bindir}/zervo
%{_datadir}/applications/app.zervo.Zervo.desktop
%{_datadir}/icons/hicolor/scalable/apps/zervo.svg
%{_datadir}/icons/hicolor/128x128/apps/zervo.png
%{_datadir}/icons/hicolor/256x256/apps/zervo.png
%{_datadir}/icons/hicolor/512x512/apps/zervo.png
%{_metainfodir}/app.zervo.Zervo.metainfo.xml
%{_mandir}/man1/zervo.1*
%dir %{_datadir}/doc/zervo
%doc %{_datadir}/doc/zervo/README.md
%doc %{_datadir}/doc/zervo/CHANGELOG.md
%doc %{_datadir}/doc/zervo/changelog.gz
# Debian's name for the licence, shipped for the .deb out of the same staging
# tree. rpmbuild fails the build on any file it was handed and not told about,
# so it is listed here even though %license below is the one Fedora reads.
%license %{_datadir}/doc/zervo/copyright
%license %{_datadir}/licenses/zervo/LICENSE

%changelog
* $CHANGELOG_DATE $ZERVO_MAINTAINER - $VERSION-1
- See $ZERVO_HOMEPAGE/blob/main/CHANGELOG.md
SPEC

    if ! rpmbuild --define "_topdir $TOP" -bb "$TOP/SPECS/zervo.spec" > "$WORK/rpmbuild.log" 2>&1; then
        printf 'rpmbuild failed:\n' >&2
        tail -40 "$WORK/rpmbuild.log" >&2
        exit 1
    fi
    # rpmbuild writes into RPMS/<arch>/. Counted rather than assumed: rpmbuild
    # can exit zero having produced nothing at all, and the old code then
    # printed a filename with a literal asterisk in it and carried on.
    RPM_COUNT=0
    for built in "$TOP"/RPMS/*/*.rpm; do
        [ -f "$built" ] || continue
        cp "$built" "$OUTPUT/"
        note "$OUTPUT/$(basename -- "$built")"
        RPM_COUNT=$((RPM_COUNT + 1))
    done
    [ "$RPM_COUNT" -gt 0 ] || {
        echo "rpmbuild reported success but produced no package:" >&2
        tail -40 "$WORK/rpmbuild.log" >&2
        exit 1
    }
fi

# ── portable tarball ──────────────────────────────────────────────────────
# The thing the AUR -bin package, and anyone on a distribution nobody has
# packaged for, actually downloads. Laid out as a relocatable prefix so it can
# be unpacked anywhere, with the same tree the .deb installs.
if [ "$WANT_TARBALL" = 1 ]; then
    say "tarball"
    TARNAME="zervo-${VERSION}-${UNAME_ARCH}-linux-gnu"
    TARDIR="$WORK/$TARNAME"
    mkdir -p "$TARDIR"
    cp -a "$STAGE/usr/." "$TARDIR/"

    # GNU tar only: everything here is reproducible given SOURCE_DATE_EPOCH.
    tar --sort=name \
        --mtime="@$SOURCE_DATE_EPOCH" \
        --owner=0 --group=0 --numeric-owner \
        --format=gnu \
        -C "$WORK" -czf "$OUTPUT/$TARNAME.tar.gz" "$TARNAME"
    note "$OUTPUT/$TARNAME.tar.gz"
fi

# ── AppImage ──────────────────────────────────────────────────────────────
if [ "$WANT_APPIMAGE" = 1 ]; then
    APPIMAGE_ARGS=(--profile "$PROFILE" --output "$OUTPUT" --no-build)
    [ -n "$FEATURES" ] && APPIMAGE_ARGS+=(--features "$FEATURES")
    [ -n "$TRIPLE" ] && APPIMAGE_ARGS+=(--target "$TRIPLE")
    ./scripts/build-appimage.sh "${APPIMAGE_ARGS[@]}"
fi

# ── PKGBUILD ──────────────────────────────────────────────────────────────
if [ "$WANT_PKGBUILD" = 1 ]; then
    ./scripts/make-aur.sh --output "$OUTPUT/aur" --artifacts "$OUTPUT"
fi

say "in $OUTPUT"
ls -la "$OUTPUT"
