# Zervo app icon

[Zervo.icon](Zervo.icon) is the editable Apple Icon Composer document. The four
vector layers form a frosted hub and five colored plates. The plates adapt the
silhouette of the [Servo project mark](https://github.com/servo/servo/blob/v0.8.0/resources/servo.svg).
Icon Composer supplies the macOS enclosure, system light and dark backgrounds, translucency, and highlights.

Export the macOS Default appearance as a 1024-pixel PNG and save it as
`zervo-1024.png`. Run `./scripts/make-icons.py` to update the committed
launcher PNGs and Windows ICO. The macOS bundler turns the export into
`Zervo.icns` with the command-line tools. With an Icon Composer-compatible
Xcode toolchain, it also compiles `Zervo.icon` into the bundle's asset catalog
so macOS can apply the selected icon appearance. The resized Dark export,
`zervo-macos-dark-512.png`, is used on the About page.

`zervo-icon-default.svg` is the scalable Linux icon.
