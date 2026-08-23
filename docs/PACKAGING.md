# Packaging and distribution

Everything a release produces comes out of `scripts/`, and CI does nothing a
person cannot do by hand.

```bash
# macOS — an .app and a disk image for this machine's architecture
./scripts/bundle-macos.sh --dmg --features engine-downloads,media

# Linux — every format the machine has the tools for
./scripts/package-linux.sh --all --features engine-downloads,media

# Windows
pwsh scripts/package-windows.ps1 -Features engine-downloads,media -Installer
```

Each script takes `--help`.

## What a release contains

| File | Built on | Runs on |
| --- | --- | --- |
| `Zervo-<v>-arm64.dmg` | macOS 15, Apple Silicon | macOS 11 and later, Apple Silicon |
| `Zervo-<v>-x86_64.dmg` | macOS 15, Intel | macOS 11 and later, Intel |
| `zervo_<v>_amd64.deb` | Ubuntu 26.04 | Ubuntu 26.04 and later |
| `zervo-<v>-1.fc44.x86_64.rpm` | Fedora 44 | Fedora 44 |
| `Zervo-<v>-x86_64.AppImage` (+ `.zsync`) | Ubuntu 22.04 | any glibc 2.35 or newer |
| `zervo-<v>-x86_64-linux-gnu.tar.gz` | Ubuntu 26.04 | anything the `.deb` runs on, unpacked anywhere |
| `Zervo-<v>-windows-x64.zip`, `-setup.exe` | Windows Server 2025 | Windows 10 1809 and later, x64 and ARM64 under Prism |
| `zervo-aur-packages.tar.gz` | — | `PKGBUILD` + `.SRCINFO` for the AUR |
| `SHA256SUMS` | — | every file above |

Everything except macOS is x86_64. There is no technical obstacle to arm64 on
Linux — the runners are generally available and free — but nobody is asking for
it, and each one is another hour of engine build.

**The `.deb` requires Ubuntu 26.04 or newer, and that is the trade it makes.**
glibc runs old binaries on new systems and never the reverse, so a package
built on 26.04 acquires a `libc6 (>= 2.43)` dependency that 24.04 cannot
satisfy. Building on the older LTS instead would cover both — it is the usual
advice — but only one `.deb` is built, and it is built on the release Ubuntu
currently ships. Anyone older is covered by the AppImage, which is built
against glibc 2.35 precisely so that it reaches back further than any distro
package can.

Two things about that runner are worth knowing. `ubuntu-26.04` is **public
preview**: GitHub excludes it from its service level agreement and warns about
capacity, and it is the one runner a release artefact now depends on. And its
toolchain is aggressive — CMake 4, GCC 15, clang 21 — which has already broken
this build once (see below).

---

## macOS

### Two disk images, one per architecture

CI builds each natively — `macos-15` for Apple Silicon, `macos-15-intel` for
Intel — and ships both.

```bash
./scripts/bundle-macos.sh --dmg                # this machine's architecture
./scripts/bundle-macos.sh --dmg --target x86_64-apple-darwin
./scripts/bundle-macos.sh --universal --dmg    # both, fused with lipo
```

`--universal` still works and is cheap: `mozjs` downloads a prebuilt
SpiderMonkey per target rather than compiling it, so the second
`cargo build --target` is minutes rather than hours, and every dylib in the
GStreamer framework Servo pins is *already* fat — which leaves exactly one file
for `lipo` to merge. It is not what a release ships, because two named
downloads beat one that runs on both, and because each of these is about 80 MB
smaller: a single-architecture bundle has its GStreamer libraries thinned to
match, and a universal one cannot be.

Cross-compiling with `--features media` needs one extra thing, and the script
sets it: `glib-sys` and `gstreamer-sys` refuse to run `pkg-config` for a target
that is not the host. `PKG_CONFIG_ALLOW_CROSS=1` is normally a loaded gun,
because one set of `.pc` files usually describes one architecture; here it does
not, because the framework is universal.

### The minimum macOS version

`.cargo/config.toml` sets `MACOSX_DEPLOYMENT_TARGET = "11.0"`, with
`force = true`, and `LSMinimumSystemVersion` in the bundle is derived from it.

Both halves of a universal binary have to claim the same floor — `lipo` will
fuse two that disagree without a word — and 11.0 is the honest number rather
than a chosen one: it is what the arm64 binary already recorded before anyone
set the variable, and it is the floor of the arm64 slice of the GStreamer
framework. The plist used to say 13.0, typed in beside the compiler rather than
derived from it, which turned away macOS 11 and 12 at launch from a binary that
would have run.

### Unsigned builds — what your users will see

Zervo releases are **not signed with an Apple Developer ID and not notarized**.
A signing certificate costs $99/year, and notarization requires uploading each
build to Apple.

The bundler applies an *ad-hoc* signature (`codesign -s -`), signing the nested
libraries first and the bundle last. That is not optional: since Apple Silicon,
all executable code has to carry a signature, and a broken one is worse than
none. It is also not a Developer ID signature, and does not satisfy Gatekeeper.

`--deep` is not used. Apple deprecated it in macOS 13 — it applies one set of
options to everything it reaches, which is almost never right, and it only
reaches a fixed list of subdirectories.

Gatekeeper's verdict turns entirely on one thing: whether the app carries
`com.apple.quarantine`. Apple's own `gktool scan Zervo.app` says *"Scan
completed and software is allowed by system policy"* for an unquarantined build
of Zervo, and *"failed because the software is not signed by a distributor that
meets the system Gatekeeper requirements"* once the flag is on. The bundle is
fine either way; the flag is the whole story.

Do not use `spctl -a -t exec` to check this. It reports `rejected` for an
ad-hoc signed app whether or not it is quarantined, so it tells you nothing
about what a user will actually see.

The flag does not live on the app inside the disk image. The download stamps it
on the `.dmg`, the mounted volume then carries a `quarantine` mount flag, and
the attribute is written onto the copy when the app is dragged out. So the two
places worth clearing it are the `.dmg` before mounting, or the installed app
afterwards:

```bash
xattr -dr com.apple.quarantine /Applications/Zervo.app
```

Without that, macOS says *"Zervo is damaged and can't be opened"*, which reads
like a corrupt download and is not what has happened.

**That command is now the only way.** Do not tell users to Control-click →
Open: Apple removed that bypass for improperly signed apps in macOS Sequoia
([Updates to runtime protection in macOS Sequoia][sequoia]). And on macOS 26 and
later, a quarantined ad-hoc-signed app does not get an **Open Anyway** button in
System Settings either — it is offered to the Trash instead. The instruction in
the disk image's *READ ME FIRST* says so.

[sequoia]: https://developer.apple.com/news/?id=saqachfa

Building from source has no such problem: a locally built app is never
quarantined.

### Signing and notarizing, if you ever want to

The script already does it; it needs an identity.

```bash
./scripts/bundle-macos.sh --universal --dmg \
    --sign "Developer ID Application: NAME (TEAMID)" --notarize
```

`--sign` switches on the hardened runtime and a secure timestamp, both of which
notarization requires. Two things about that are easy to get wrong and give no
sign that they went wrong.

The argument order: `codesign --sign … --options runtime`, never the reverse.
`--options` before `--sign` is *silently ignored*, and the signature comes out
without the hardened runtime — which notarization then rejects with an error
that says nothing about argument order.

And the entitlements. The hardened runtime forbids writing to memory and then
executing it, which is precisely what a JavaScript JIT does, so a Developer ID
build without `com.apple.security.cs.allow-jit` crashes on the first page that
runs a script — which is every page. `assets/macos/entitlements.plist` carries
that and `allow-unsigned-executable-memory`, and the bundler passes it when
signing the bundle. The nested libraries do not get it: they are libraries and
they JIT nothing.

`--notarize` wants `APPLE_ID`, `APPLE_TEAM_ID` and `APPLE_APP_PASSWORD` in the
environment, submits the disk image with `notarytool` and staples the ticket, so
the result opens with no quarantine step at all.

CI picks this up on its own: `.github/workflows/macos.yml` imports a certificate
into a temporary keychain when the repository has these secrets, and falls back
to an ad-hoc signature when it does not.

| Secret | What it is |
| --- | --- |
| `MACOS_CERTIFICATE` | the `.p12`, base64-encoded |
| `MACOS_CERTIFICATE_PASSWORD` | its export password |
| `MACOS_SIGN_IDENTITY` | `Developer ID Application: NAME (TEAMID)` |
| `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` | notarization, optional |

### If a build will not start: `@rpath/libz.1.dylib`

```
dyld: Library not loaded: @rpath/libz.1.dylib … no LC_RPATH's found
```

This happens only on machines with the GStreamer framework installed, and only
to `cargo build --release` — the bundler handles it. GStreamer's `-sys` build
scripts put the framework's lib directory on the link path whether or not
`--features media` asked for it, so `-lz` finds the zlib in there before the
system one. That copy's install name is `@rpath/libz.1.dylib`, and nothing adds
an rpath.

`./scripts/bundle-macos.sh` repoints it at `/usr/lib/libz.1.dylib` before
signing. To fix a binary you built yourself:

```bash
install_name_tool -change @rpath/libz.1.dylib /usr/lib/libz.1.dylib target/release/zervo
```

The obvious fix — a linker flag in `.cargo/config.toml` — is not used, for two
reasons: setting `rustflags` there is silently dropped when RUSTFLAGS is set in
the environment, which is what CI does, and changing it invalidates every
crate's fingerprint, so it costs a full rebuild of Servo.

### Audio and video

`--features media` needs the official GStreamer distribution installed at
`/Library/Frameworks/GStreamer.framework`. Servo supports no other source for it
on macOS, and pins 1.22.3. Both packages are needed — the runtime and the
headers:

```bash
BASE=https://github.com/servo/servo-build-deps/releases/download/macOS
curl -L -o /tmp/gstreamer.pkg       $BASE/gstreamer-1.0-1.22.3-universal.pkg
curl -L -o /tmp/gstreamer-devel.pkg $BASE/gstreamer-1.0-devel-1.22.3-universal.pkg
sudo installer -pkg /tmp/gstreamer.pkg -target /
sudo installer -pkg /tmp/gstreamer-devel.pkg -target /
```

`scripts/bundle-gstreamer.py` then copies the libraries and plugins into
`Zervo.app/Contents/MacOS/lib` and rewrites their install names, so the bundle
runs on machines that have never heard of GStreamer. `bundle-macos.sh` calls it
for you. The plugins are loaded by name at runtime and never show up in `otool`
output, so they are listed explicitly in that script; if playback breaks after a
Servo update, check the list against `gstreamer_plugin_lists/` in the servo
crate.

Expect the bundle to grow by roughly 100 MB — 150 MB before thinning.

That pin is old. Servo has not moved off 1.22.3, released in June 2023, and the
release it downloads from has not been touched since August 2023, while upstream
GStreamer is on 1.28. Zervo cannot get ahead of the engine here.

---

## Linux

```bash
./scripts/package-linux.sh --deb        # on Ubuntu or Debian
./scripts/package-linux.sh --rpm        # on Fedora
./scripts/package-linux.sh --tarball    # anywhere
./scripts/package-linux.sh --appimage   # on the oldest glibc you can find
./scripts/package-linux.sh --pkgbuild   # PKGBUILDs for the AUR
./scripts/package-linux.sh --all        # every format this machine can produce
```

`--pkgbuild` on its own renders two text files and builds nothing — no engine,
no staging tree. Every other format needs the binary.

One staging tree feeds all of them, so a file added for one format appears in
every format. It holds the binary, the desktop entry, a scalable and a 256×256
icon, the AppStream metainfo, a gzipped man page, the licence and the readme.

Build each package on the distribution it targets. A binary linked against
Ubuntu's libraries will not reliably run on Fedora, and both `dpkg-shlibdeps`
and `rpmbuild` derive the package's requirements from the binary itself, which
means those requirements have to be that distribution's sonames.

Neither package carries a hand-written library list. What they *do* carry, and
did not before, is the list of programs Zervo shells out to — `zenity` for the
file chooser, `xdg-utils` for opening a download, `libsecret` for the keyring.
None of those are linked, so no dependency generator finds them, and without
them the feature silently does nothing.

Neither carries a `%post` scriptlet either: the desktop database, the icon cache
and the MIME cache are all refreshed by triggers that `desktop-file-utils`,
`hicolor-icon-theme` and `shared-mime-info` own, on both Debian and Fedora. The
`.deb` does register `/usr/bin/x-www-browser` through `update-alternatives`,
which is the only thing a maintainer script is genuinely needed for.

GStreamer is an ordinary system package on Linux, so `--features media` needs no
bundling — unlike macOS, where the framework has to be copied into the app.

Nothing here is signed, which on Linux is not the obstacle it is on macOS: both
`dpkg -i` and `dnf install` will install an unsigned local package, with at most
a warning.

### The AppImage

```bash
./scripts/build-appimage.sh --features engine-downloads,media
```

Built inside an `ubuntu:22.04` container, not on the runner. glibc is backwards
compatible and not forwards, so the oldest base still worth supporting reaches
furthest — and a container outlives the `ubuntu-22.04` runner label, whose
deprecation starts in September 2026.

Two things are deliberately **not** bundled.

**The graphics and display stack.** `libGL`, `libEGL`, `libGLdispatch`,
`libglapi`, `libgbm`, `libdrm`, `libX11`, `libxcb` and `libwayland-client` are
coupled to the host's video driver and its compositor. AppImage's own excludelist
says so, and the failure is not subtle: a bundled `libwayland-client` against a
newer host Mesa makes `eglGetDisplay` return `EGL_BAD_PARAMETER`, and the
browser never draws a frame. `linuxdeploy` applies that excludelist; the script
then checks that it did, and refuses to produce a file if anything slipped
through.

**GStreamer.** Servo refuses to bootstrap GStreamer on Linux at all, the
`linuxdeploy` plugin for it describes itself as experimental, and bundling it
drags in glib, gio, gobject and `libwayland-client` — precisely the set above.
Audio and video therefore come from the host's own GStreamer, and
`GST_PLUGIN_SYSTEM_PATH_1_0` is left alone: pointing it at a directory that does
not exist disables the host's plugin search and corrupts the user's GStreamer
registry for *every other application on the machine*.

So the AppImage needs `gstreamer1.0-plugins-{base,good,bad,ugly}` and
`gstreamer1.0-libav` from the distribution for media to play. Everything else
works without them.

Each AppImage ships with a `.zsync` file beside it, so `AppImageUpdate` and
`appimaged` can fetch a delta rather than the whole thing.

If an AppImage will not start with *"Cannot mount AppImage, please check your
FUSE setup"*, run it as `./Zervo-x.y.z-x86_64.AppImage --appimage-extract-and-run`.
The runtime is statically linked against libfuse3, so there is no `libfuse2` to
install any more, but it still needs `/dev/fuse` and a setuid `fusermount`.

### Arch and the AUR

`./scripts/package-linux.sh --pkgbuild` renders two packages from the templates
in `scripts/aur/`:

- **`zervo-bin`** repackages the release tarball. This is the one to install.
- **`zervo`** builds from source, which means compiling the Servo engine.

Both are rendered with the checksums of the artefacts that were actually built,
and CI attaches them to the release as `zervo-aur-packages.tar.gz`, complete
with the `.SRCINFO` the AUR serves.

Publishing is not automated and will not be. The AUR is a git remote over SSH
tied to one person's account:

```bash
git -c init.defaultBranch=master clone ssh://aur@aur.archlinux.org/zervo-bin.git
cp zervo-bin/PKGBUILD zervo-bin/.SRCINFO aur-checkout/
cd aur-checkout && git commit -am "zervo-bin 0.4.1" && git push
```

Three things the AUR's server-side hook will reject a push for, all of which the
CI job checks in seconds: `.SRCINFO` missing or out of date with `PKGBUILD`, a
push to any branch other than `master`, and any subdirectory in the repository.

Servo's own `mach bootstrap` is a silent no-op on Arch — it accepts Arch as a
known distribution and then installs nothing — so the `makedepends` array in
`scripts/aur/PKGBUILD.in` is the only Arch dependency list that exists anywhere.

---

## Windows

```powershell
pwsh scripts/package-windows.ps1 -Features engine-downloads,media -Installer
```

Produces a directory, a `.zip` of it, and — with `-Installer` — an NSIS setup
executable.

Three things travel beside the exe because they have to:

**ANGLE.** Windows composites through EGL on Direct3D 11 rather than WGL, and
`mozangle` builds `libEGL.dll` and `libGLESv2.dll` into its own `OUT_DIR`, not
into the profile directory. The exe is linked against `libEGL`, so without them
it does not start at all. The script picks the *newest* copy under
`target/*/build`, because a target directory that has built more than one
`mozangle` contains several and a stale one is a crash at launch.

**The Visual C++ runtime.** Rust's MSVC targets link the CRT dynamically, so a
machine without the redistributable gets *"vcruntime140.dll was not found"* and
nothing else.

**GStreamer**, when `-GStreamerRoot` is given. Everything in the installation's
`bin/` and everything in `lib/gstreamer-1.0/` is copied *flat* beside the exe —
flat because the engine looks for plugins next to the binary, and a tidier
layout means media silently stops working with no error to point at.

### The installer

Per-user, into `%LOCALAPPDATA%\Programs\Zervo`, registered under `HKCU`, with
`RequestExecutionLevel user`. That is deliberate: the build is unsigned, and an
unsigned installer asking for administrator rights is the combination Windows
objects to most loudly. Nothing in Zervo needs to live outside the user's
profile.

It registers Zervo's *capability* to handle `http` and `https`, which is what
puts it in Settings → Default apps. It does not make itself the default; an
installer may not, and should not.

Uninstalling removes the program and its registry entries and leaves
`%APPDATA%` alone. An uninstall is not a request to throw away bookmarks.

### SmartScreen

An unsigned binary shows *"Windows protected your PC"* on first run, and on a
machine with Smart App Control enabled it does not run at all.

Signing does not remove that prompt — reputation still has to accumulate — but
it is the only thing that lets reputation accumulate *across versions* rather
than resetting to zero on every release. Do not buy an EV certificate for this:
Microsoft's own documentation now says EV no longer bypasses SmartScreen. The
cheapest real option is Azure Artifact Signing (formerly Trusted Signing) at
about $10 a month, with the caveat that individual enrolment is limited to the
United States and Canada and identity validation takes days.

### ARM64

Not built, and that is a decision rather than an omission.

`aarch64-pc-windows-msvc` is Rust tier 1, `mozjs` already publishes a prebuilt
SpiderMonkey for it, and `mozangle` guards its SSE2 flags by architecture. What
blocks it is a header collision: MSVC's `<arm_fp16.h>` typedefs `float16_t`, and
the vendored `glsl-optimizer` inside `glslopt` declares a struct of the same
name. Both the fix ([glslopt-rs#11][glslopt]) and the Servo PR that carries it
([servo#42312][servoarm]) are open and unmerged, and there is no official
GStreamer build for Windows on ARM at all.

Meanwhile Windows 11 on ARM emulates x64 user-mode code through Prism, and does
not emulate the GPU path — Direct3D and ANGLE call into native arm64 system
libraries. So the x64 build runs on those machines; it is slower at CPU-bound
work and correct at everything else.

The job is wired and switched off. `workflow_dispatch` with `try-arm64` turns it
on, and it will either work or say exactly why not.

[glslopt]: https://github.com/jamienicol/glslopt-rs/pull/11
[servoarm]: https://github.com/servo/servo/pull/42312

---

## The release pipeline

`.github/workflows/release.yml` owns a release, and it is the only workflow with
a token that can write one.

```
draft ──┬─ linux.yml   (deb, rpm, AppImage, PKGBUILD)
        ├─ macos.yml   (.dmg ×2: arm64, x86_64)
        ├─ windows.yml (zip + installer)
        └─ source      (git archive)
                │
              aur ─────┐
                       ├─ publish: SHA256SUMS, provenance, upload, publish
```

**A failing job inside a called workflow fails the caller's job**, whatever
`continue-on-error` says about the matrix leg — and `publish` is guarded on no
job having failed. So there is no such thing as a non-blocking leg inside
`linux.yml`: anything in there that can fail can hold up a release. The only
shape that reliably reports success for a job that did nothing is a gate step
that every other step is `if:`-conditional on, which is what the Windows arm64
leg used before it was removed outright.

### When clang moves under you

Ubuntu 26.04 carries several LLVM versions. `mozangle` runs bindgen over
ANGLE's shader translator, and bindgen loads `libclang` at runtime while
reading the headers that `clang` resolves to — but clang-sys picks the *newest*
libclang it can find. So bindgen parsed LLVM 21's headers with a newer
libclang, and every SSE builtin those headers name had been removed in the
meantime:

```
/usr/lib/llvm-21/lib/clang/21/include/xmmintrin.h:245:18:
    error: use of undeclared identifier '__builtin_ia32_sqrtss'
```

Twenty of those, then `fatal error: too many errors emitted`, then a panic in a
build script an hour into the job. The fix is to make the two the same version
by construction rather than by luck:

```bash
RESOURCE_DIR="$(clang -print-resource-dir)"     # /usr/lib/llvm-21/lib/clang/21
echo "LIBCLANG_PATH=${RESOURCE_DIR%/clang/*}"   # /usr/lib/llvm-21/lib
```

The `.deb` and AppImage jobs both do this; the Fedora one pins `/usr/lib64`
for the same reason, because `clang-libs` there ships only a versioned
`libclang.so.NN` and the unversioned symlink bindgen prefers lives in
`clang-devel`.

Draft first, attach everything to the draft, publish last. The three platform
workflows used to attach their own artefacts to the tag as they finished, which
is a create-or-update race between jobs, cannot produce a `SHA256SUMS` covering
more than one platform, and breaks outright the day immutable releases are
switched on for the repository. If any platform fails, the draft stays a draft
and nothing half-finished is ever visible.

The long build jobs hold no write token and check out with
`persist-credentials: false`, so the release token is never in reach of a build
script from any of the thousand-odd crates in the dependency graph.

`workflow_dispatch` on `release.yml` takes an existing tag and leaves the result
as a draft unless you ask otherwise, which is how to rehearse one.

### Cutting a release

1. Bump `version` in `Cargo.toml` and `zervo-core/Cargo.toml`, add a
   `CHANGELOG.md` entry and a `<release>` to
   `assets/linux/app.zervo.Zervo.metainfo.xml`.
2. Commit, tag `vX.Y.Z`, push the tag.
3. Watch. A cold build is hours; a warm one considerably less.
4. Check the draft, then let it publish — or publish it yourself if you ran the
   workflow by hand.

### What CI checks before any of that

`check.yml` runs on every pull request and compiles nothing heavier than
`zervo-core`. It also lints the half of the repository that cannot be run on a
developer's machine at all: `shellcheck -x` over every script including the
shared library, `actionlint` over every workflow, PSScriptAnalyzer over the
PowerShell, `desktop-file-validate` and `appstreamcli` over the freedesktop
metadata, `mandoc -Tlint` over the man page, and a render-and-parse of both
PKGBUILDs.

None of that catches a broken package. All of it catches the typo that would
otherwise surface three hours into a release build.

---

## What is missing

- **File uploads** shell out to `zenity`. The packages recommend it; the proper
  fix is the XDG desktop portal over D-Bus.
- **No vibrancy on Linux or Windows.** The translucent chrome is an AppKit
  feature; the other platforms draw the same chrome without the frosted backdrop
  behind it.
- **Wayland versus X11** has had no testing at all.
- **The Windows exe has no icon of its own.** The installer's shortcuts do;
  Explorer shows the generic one, because embedding it needs a build script and
  a resource compiler.
- **Nothing is signed**, on any platform.
