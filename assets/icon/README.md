# Heion app icon

Refraction forms an H with two glass rails and a frosted bridge. The
[Icon Composer document](Heion.icon) contains editable layers and separate
white and midnight backgrounds for the Default and Dark appearances.

Export Default and Dark from Icon Composer with transparent padding, at 1024
and 512 pixels respectively. Save them as `zervo-1024.png` and
`zervo-macos-dark-512.png`. Run `./scripts/make-icons.py` to update the
launcher PNGs and Windows ICO. The macOS bundler compiles both appearances
into the asset catalog and provides a default ICNS as a fallback.

The [scalable light icon](zervo-icon-default.svg) is used by Linux launchers.
Existing packaging and update scripts use the historical filenames.
