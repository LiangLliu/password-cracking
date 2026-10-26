#!/usr/bin/env bash
# Install the password-cracking binary from GitHub Releases.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/LiangLliu/password-cracking/master/scripts/install.sh | sh
#   ./scripts/install.sh --version v0.2.0 --prefix "$HOME/.local"
#
# Environment variables:
#   VERSION   Release tag (default: latest)
#   PREFIX    Install prefix; the binary goes to "$PREFIX/bin"
#
# The script prefers the static musl build on Linux so the binary runs on any
# distribution (glibc version independent).
set -euo pipefail

REPO="LiangLliu/password-cracking"
BIN="password-cracking"
VERSION="${VERSION:-latest}"
PREFIX="${PREFIX:-}"

usage() {
    cat <<'EOF'
Install password-cracking from GitHub Releases.

Options:
  --version <tag>   Release tag to install (e.g. v0.2.0); default: latest
  --prefix <dir>    Install prefix, binary goes to <dir>/bin; default: /usr/local
                    if writable, otherwise ~/.local
  --dry-run         Print the download URL and exit
  -h, --help        Show this help
EOF
}

DRY_RUN=0
while [ $# -gt 0 ]; do
    case "$1" in
        --version) VERSION="$2"; shift 2 ;;
        --prefix) PREFIX="$2"; shift 2 ;;
        --dry-run) DRY_RUN=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown option: $1" >&2; usage >&2; exit 1 ;;
    esac
done

detect_os() {
    case "$(uname -s)" in
        Linux) echo "linux" ;;
        Darwin) echo "macos" ;;
        *) echo "Unsupported OS: $(uname -s)" >&2; exit 1 ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64) echo "amd64" ;;
        arm64|aarch64) echo "arm64" ;;
        armv7l|armv7) echo "armv7" ;;
        *) echo "Unsupported architecture: $(uname -m)" >&2; exit 1 ;;
    esac
}

OS="$(detect_os)"
ARCH="$(detect_arch)"

# Prefer the static musl build on Linux for maximum compatibility (glibc
# version independent, works on Alpine and older distributions).
if [ "$OS" = "linux" ]; then
    case "$ARCH" in
        amd64|arm64|armv7) ASSET="${BIN}-linux-${ARCH}-musl.tar.gz" ;;
        *) ASSET="${BIN}-linux-${ARCH}.tar.gz" ;;
    esac
else
    ASSET="${BIN}-macos-${ARCH}.tar.gz"
fi

if [ "$VERSION" = "latest" ]; then
    BASE_URL="https://github.com/${REPO}/releases/latest/download"
else
    BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
fi
URL="${BASE_URL}/${ASSET}"

if [ "$DRY_RUN" = "1" ]; then
    echo "Would install ${URL}"
    exit 0
fi

# Pick an install prefix without requiring sudo.
if [ -z "$PREFIX" ]; then
    if [ -w /usr/local ] || [ -w /usr/local/bin ] 2>/dev/null; then
        PREFIX="/usr/local"
    else
        PREFIX="${HOME}/.local"
    fi
fi
INSTALL_DIR="${PREFIX}/bin"

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading ${URL}"
if command -v curl >/dev/null 2>&1; then
    curl -fL --retry 3 -o "${TMPDIR}/${ASSET}" "$URL"
else
    wget -O "${TMPDIR}/${ASSET}" "$URL"
fi

# Verify the SHA-256 checksum when the release provides one.
if curl -fsL -o "${TMPDIR}/${ASSET}.sha256" "${URL}.sha256" 2>/dev/null; then
    if command -v sha256sum >/dev/null 2>&1; then
        (cd "$TMPDIR" && sha256sum -c "${ASSET}.sha256")
    else
        (cd "$TMPDIR" && shasum -a 256 -c "${ASSET}.sha256")
    fi
else
    echo "Warning: checksum file not available; skipping verification" >&2
fi

tar -xzf "${TMPDIR}/${ASSET}" -C "$TMPDIR"

if [ ! -f "${TMPDIR}/${BIN}" ]; then
    # Release archives may place the binary under a directory.
    FOUND="$(find "$TMPDIR" -type f -name "$BIN" -perm -u+x | head -n 1)"
    if [ -z "$FOUND" ]; then
        echo "Could not find ${BIN} in the archive" >&2
        exit 1
    fi
    mv "$FOUND" "${TMPDIR}/${BIN}"
fi

mkdir -p "$INSTALL_DIR"
install -m 0755 "${TMPDIR}/${BIN}" "${INSTALL_DIR}/${BIN}"

echo "Installed ${BIN} to ${INSTALL_DIR}/${BIN}"
case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) ;;
    *) echo "Note: ${INSTALL_DIR} is not on your PATH." ;;
esac
echo "Other installation methods (Homebrew, APT, AUR, Docker):"
echo "  https://github.com/${REPO}/blob/master/docs/DISTRIBUTION.md"
"${INSTALL_DIR}/${BIN}" --version
