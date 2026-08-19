#!/usr/bin/env bash
# Package Zervo as a .deb and/or .rpm.
#
#   ./scripts/package-linux.sh [--profile <name>] [--features <list>] [--deb] [--rpm]
#
# Build each package on the distribution it targets. A binary linked against
# Ubuntu's libraries will not reliably run on Fedora, so the CI workflow builds
# the .deb on Ubuntu and the .rpm in a Fedora container rather than producing
# both from one build.
set -euo pipefail

cd "$(dirname "$0")/.."

PROFILE="release"
FEATURES=""
MAKE_DEB=0
MAKE_RPM=0
while [ $# -gt 0 ]; do
    case "$1" in
        --profile) PROFILE="$2"; shift 2 ;;
        --features) FEATURES="$2"; shift 2 ;;
        --deb) MAKE_DEB=1; shift ;;
        --rpm) MAKE_RPM=1; shift ;;
        *) echo "unknown option: $1" >&2; exit 1 ;;
    esac
done
[ "$MAKE_DEB" = "0" ] && [ "$MAKE_RPM" = "0" ] && { echo "nothing to do: pass --deb and/or --rpm" >&2; exit 1; }

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
ARCH_RAW="$(uname -m)"
case "$ARCH_RAW" in
    x86_64) DEB_ARCH=amd64 ;;
    aarch64) DEB_ARCH=arm64 ;;
    *) DEB_ARCH="$ARCH_RAW" ;;
esac

echo "==> building (profile: $PROFILE${FEATURES:+, features: $FEATURES})"
if [ -n "$FEATURES" ]; then
    cargo build --profile "$PROFILE" --features "$FEATURES"
else
    cargo build --profile "$PROFILE"
fi

BUILT="target/$PROFILE/zervo"
[ "$PROFILE" = "dev" ] && BUILT="target/debug/zervo"
[ -x "$BUILT" ] || { echo "no binary at $BUILT" >&2; exit 1; }

# ── The install tree, shared by both package formats.
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
install -Dm755 "$BUILT" "$STAGE/usr/bin/zervo"
install -Dm644 assets/linux/zervo.desktop "$STAGE/usr/share/applications/zervo.desktop"
# Scalable, so there is nothing to resize and no image tooling to depend on.
install -Dm644 assets/icon/zervo-icon-default.svg \
    "$STAGE/usr/share/icons/hicolor/scalable/apps/zervo.svg"
install -Dm644 LICENSE "$STAGE/usr/share/licenses/zervo/LICENSE"
install -Dm644 README.md "$STAGE/usr/share/doc/zervo/README.md"

SUMMARY="A sidebar-first browser built on the Servo engine"
DESCRIPTION="Zervo is a browser chrome — sidebar, workspaces, tabs, settings —
wrapped around Servo, the independent web engine written in Rust. There is no
Chromium and no Gecko underneath.

Servo is not yet a complete web engine, so some sites will not work. This is an
experiment rather than a browser to depend on."

mkdir -p target/linux

if [ "$MAKE_DEB" = "1" ]; then
    echo "==> .deb"
    command -v dpkg-deb >/dev/null || { echo "dpkg-deb not found (apt install dpkg-dev)" >&2; exit 1; }

    DEBROOT="$(mktemp -d)"
    cp -a "$STAGE/." "$DEBROOT/"
    mkdir -p "$DEBROOT/DEBIAN"

    # Let dpkg work the library dependencies out from the binary itself rather
    # than maintaining a hand-written list that silently rots. It needs a
    # debian/ directory to run in, hence the stub.
    DEPENDS=""
    if command -v dpkg-shlibdeps >/dev/null; then
        SHLIBDIR="$(mktemp -d)"
        mkdir -p "$SHLIBDIR/debian"
        printf 'Source: zervo\n' > "$SHLIBDIR/debian/control"
        cp "$STAGE/usr/bin/zervo" "$SHLIBDIR/zervo"
        DEPENDS="$(cd "$SHLIBDIR" && dpkg-shlibdeps -O --ignore-missing-info ./zervo 2>/dev/null \
            | sed -n 's/^shlibs:Depends=//p')"
        rm -rf "$SHLIBDIR"
    fi
    [ -n "$DEPENDS" ] || echo "    (dpkg-shlibdeps produced nothing; shipping without Depends)"

    {
        echo "Package: zervo"
        echo "Version: $VERSION"
        echo "Architecture: $DEB_ARCH"
        echo "Maintainer: Goddv <habitat.cristal@gmail.com>"
        echo "Homepage: https://github.com/Goddv/Zervo"
        echo "Section: web"
        echo "Priority: optional"
        [ -n "$DEPENDS" ] && echo "Depends: $DEPENDS"
        echo "Description: $SUMMARY"
        echo "$DESCRIPTION" | sed 's/^$/./; s/^/ /'
    } > "$DEBROOT/DEBIAN/control"

    DEB="target/linux/zervo_${VERSION}_${DEB_ARCH}.deb"
    dpkg-deb --build --root-owner-group "$DEBROOT" "$DEB" > /dev/null
    rm -rf "$DEBROOT"
    echo "==> $DEB"
fi

if [ "$MAKE_RPM" = "1" ]; then
    echo "==> .rpm"
    command -v rpmbuild >/dev/null || { echo "rpmbuild not found (dnf install rpm-build)" >&2; exit 1; }

    TOP="$(mktemp -d)"
    mkdir -p "$TOP"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
    cat > "$TOP/SPECS/zervo.spec" <<SPEC
Name:           zervo
Version:        $VERSION
Release:        1%{?dist}
Summary:        $SUMMARY
License:        MPL-2.0
URL:            https://github.com/Goddv/Zervo
# Built ahead of time by cargo; rpmbuild only assembles the package. It still
# works the library requirements out from the binary on its own.
BuildArch:      $ARCH_RAW
%global debug_package %{nil}

%description
$DESCRIPTION

%install
cp -a %{_sourcedir}/tree/. %{buildroot}/

%files
%{_bindir}/zervo
%{_datadir}/applications/zervo.desktop
%{_datadir}/icons/hicolor/scalable/apps/zervo.svg
%{_datadir}/doc/zervo/README.md
%license %{_datadir}/licenses/zervo/LICENSE

%changelog
* Wed Aug 19 2026 Goddv <habitat.cristal@gmail.com> - $VERSION-1
- See https://github.com/Goddv/Zervo/blob/main/CHANGELOG.md
SPEC

    mkdir -p "$TOP/SOURCES/tree"
    cp -a "$STAGE/." "$TOP/SOURCES/tree/"
    rpmbuild --define "_topdir $TOP" -bb "$TOP/SPECS/zervo.spec" > "$TOP/build.log" 2>&1 || {
        echo "rpmbuild failed:" >&2; tail -30 "$TOP/build.log" >&2; exit 1; }
    find "$TOP/RPMS" -name '*.rpm' -exec cp {} target/linux/ \;
    rm -rf "$TOP"
    echo "==> $(ls target/linux/*.rpm | tail -1)"
fi

ls -la target/linux/
