#!/usr/bin/env bash
# Smoke-test the package manifest generator without building real artifacts.
#
# Creates dummy release assets, renders every manifest, and validates the
# result (Ruby syntax, shell syntax, required fields). Runs on macOS and
# Linux, so it can be used both locally and in CI.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

DIST="$TMP/dist"
OUT="$TMP/out"
mkdir -p "$DIST"

ASSETS=(
    password-cracking-macos-arm64.tar.gz
    password-cracking-macos-amd64.tar.gz
    password-cracking-linux-amd64-musl.tar.gz
    password-cracking-linux-arm64-musl.tar.gz
    password-cracking-linux-amd64.tar.gz
    password-cracking-linux-arm64.tar.gz
)

for asset in "${ASSETS[@]}"; do
    printf 'dummy %s\n' "$asset" > "$DIST/$asset"
done

# Write <asset>.sha256 files with the real digest of each dummy file.
python3 - "$DIST" <<'PY'
import hashlib
import pathlib
import sys

dist = pathlib.Path(sys.argv[1])
for path in sorted(dist.iterdir()):
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    (dist / f"{path.name}.sha256").write_text(f"{digest}  {path.name}\n")
PY

python3 "$ROOT/scripts/gen-package-manifests.py" \
    --dist "$DIST" --version v9.9.9 --out "$OUT" >/dev/null

test -s "$OUT/password-cracking.rb"
test -s "$OUT/PKGBUILD"
test -s "$OUT/PKGBUILD.SRCINFO"

ruby -c "$OUT/password-cracking.rb" >/dev/null
bash -n "$OUT/PKGBUILD"

grep -q '9\.9\.9' "$OUT/password-cracking.rb"
grep -q 'pkgver=9\.9\.9' "$OUT/PKGBUILD"
grep -q 'pkgver = 9\.9\.9' "$OUT/PKGBUILD.SRCINFO"
grep -qP '^\tsha256sums_x86_64 = [0-9a-f]{64}$' "$OUT/PKGBUILD.SRCINFO" 2>/dev/null \
    || grep -q "$(printf '\tsha256sums_x86_64 = ')" "$OUT/PKGBUILD.SRCINFO"

echo "package manifests OK"
