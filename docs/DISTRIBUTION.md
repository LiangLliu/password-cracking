# Distribution Channels

**English** | [中文](DISTRIBUTION.zh-CN.md)

One tag push drives every channel. The GitHub Release remains the source of
truth for binaries and checksums.

| Channel | Platform | Command | Status |
|---------|----------|---------|--------|
| [Homebrew](#homebrew) | macOS, Linux | `brew install LiangLliu/tap/password-cracking` | live, verified for v0.2.1 |
| [APT](#debian--ubuntu-apt) | Debian, Ubuntu, Raspberry Pi OS | `sudo apt install password-cracking` | live, signed, verified |
| [Docker](#docker-ghcr) | Linux containers | `docker run ghcr.io/lianglliu/password-cracking` | live, amd64 + arm64 |
| [AUR](#arch-linux-aur) | Arch Linux | `yay -S password-cracking-bin` | PKGBUILD generated; submission pending |
| [crates.io](#cratesio) | any Rust platform | `cargo install password-cracking` | pipeline ready; token required |
| [install.sh](#installsh) | Linux, macOS | `curl …/install.sh \| sh` | live, verified |
| [GitHub Releases](https://github.com/LiangLliu/password-cracking/releases) | Linux, macOS | direct download | 39 assets + `SHA256SUMS` |

---

## Homebrew

```bash
brew install LiangLliu/tap/password-cracking
brew upgrade password-cracking
```

The formula lives in
[`LiangLliu/homebrew-tap`](https://github.com/LiangLliu/homebrew-tap) and is
updated automatically on every release. macOS uses the native builds; Linux
uses the static musl builds, so the formula is independent of the distribution's
glibc version.

The formula installs the binary, man page and bash/zsh/fish completions.

Verified on macOS arm64: `brew install`, `brew test`, completions, man page and
a real fixture crack.

## Debian / Ubuntu (APT)

The repository is hosted on GitHub Pages and signed with the project key.

```bash
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://lianglliu.github.io/password-cracking/key.gpg \
  | sudo tee /etc/apt/keyrings/password-cracking.gpg > /dev/null

echo "deb [signed-by=/etc/apt/keyrings/password-cracking.gpg] \
https://lianglliu.github.io/password-cracking stable main" \
  | sudo tee /etc/apt/sources.list.d/password-cracking.list

sudo apt update
sudo apt install password-cracking
```

Signing key fingerprint:

```text
17E4 9D06 F4E3 B63C 8868  EC40 C334 9BA9 9EBE 5A92
```

Verify it with:

```bash
gpg --show-keys /etc/apt/keyrings/password-cracking.gpg
```

Supported architectures: `amd64`, `arm64`, `armhf` (Raspberry Pi 2+).

Pinning works as usual:

```bash
sudo apt install password-cracking=0.2.1
# Downgrading an installed version also needs --allow-downgrades
```

The repository keeps multiple versions (`dpkg-scanpackages --multiversion`), so
pinning and reproducible installs keep working. Each architecture costs roughly
700 KB per release; GitHub Pages limits (1 GB) are not a concern for the
foreseeable future.

Verified in a Debian container against the live URL: `apt update`, signature
check, `apt install`, and `password-cracking --version`.

## Docker (GHCR)

```bash
docker pull ghcr.io/lianglliu/password-cracking:latest

docker run --rm -v "$PWD:/data" ghcr.io/lianglliu/password-cracking \
  -f /data/document.zip dictionary -w /data/wordlist.txt
```

- Multi-arch: `linux/amd64`, `linux/arm64`
- Built `FROM scratch` around the static musl binary; the image is ~1.7 MB
- Tags: `latest`, `0.2`, `0.2.1`
- No shell, no libc, no network access; mount the target file and wordlist
- The container runs as root, so world-readable mounts work without extra
  flags. Pass `--user "$(id -u):$(id -g)"` if the wordlist is restricted.

Verified with `docker run … --version` for both the versioned and `latest` tags.

## Arch Linux (AUR)

The package name is `password-cracking-bin`.

```bash
yay -S password-cracking-bin
# or
paru -S password-cracking-bin
```

The release job generates `PKGBUILD` and `PKGBUILD.SRCINFO` and attaches them
to the release (the AUR checkout installs the latter as `.SRCINFO`). When the
`AUR_SSH_PRIVATE_KEY` secret is configured, it also pushes to
`ssh://aur@aur.archlinux.org/password-cracking-bin.git`.

The package must exist in the AUR before the first automated push:

1. Download `PKGBUILD` and `PKGBUILD.SRCINFO` from the release; rename the
   latter to `.SRCINFO`.
2. Submit `PKGBUILD` at <https://aur.archlinux.org/submit/>.
3. Add the public half of the CI key to the AUR account
   (**My Account → SSH Public Key**).
4. Store the private key as the `AUR_SSH_PRIVATE_KEY` secret.

Supported architectures: `x86_64`, `aarch64`. The PKGBUILD uses the gnu
tarballs and generates the man page and completions at package time.

## crates.io

```bash
cargo install password-cracking
# until the crate is published:
cargo install --git https://github.com/LiangLliu/password-cracking
```

Publishing is automatic when `CARGO_REGISTRY_TOKEN` is configured and the
version is not already on crates.io. The job compares `Cargo.toml` with the tag
and skips already-published versions. Create a token at
<https://crates.io/settings/tokens>, then:

```bash
gh secret set CARGO_REGISTRY_TOKEN
gh workflow run publish.yml -f version=0.2.1
```

The crate name `password-cracking` is currently available.

## install.sh

```bash
curl -fsSL https://raw.githubusercontent.com/LiangLliu/password-cracking/master/scripts/install.sh | sh
```

Detects OS and architecture, prefers the static musl build on Linux, verifies
the release SHA-256, and installs to `/usr/local` or `~/.local` without `sudo`.

## GitHub Releases

Every release contains:

- `password-cracking-linux-{amd64,arm64,armv7}{,-musl}.tar.gz`
- `password-cracking-macos-{amd64,arm64}.tar.gz`
- `.deb` for `amd64`, `arm64`, `armhf`
- `.rpm` for `x86_64`, `aarch64`, `armv7hl`
- `.apk` for `x86_64`, `aarch64`, `armv7`
- `install.sh`, `SHA256SUMS`, Homebrew formula, `PKGBUILD`, `PKGBUILD.SRCINFO`

Each archive/package has a `.sha256` file. Verify everything with:

```bash
gh release download v0.2.1 --dir dist
cd dist && sha256sum -c SHA256SUMS
```

---

## How publishing works

```text
git tag v0.2.1 && git push origin v0.2.1
        │
        ▼
CI (ci.yml)
  quality / audit / tests
        │
        ▼
build matrix (8 targets) ──► GitHub Release
        │                     tarballs, deb/rpm/apk, checksums
        ▼
publish (publish.yml, reusable)
  ├── apt       build a signed repo from .deb assets, push gh-pages
  ├── homebrew  render formula from checksums, push homebrew-tap
  ├── aur       render PKGBUILD/.SRCINFO, optional AUR push
  ├── docker    multi-arch image to ghcr.io
  └── crates    cargo publish (optional, needs token)
```

The publish workflow can be re-run for any existing release:

```bash
gh workflow run publish.yml -f version=0.2.1
```

It downloads assets from the GitHub Release, so a re-run does not rebuild
binaries. Channels without configured credentials are skipped with a notice
rather than failing the release.

### Manifests

`scripts/gen-package-manifests.py` renders every manifest from a directory of
release assets plus their `.sha256` files:

```bash
gh release download v0.2.1 --dir dist
python3 scripts/gen-package-manifests.py --dist dist --version 0.2.1 --out manifests
```

It refuses to produce a manifest when an asset is missing. Templates live in
`packaging/`.

---

## One-time setup

These steps are already done for `LiangLliu/password-cracking`; they are
documented for forks and key rotation.

### 1. APT signing key

Generate an RSA 4096 signing key. CI uses it unattended, so it has no
passphrase:

```bash
export GNUPGHOME=$(mktemp -d)
gpg --batch --generate-key <<'EOF'
%echo generating
Key-Type: RSA
Key-Length: 4096
Key-Usage: sign
Name-Real: password-cracking APT repository
Name-Email: you@example.com
Expire-Date: 0
%no-protection
%commit
EOF

gpg --batch --export-secret-keys > /tmp/apt-private.asc
gpg --batch --export > packaging/apt/password-cracking.gpg
gh secret set APT_GPG_PRIVATE_KEY < /tmp/apt-private.asc
shred -u /tmp/apt-private.asc && rm -rf "$GNUPGHOME"
```

Commit `packaging/apt/password-cracking.gpg`; the publish job copies it to the
site root as `key.gpg`. **Rotating the key breaks existing user
configurations** — announce the new fingerprint and keep the old key available
for a transition period.

### 2. GitHub Pages

The APT repository is served from the `gh-pages` branch:

```bash
gh api -X POST repos/LiangLliu/password-cracking/pages \
  --input - <<< '{"source":{"branch":"gh-pages","path":"/"}}'
```

The publish workflow enables Pages if needed and requests a build. The site is
<https://lianglliu.github.io/password-cracking/>.

### 3. Homebrew tap

1. Create `LiangLliu/homebrew-tap` (public).
2. Add a write-enabled deploy key:

```bash
ssh-keygen -t ed25519 -N '' -f tap_key
gh repo deploy-key add tap_key.pub --repo LiangLliu/homebrew-tap \
  --title "password-cracking release" --allow-write
gh secret set HOMEBREW_TAP_DEPLOY_KEY < tap_key
rm -f tap_key tap_key.pub
```

### 4. AUR

Create the package once (see [AUR](#arch-linux-aur)), then:

```bash
gh secret set AUR_SSH_PRIVATE_KEY
```

### 5. crates.io

```bash
cargo login <token>                       # https://crates.io/settings/tokens
gh secret set CARGO_REGISTRY_TOKEN
```

### 6. Container visibility

GHCR packages created by `GITHUB_TOKEN` are public for public repositories, but
verify once after the first push:

```bash
docker pull ghcr.io/lianglliu/password-cracking:latest
```

If the pull is unauthorized, open **Packages → password-cracking → Package
settings → Change visibility → Public**.

---

## Verification checklist

```bash
# 1. APT (live repository)
docker run --rm debian:stable-slim bash -c '
  apt-get update -qq &&
  apt-get install -y -qq curl gnupg ca-certificates &&
  install -m 0755 -d /etc/apt/keyrings &&
  curl -fsSL https://lianglliu.github.io/password-cracking/key.gpg \
    -o /etc/apt/keyrings/password-cracking.gpg &&
  echo "deb [signed-by=/etc/apt/keyrings/password-cracking.gpg] \
https://lianglliu.github.io/password-cracking stable main" \
    > /etc/apt/sources.list.d/pc.list &&
  apt-get update -qq && apt-get install -y -qq password-cracking &&
  password-cracking --version'

# 2. Homebrew
brew install LiangLliu/tap/password-cracking
brew test LiangLliu/tap/password-cracking
brew uninstall password-cracking

# 3. Docker
docker run --rm ghcr.io/lianglliu/password-cracking --version

# 4. Installer
./scripts/install.sh --prefix /tmp/pc

# 5. Manifests
python3 scripts/gen-package-manifests.py --dist dist --version 0.2.1 --out /tmp/m
bash -n /tmp/m/PKGBUILD
ruby -c /tmp/m/password-cracking.rb
```

Repository metadata:

```bash
curl -fsS https://lianglliu.github.io/password-cracking/dists/stable/InRelease \
  | gpg --verify
curl -fsS https://lianglliu.github.io/password-cracking/dists/stable/Release
```

---

## Troubleshooting

| Symptom | Cause / fix |
|---------|-------------|
| `apt update`: `NO_PUBKEY` | Wrong `signed-by` path, or the key was rotated. Re-download `key.gpg`. |
| `apt update`: `403` | The Pages build is still running; wait a minute and retry. |
| `brew`: `No available formula` | Tap push failed. Check the `homebrew` job and the deploy key. |
| `brew`: `SHA256 mismatch` | Assets were replaced after the formula was rendered. Re-run `publish.yml`. |
| GHCR `unauthorized` | The package is private; change its visibility. |
| `cargo publish`: `already exists` | Expected; the job skips published versions. Bump `Cargo.toml` first. |
| AUR push fails | The AUR package does not exist yet, or the SSH key is not registered. The PKGBUILD is still attached to the release. |
| `nfpm`: wrong ARMv7 architecture | Debian uses `armhf`, RPM uses `armv7hl`, Alpine uses `armv7`; CI maps them per packager. |
| `brew install` fails with `Path contains '..'` | A macOS archive was built with a parent path. The current CI archives flat names; rebuild the release. |

---

## Adding another channel

`packaging/` is manifest-first:

```text
packaging/
  apt/index.html               landing page for the APT site
  apt/password-cracking.gpg    public signing key
  aur/PKGBUILD.tmpl
  aur/.SRCINFO.tmpl
  homebrew/password-cracking.rb.tmpl
  nfpm.yaml                    deb / rpm / apk
```

To add a channel (for example Nix or Snap):

1. Add a template under `packaging/<channel>/`.
2. Extend `scripts/gen-package-manifests.py` with the new output.
3. Add a job to `.github/workflows/publish.yml`.
4. Verify locally with `python3 scripts/gen-package-manifests.py …`.
