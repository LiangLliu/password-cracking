#!/usr/bin/env python3
"""Generate package-manager manifests from GitHub Release artifacts.

One release produces one set of assets; this script turns them into:

    password-cracking.rb        Homebrew formula (macOS + Linux)
    password-cracking.json      Scoop manifest (Windows)
    PKGBUILD                    AUR package (Arch Linux)
    PKGBUILD.SRCINFO            AUR metadata, installed as `.SRCINFO`

Usage:

    python3 scripts/gen-package-manifests.py --dist dist --version 0.2.0 --out manifests
    python3 scripts/gen-package-manifests.py --dist dist --version 0.2.0 --out manifests \
        --base-url http://127.0.0.1:8000

`--dist` must contain the archives produced by .github/workflows/ci.yml and a
`<archive>.sha256` file next to each one. Missing checksum files are tolerated:
the hash is then computed from the archive.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

REPO = "LiangLliu/password-cracking"

# asset name -> template placeholder
ASSETS = {
    "password-cracking-macos-arm64.tar.gz": "SHA_MACOS_ARM64",
    "password-cracking-macos-amd64.tar.gz": "SHA_MACOS_AMD64",
    "password-cracking-linux-amd64-musl.tar.gz": "SHA_LINUX_AMD64_MUSL",
    "password-cracking-linux-arm64-musl.tar.gz": "SHA_LINUX_ARM64_MUSL",
    "password-cracking-linux-amd64.tar.gz": "SHA_LINUX_AMD64",
    "password-cracking-linux-arm64.tar.gz": "SHA_LINUX_ARM64",
    "password-cracking-windows-amd64.zip": "SHA_WINDOWS_AMD64",
    "password-cracking-windows-i686.zip": "SHA_WINDOWS_I686",
}

HEX64 = re.compile(r"^[0-9a-fA-F]{64}$")


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def hash_for(dist: Path, name: str) -> str:
    """Read `<name>.sha256` (first field) or hash the archive itself."""
    checksum = dist / f"{name}.sha256"
    if checksum.is_file():
        fields = checksum.read_text(encoding="utf-8", errors="replace").split()
        if fields and HEX64.match(fields[0]):
            return fields[0].lower()
        print(f"warning: ignoring malformed checksum file {checksum}", file=sys.stderr)

    archive = dist / name
    if not archive.is_file():
        raise SystemExit(f"error: missing release asset: {archive}")
    return sha256_of(archive)


def render(template: Path, values: dict[str, str]) -> str:
    text = template.read_text(encoding="utf-8")
    for key, value in values.items():
        text = text.replace(f"@@{key}@@", value)
    leftover = re.findall(r"@@[A-Z0-9_]+@@", text)
    if leftover:
        raise SystemExit(f"error: {template} has unresolved placeholders: {leftover}")
    return text


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dist", type=Path, required=True, help="directory with release assets")
    parser.add_argument("--version", required=True, help="release version, e.g. 0.2.0 or v0.2.0")
    parser.add_argument("--out", type=Path, required=True, help="output directory")
    parser.add_argument("--repo", default=REPO, help=f"GitHub repository (default: {REPO})")
    parser.add_argument(
        "--base-url",
        help="override the release download URL (useful for local testing)",
    )
    args = parser.parse_args()

    version = args.version.strip().lstrip("v")
    if not version:
        raise SystemExit("error: --version is empty")

    dist = args.dist.resolve()
    if not dist.is_dir():
        raise SystemExit(f"error: --dist is not a directory: {dist}")

    base_url = args.base_url or f"https://github.com/{args.repo}/releases/download/v{version}"
    root = Path(__file__).resolve().parent.parent
    templates = root / "packaging"

    values = {
        "VERSION": version,
        "BASE_URL": base_url,
        "REPO": args.repo,
        "HOMEPAGE": f"https://github.com/{args.repo}",
    }
    for asset, key in ASSETS.items():
        values[key] = hash_for(dist, asset)

    outputs = {
        "password-cracking.rb": templates / "homebrew" / "password-cracking.rb.tmpl",
        "password-cracking.json": templates / "scoop" / "password-cracking.json.tmpl",
        "PKGBUILD": templates / "aur" / "PKGBUILD.tmpl",
        # Do not use a leading dot: `gh release upload` renames dotfiles to
        # "default.<ext>". The AUR checkout copies this to `.SRCINFO`.
        "PKGBUILD.SRCINFO": templates / "aur" / ".SRCINFO.tmpl",
    }

    args.out.mkdir(parents=True, exist_ok=True)
    for name, template in outputs.items():
        rendered = render(template, values)
        (args.out / name).write_text(rendered, encoding="utf-8")
        print(f"generated {args.out / name}")

    # Fail fast on malformed JSON instead of shipping a broken Scoop manifest.
    json.loads((args.out / "password-cracking.json").read_text(encoding="utf-8"))

    print(f"\nversion:  {version}")
    print(f"base url: {base_url}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
