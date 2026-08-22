#!/usr/bin/env python3
"""Render every raster icon Zervo ships from the one 1024x1024 master.

    ./scripts/make-icons.py

Produces, all committed so a release needs no image tooling:

    assets/icon/zervo-{128,256,512}.png   the hicolor sizes the Linux packages
                                          install, at the sizes their directory
                                          names claim
    assets/icon/zervo.ico                 the Windows installer's icon

`assets/icon/Zervo.icns` is built separately by scripts/make-icns.sh, which
uses stock macOS tools and so cannot run anywhere else.

The master itself, assets/icon/zervo-1024.png, is the Icon Composer document
in assets/icon/Zervo.icon rendered flat; re-export it from there after editing
the layers.

Needs Pillow. This is the one script in the tree with a dependency outside the
standard library, which is why its output is committed rather than generated at
package time.
"""

import sys
from pathlib import Path

try:
    from PIL import Image
except ImportError:
    raise SystemExit("this needs Pillow: pip install Pillow")

ROOT = Path(__file__).resolve().parent.parent
MASTER = ROOT / "assets" / "icon" / "zervo-1024.png"

# The sizes the .deb, .rpm, tarball and AppDir install into
# /usr/share/icons/hicolor/<size>x<size>/apps/. 1024 is the master itself.
HICOLOR = (128, 256, 512)

# What Windows wants in an .ico. 256 is the largest the format can describe —
# its width and height fields are one byte each, with zero meaning 256.
ICO = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]


def main() -> int:
    if not MASTER.exists():
        raise SystemExit(f"missing {MASTER}")

    master = Image.open(MASTER).convert("RGBA")
    if master.size != (1024, 1024):
        raise SystemExit(f"{MASTER} is {master.size[0]}x{master.size[1]}, expected 1024x1024")

    for size in HICOLOR:
        out = MASTER.with_name(f"zervo-{size}.png")
        # Lanczos: the artwork is a thin ring around a glyph, and anything
        # cheaper aliases both into a smear at 128.
        master.resize((size, size), Image.LANCZOS).save(out, optimize=True)
        print(f"wrote {out.relative_to(ROOT)}")

    ico = MASTER.with_name("zervo.ico")
    master.save(ico, format="ICO", sizes=ICO)
    print(f"wrote {ico.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
