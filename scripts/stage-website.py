"""Stage Heion's GitHub Pages site with the latest stable installer URLs."""

import json
from pathlib import Path
import re
import shutil
import sys


TAG = r"heion-v(\d+\.\d+\.\d+(?:\.\d+)?)"
ARCHITECTURES = ("arm64", "x86_64")


def version_order(version: str) -> tuple[int, ...]:
    return tuple(int(part) for part in version.split(".") + ["0"])[:4]


def replace_once(page: str, previous: str, updated: str) -> str:
    if page.count(previous) != 1:
        raise SystemExit(f"Expected exactly one {previous!r} in docs/index.html")
    return page.replace(previous, updated, 1)


def main() -> None:
    release_path, repository = sys.argv[1:]
    release = json.loads(Path(release_path).read_text())
    latest = re.fullmatch(TAG, release["tag_name"], re.ASCII)
    if not latest or release["draft"] or release["prerelease"]:
        raise SystemExit("Expected a public stable Heion release")

    docs = Path("docs")
    page = (docs / "index.html").read_text(encoding="utf-8")
    minimum = re.search(rf'name="heion-release" content="({TAG})"', page, re.ASCII)
    if not minimum:
        raise SystemExit("Missing a valid Heion release in the site metadata")
    latest_tag, latest_version = latest[0], latest[1]
    minimum_tag, minimum_version = minimum[1], minimum[2]
    if version_order(latest_version) < version_order(minimum_version):
        print(f"::notice::The site will deploy when {minimum_tag} is public.")
        return

    assets = {asset["name"] for asset in release["assets"] if asset["state"] == "uploaded"}
    base = f"https://github.com/{repository}/releases"
    page = replace_once(page, f'content="{minimum_tag}"', f'content="{latest_tag}"')
    page = replace_once(page, f"{base}/tag/{minimum_tag}", f"{base}/tag/{latest_tag}")
    page = replace_once(page, f"Get Heion {minimum_version}</h2>", f"Get Heion {latest_version}</h2>")
    for arch in ARCHITECTURES:
        filename = f"Heion-{latest_version}-{arch}.dmg"
        if filename not in assets:
            raise SystemExit(f"Missing public installer {filename}")
        page = replace_once(
            page,
            f"{base}/download/{minimum_tag}/Heion-{minimum_version}-{arch}.dmg",
            f"{base}/download/{latest_tag}/{filename}",
        )

    stage = Path("_site")
    shutil.copytree(docs / "site", stage / "site")
    (stage / "index.html").write_text(page, encoding="utf-8")
    print(f"Staged the website for {latest_tag}.")


if __name__ == "__main__":
    main()
