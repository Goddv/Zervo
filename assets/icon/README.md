# Heion app icon

Horizon depicts a sun above two level lines. The
[scalable icon](zervo-icon-default.svg) is the artwork for Linux and the
portable app icon. The [Icon Composer document](Heion.icon) contains the
sun, horizon lines, and indigo field as separate layers for macOS.

Render the scalable icon at 1024 pixels to `zervo-1024.png`. Run
`./scripts/make-icons.py` to update the launcher PNGs and Windows ICO.
The macOS bundler turns the export into
`Zervo.icns` with the command-line tools. With an Icon Composer-compatible
Xcode toolchain, it also compiles `Heion.icon` into the bundle's asset catalog
so macOS can apply the selected icon appearance. The darker
`zervo-macos-dark-512.png` variant is used in dark mode in the app.

The filenames are used by existing packaging and update scripts.
