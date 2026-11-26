"""Attach the expected executable digest and sign each architecture-specific delta."""

import re
import subprocess
import sys
from pathlib import Path
from xml.etree import ElementTree as ET


def main() -> None:
    feed, stage_dir, arch, version, prefix, sign_update = sys.argv[1:]
    stage = Path(stage_dir)
    digest_hex = (stage / "target-executable.sha256").read_text().strip()
    if not re.fullmatch(r"[0-9a-f]{64}", digest_hex):
        raise SystemExit("Missing or invalid target executable digest")
    trailer = b"ZERVOD01" + bytes.fromhex(digest_hex)
    seed = sys.stdin.buffer.read()
    namespace = "http://www.andymatuschak.org/xml-namespaces/sparkle"
    ET.register_namespace("sparkle", namespace)
    tree = ET.parse(feed)
    for item in tree.findall("./channel/item"):
        display_version = item.find(f"{{{namespace}}}shortVersionString")
        if display_version is None:
            raise SystemExit("Missing Sparkle short version")
        display_version.text = version
        for delta in item.findall(f"{{{namespace}}}deltas/enclosure"):
            source = delta.attrib["url"].removeprefix(prefix)
            if not re.fullmatch(r"(?:Heion|Zervo)[0-9.]+-[0-9.]+\.delta", source):
                raise SystemExit(f"Unexpected Sparkle delta name: {source}")
            target = stage / f"{source.removesuffix('.delta')}-{arch}.delta"
            (stage / source).rename(target)
            with target.open("ab") as patch:
                patch.write(trailer)
            signature = subprocess.run(
                [sign_update, "--ed-key-file", "-", "-p", str(target)],
                input=seed, stdout=subprocess.PIPE, check=True,
            ).stdout.decode().strip()
            delta.set("url", prefix + target.name)
            delta.set("length", str(target.stat().st_size))
            delta.set(f"{{{namespace}}}edSignature", signature)
    tree.write(feed, encoding="utf-8", xml_declaration=True)


if __name__ == "__main__":
    main()
