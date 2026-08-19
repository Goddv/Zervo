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
    output = subprocess.run(
        ["/usr/bin/otool", "-L", binary], capture_output=True, text=True, check=True
    ).stdout
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
    relative = dependency.replace("@rpath/", "")
    for directory in ["", "..", "gstreamer-1.0"]:
        candidate = os.path.join(rpath, directory, relative)
        if os.path.exists(candidate):
            return os.path.normpath(candidate)
    raise SystemExit(f"cannot resolve rpath dependency: {dependency}")


def rewrite_to_relative(binary, dependencies, relative_path):
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
    subprocess.run(
        [
            "install_name_tool",
            "-add_rpath",
            os.path.join("@executable_path", relative_path),
            binary,
        ],
        capture_output=True,
        text=True,
    )

    # The plugins are dlopened by name, so otool never mentions them. They have
    # to be listed explicitly or playback fails at runtime with no missing
    # symbol to point at why.
    pending = non_system_dependencies(binary)
    pending.update(
        os.path.join(gstreamer_libs, "gstreamer-1.0", f"lib{plugin}.dylib") for plugin in PLUGINS
    )
    rewrite_to_relative(binary, pending, relative_path)

    copied = set()
    count = 0
    while pending:
        batch, pending = set(pending), set()
        for dependency in batch:
            copied.add(dependency)
            source = resolve_rpath(dependency, gstreamer_libs)
            transitive = non_system_dependencies(source)

            destination = os.path.join(libraries, os.path.basename(source))
            if not os.path.exists(destination):
                shutil.copyfile(source, destination)
                os.chmod(destination, 0o755)
                count += 1
                rewrite_to_relative(destination, transitive, relative_path)

            pending.update(transitive - copied)

    total = sum(
        os.path.getsize(os.path.join(libraries, name)) for name in os.listdir(libraries)
    )
    print(f"  GStreamer: {count} libraries, {total / 1024 / 1024:.0f} MB")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    main(sys.argv[1])
