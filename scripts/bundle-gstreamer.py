#!/usr/bin/env python3
"""Copy the GStreamer libraries Zervo needs into a built .app.

A bundle that only runs on machines with GStreamer already installed is no use,
so every non-system dylib the binary needs — plus the plugins, which are loaded
by name at runtime and so never appear in `otool` output — is copied next to the
binary and every reference to it rewritten to point there.

The destination is `Contents/MacOS/lib`, which is not where a macOS bundle would
normally put libraries, but Servo looks for its plugins in `<executable>/lib` and
that path is compiled in.

Ported from Servo's own packaging (python/servo/gstreamer.py, MPL-2.0), which is
where the plugin lists and the rpath resolution rules come from.

    ./scripts/bundle-gstreamer.py target/Zervo.app
"""

import os
import shutil
import subprocess
import sys

GSTREAMER_ROOT = "/Library/Frameworks/GStreamer.framework/Versions/1.0"

# Taken from the `gstreamer_plugin_lists` directory of the servo 0.5.0 crate
# (common.rs.in and macos.rs.in). Kept here rather than read out of the crate
# source, which lives at a registry path that moves with every version. If
# playback breaks after a Servo update, check these against the crate again.
PLUGINS = [
    # common
    "gstcoreelements", "gstnice", "gstapp", "gstaudioconvert", "gstaudioresample",
    "gstgio", "gstogg", "gstopengl", "gstopus", "gstplayback", "gsttheora",
    "gsttypefindfunctions", "gstvideoconvertscale", "gstvolume", "gstvorbis",
    "gstaudiofx", "gstaudioparsers", "gstautodetect", "gstdeinterlace",
    "gstid3demux", "gstinterleave", "gstisomp4", "gstmatroska", "gstrtp",
    "gstrtpmanager", "gstvideofilter", "gstvpx", "gstwavparse",
    "gstaudiobuffersplit", "gstdtls", "gstid3tag", "gstproxy",
    "gstvideoparsersbad", "gstwebrtc", "gstlibav",
    # macOS
    "gstosxaudio", "gstosxvideo", "gstapplemedia",
]


def is_system_library(path):
    """System libraries are present everywhere and must not be packaged."""
    return path.startswith("/System/Library") or path.startswith("/usr/lib") or ".asan." in path


def non_system_dependencies(binary):
    """Every non-system dylib `binary` links against, as otool reports them."""
    result = subprocess.run(
        ["/usr/bin/otool", "-L", binary], capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        # A plugin that has been renamed or dropped between GStreamer versions
        # used to surface here as a bare CalledProcessError traceback naming
        # otool, which says nothing about which plugin or why.
        raise SystemExit(
            f"otool could not read {binary}\n"
            f"  {result.stderr.strip()}\n"
            "If this is a plugin, check PLUGINS in this file against "
            "gstreamer_plugin_lists/ in the servo crate."
        )
    output = result.stdout
    found = set()
    for line in output.splitlines():
        if not line.startswith("\t"):
            continue
        dependency = line.split(" ", 1)[0].strip()
        if not is_system_library(dependency) and "librustc-stable_rt" not in dependency:
            found.add(dependency)
    return found


def resolve_rpath(dependency, rpath):
    """Turn an @rpath/... dependency into a real path.

    Not everything sits beside the binary that references it: plugins live in a
    `gstreamer-1.0` subdirectory of their own.
    """
    if not dependency.startswith("@rpath/"):
        return dependency
    # removeprefix, not replace: `replace` strips every occurrence, so a path
    # that legitimately contained the string twice came out mangled.
    relative = dependency[len("@rpath/"):]
    for directory in ["", "..", "gstreamer-1.0"]:
        candidate = os.path.join(rpath, directory, relative)
        if os.path.exists(candidate):
            return os.path.normpath(candidate)
    raise SystemExit(f"cannot resolve rpath dependency: {dependency}")


def rewrite_to_relative(binary, dependencies):
    """Point `binary` at its dependencies inside the bundle instead of at
    wherever GStreamer happens to be installed on this machine.

    Two differences from Servo's version. It rewrites `@rpath/...` references
    too — Servo can leave those alone because the framework's own layout is
    preserved under its rpath, but everything is flattened into one directory
    here, so `@rpath/lib/libglib-2.0.0.dylib` would resolve to nothing.

    And it rewrites to `@rpath/<name>` rather than an `@executable_path` path,
    leaving one LC_RPATH to say where that is. Rewritten names must not be
    longer than the originals: the load commands have to fit in the space the
    linker left, and `install_name_tool` refuses outright when they do not.
    """
    for dependency in dependencies:
        if is_system_library(dependency):
            continue
        new_path = os.path.join("@rpath", os.path.basename(dependency))
        result = subprocess.run(
            ["install_name_tool", "-change", dependency, new_path, binary],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            print(f"  ! install_name_tool on {os.path.basename(binary)}: {result.stderr.strip()}")


def main(app):
    if not os.path.exists(GSTREAMER_ROOT):
        raise SystemExit(
            f"GStreamer is not installed at {GSTREAMER_ROOT}.\n"
            "Servo only supports the official distribution; see docs/PACKAGING.md."
        )

    binary = os.path.join(app, "Contents", "MacOS", "Zervo")
    if not os.path.exists(binary):
        raise SystemExit(f"no binary at {binary}")

    # Servo hardcodes `<directory of the executable>/lib` on macOS.
    libraries = os.path.join(app, "Contents", "MacOS", "lib")
    gstreamer_libs = os.path.join(GSTREAMER_ROOT, "lib")
    relative_path = os.path.relpath(libraries, os.path.dirname(binary))

    if os.path.exists(libraries):
        shutil.rmtree(libraries)
    os.makedirs(libraries)

    # Give the binary an rpath into the bundle. Servo's carries one already;
    # ours does not, and without it any reference we fail to rewrite dangles
    # with "no LC_RPATH's found" before the app even starts.
    added = subprocess.run(
        [
            "install_name_tool",
            "-add_rpath",
            os.path.join("@executable_path", relative_path),
            binary,
        ],
        capture_output=True,
        text=True,
    )
    # Unchecked, this produced a bundle that built cleanly and died at launch
    # with "Library not loaded: @rpath/..., no LC_RPATH's found" — the exact
    # failure the rpath is here to prevent. An rpath that is already present is
    # not an error, only a repeat.
    if added.returncode != 0 and "would duplicate path" not in added.stderr:
        raise SystemExit(f"could not add an rpath to {binary}:\n  {added.stderr.strip()}")

    # The plugins are dlopened by name, so otool never mentions them. They have
    # to be listed explicitly or playback fails at runtime with no missing
    # symbol to point at why.
    pending = non_system_dependencies(binary)
    missing = []
    for plugin in PLUGINS:
        path = os.path.join(gstreamer_libs, "gstreamer-1.0", f"lib{plugin}.dylib")
        if os.path.exists(path):
            pending.add(path)
        else:
            missing.append(plugin)
    if missing:
        # Named rather than fatal. Plugins come and go between GStreamer
        # releases, and one absent codec is a format that will not play — not a
        # reason to throw away a bundle that took hours to build. Failing here
        # is what a stale list used to do, with a traceback naming otool.
        print(f"  ! not in this GStreamer, skipped: {', '.join(missing)}")
    rewrite_to_relative(binary, pending)

    copied = set()
    by_basename = {}
    count = 0
    while pending:
        batch, pending = set(pending), set()
        for dependency in batch:
            copied.add(dependency)
            source = resolve_rpath(dependency, gstreamer_libs)
            transitive = non_system_dependencies(source)

            name = os.path.basename(source)
            destination = os.path.join(libraries, name)
            if not os.path.exists(destination):
                shutil.copyfile(source, destination)
                os.chmod(destination, 0o755)
                by_basename[name] = source
                count += 1
                rewrite_to_relative(destination, transitive)
            elif by_basename.get(name) not in (None, source):
                # Everything is flattened into one directory, so two libraries
                # with the same file name and different contents cannot both be
                # there. First writer used to win in silence, and whatever
                # linked against the loser got the wrong library.
                print(
                    f"  ! two different libraries are both called {name}:\n"
                    f"      kept    {by_basename[name]}\n"
                    f"      dropped {source}"
                )

            pending.update(transitive - copied)

    total = sum(
        os.path.getsize(os.path.join(libraries, name)) for name in os.listdir(libraries)
    )
    print(f"  GStreamer: {count} libraries, {total / 1024 / 1024:.0f} MB")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    main(sys.argv[1])
