#!/usr/bin/env bash
# Build (or update) a Debian/Ubuntu APT repository from a directory of .deb files.
#
# The output directory is a normal static tree suitable for GitHub Pages:
#
#   <output>/
#     dists/<codename>/Release
#     dists/<codename>/InRelease            (signed)
#     dists/<codename>/Release.gpg          (signed)
#     dists/<codename>/main/binary-<arch>/Packages{,.gz}
#     pool/main/p/password-cracking/*.deb
#     key.gpg                               (armored public key, when signing)
#
# Existing packages are preserved, so the repository can be cloned, updated and
# pushed back on every release.
#
# Usage:
#   scripts/build-apt-repo.sh --debs dist --output site --version 0.2.0
#   scripts/build-apt-repo.sh --debs dist --output site --version 0.2.0 \
#       --gpg-key /path/to/private-key.asc
#
# Requirements: dpkg-deb, dpkg-scanpackages (dpkg-dev), gzip, gpg (for signing).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

DEBS_DIR=""
OUT_DIR=""
VERSION=""
CODENAME="stable"
ORIGIN="password-cracking"
LABEL="password-cracking"
COMPONENT="main"
GPG_KEY=""
PAGES_URL=""

usage() {
    sed -n '2,25p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    cat <<'EOF'

Options:
  --debs <dir>       Directory containing .deb files (required)
  --output <dir>     Repository root; existing content is preserved (required)
  --version <ver>    Version shown on the landing page
  --codename <name>  Distribution codename (default: stable)
  --component <name> Component name (default: main)
  --gpg-key <file>   Armored private key used to sign Release/InRelease
  --pages-url <url>  Public base URL written to the landing page
  -h, --help         Show this help
EOF
}

die() {
    echo "error: $*" >&2
    exit 1
}

while [ $# -gt 0 ]; do
    case "$1" in
        --debs) DEBS_DIR="${2:-}"; shift 2 ;;
        --output) OUT_DIR="${2:-}"; shift 2 ;;
        --version) VERSION="${2:-}"; shift 2 ;;
        --codename) CODENAME="${2:-}"; shift 2 ;;
        --component) COMPONENT="${2:-}"; shift 2 ;;
        --gpg-key) GPG_KEY="${2:-}"; shift 2 ;;
        --pages-url) PAGES_URL="${2:-}"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) die "unknown option: $1" ;;
    esac
done

[ -n "$DEBS_DIR" ] || die "--debs is required"
[ -n "$OUT_DIR" ] || die "--output is required"
[ -d "$DEBS_DIR" ] || die "not a directory: $DEBS_DIR"

command -v dpkg-deb >/dev/null 2>&1 || die "dpkg-deb not found (install dpkg-dev)"
command -v dpkg-scanpackages >/dev/null 2>&1 || die "dpkg-scanpackages not found (install dpkg-dev)"
command -v gzip >/dev/null 2>&1 || die "gzip not found"

POOL_REL="pool/${COMPONENT}/p/password-cracking"
DIST_REL="dists/${CODENAME}"
mkdir -p "${OUT_DIR}/${POOL_REL}" "${OUT_DIR}/${DIST_REL}/${COMPONENT}"

# ---------------------------------------------------------------------------
# 1. Copy new packages into the pool (idempotent by filename).
# ---------------------------------------------------------------------------
shopt -s nullglob
debs=("$DEBS_DIR"/*.deb)
shopt -u nullglob
[ ${#debs[@]} -gt 0 ] || die "no .deb files in $DEBS_DIR"

for deb in "${debs[@]}"; do
    cp -f "$deb" "${OUT_DIR}/${POOL_REL}/"
done

# ---------------------------------------------------------------------------
# 2. Regenerate Packages for every architecture that has packages.
# ---------------------------------------------------------------------------
cd "$OUT_DIR"

arches="$(mktemp)"
trap 'rm -f "$arches"; [ -n "${GNUPGHOME_TMP:-}" ] && rm -rf "$GNUPGHOME_TMP"' EXIT

for deb in "${POOL_REL}"/*.deb; do
    arch="$(dpkg-deb -f "$deb" Architecture)"
    echo "$arch" >> "$arches"
done
# Keep architectures that were published previously but have no new packages.
if [ -d "${DIST_REL}/${COMPONENT}" ]; then
    for dir in "${DIST_REL}/${COMPONENT}"/binary-*; do
        [ -d "$dir" ] || continue
        basename "$dir" | sed 's/^binary-//' >> "$arches"
    done
fi

sort -u "$arches" -o "$arches"
[ -s "$arches" ] || die "could not determine package architectures"

arch_list=""
while IFS= read -r arch; do
    [ -n "$arch" ] || continue
    target="${DIST_REL}/${COMPONENT}/binary-${arch}"
    mkdir -p "$target"
    # --multiversion keeps old versions so `apt install pkg=<version>` and
    # downgrades keep working. dpkg-scanpackages otherwise drops older ones.
    dpkg-scanpackages --multiversion --arch "$arch" pool > "${target}/Packages"
    gzip -9 -k -f "${target}/Packages"
    arch_list="${arch_list}${arch_list:+ }${arch}"
    echo "indexed ${arch}: $(grep -c '^Package:' "${target}/Packages" || true) package(s)"
done < "$arches"

# ---------------------------------------------------------------------------
# 3. Write the Release file.
# ---------------------------------------------------------------------------
release="${DIST_REL}/Release"
{
    echo "Origin: ${ORIGIN}"
    echo "Label: ${LABEL}"
    echo "Suite: ${CODENAME}"
    echo "Codename: ${CODENAME}"
    echo "Components: ${COMPONENT}"
    echo "Architectures: ${arch_list}"
    echo "Description: ${LABEL} APT repository"
    echo "Date: $(LC_ALL=C date -Ru)"
} > "$release"

write_hash_block() {
    local field="$1" command="$2"
    echo "${field}:"
    while IFS= read -r file; do
        [ -f "$file" ] || continue
        local rel="${file#"${DIST_REL}"/}"
        local digest size
        digest="$("$command" "$file" | awk '{print $1}')"
        size="$(wc -c < "$file" | tr -d ' ')"
        printf ' %s %16s %s\n' "$digest" "$size" "$rel"
    done < <(find "${DIST_REL}/${COMPONENT}" -type f \( -name 'Packages' -o -name 'Packages.gz' \) | sort)
}

write_hash_block "MD5Sum" "md5sum" >> "$release"
write_hash_block "SHA1" "sha1sum" >> "$release"
write_hash_block "SHA256" "sha256sum" >> "$release"

# ---------------------------------------------------------------------------
# 4. Sign (optional). A signed repository lets users run plain `apt update`
#    without [trusted=yes].
# ---------------------------------------------------------------------------
if [ -n "$GPG_KEY" ]; then
    [ -f "$GPG_KEY" ] || die "GPG key not found: $GPG_KEY"
    command -v gpg >/dev/null 2>&1 || die "gpg not found"

    GNUPGHOME_TMP="$(mktemp -d)"
    export GNUPGHOME="$GNUPGHOME_TMP"
    chmod 700 "$GNUPGHOME"
    gpg --batch --quiet --import "$GPG_KEY"

    key_id="$(gpg --list-secret-keys --with-colons | awk -F: '/^sec:/ {print $5; exit}')"
    [ -n "$key_id" ] || die "no secret key found in $GPG_KEY"

    gpg --batch --yes --armor --local-user "$key_id" --detach-sign \
        --output "${release}.gpg" "$release"
    gpg --batch --yes --local-user "$key_id" --clearsign \
        --output "${DIST_REL}/InRelease" "$release"
    gpg --batch --export "$key_id" > key.gpg
    echo "signed with key ${key_id}"
else
    rm -f "${release}.gpg" "${DIST_REL}/InRelease"
    echo "warning: no --gpg-key supplied; repository is unsigned"
fi

# ---------------------------------------------------------------------------
# 5. Landing page (Jekyll must stay out of the way).
# ---------------------------------------------------------------------------
touch .nojekyll
if [ -f "${REPO_ROOT}/packaging/apt/index.html" ]; then
    sed -e "s|@@VERSION@@|${VERSION:-latest}|g" \
        -e "s|@@PAGES_URL@@|${PAGES_URL}|g" \
        "${REPO_ROOT}/packaging/apt/index.html" > index.html
fi

echo "APT repository ready in ${OUT_DIR}"
echo "  suites:   ${CODENAME}"
echo "  arches:   ${arch_list}"
echo "  packages: $(find pool -name '*.deb' | wc -l | tr -d ' ')"
