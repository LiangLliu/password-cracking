# Distribution Channels

`password-cracking` ships through the channels below. All of them are driven
by one tag push; the GitHub Release remains the source of truth for binaries
and checksums.

| Channel | Platform | Command | Status |
|---------|----------|---------|--------|
| [Homebrew](#homebrew) | macOS, Linux | `brew install LiangLliu/tap/password-cracking` | auto-published on tag |
| [APT](#debian--ubuntu-apt) | Debian, Ubuntu, Raspberry Pi OS | `sudo apt install password-cracking` | auto-published on tag |
| [Docker](#docker-ghcr) | Linux containers | `docker run ghcr.io/lianglliu/password-cracking` | auto-published on tag |
| [Scoop](#windows-scoop) | Windows | `scoop install password-cracking` | auto-published on tag |
| [AUR](#arch-linux-aur) | Arch Linux | `yay -S password-cracking-bin` | PKGBUILD generated; submit once |
| [crates.io](#cratesio) | any Rust platform | `cargo install password-cracking` | pipeline ready; token required |
| [install.sh](#installsh) | Linux, macOS | `curl …/install.sh \| sh` | available |
| [GitHub Releases](https://github.com/LiangLliu/password-cracking/releases) | all | direct download | available |

---

## Homebrew

```bash
brew install LiangLliu/tap/password-cracking

# upgrade
brew upgrade password-cracking
```

The formula lives in
[`LiangLliu/homebrew-tap`](https://github.com/LiangLliu/homebrew-tap) and is
updated automatically on every release. macOS uses the native builds; Linux
uses the **static musl** builds, so the formula works on any distribution.

The formula installs the binary, man page and bash/zsh/fish completions.

## Debian / Ubuntu (APT)

The APT repository is hosted on GitHub Pages and signed with the project key.

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

```
17E4 9D06 F4E3 B63C 8868  EC40 C334 9BA9 9EBE 5A92
```

Verify it with:

```bash
gpg --show-keys /etc/apt/keyrings/password-cracking.gpg
```

Supported architectures: `amd64`, `arm64`, `armhf` (Raspberry Pi 2+).

Pinning a version works as usual:

```bash
sudo apt install password-cracking=0.2.0
# downgrading an installed package also needs --allow-downgrades
```

Old versions stay in the pool, so pinning and reproducible installs keep
working. The site is ~700 KB per architecture per release; GitHub Pages limits
(1 GB) are not a concern for the foreseeable future.

## Docker (GHCR)

```bash
docker pull ghcr.io/lianglliu/password-cracking:latest

docker run --rm -v "$PWD:/data" ghcr.io/lianglliu/password-cracking \
  -f /data/document.zip dictionary -w /data/wordlist.txt
```

- Multi-arch: `linux/amd64`, `linux/arm64`
- Image is built `FROM scratch` around the static musl binary (~2 MB)
- Tags: `latest`, `0.2`, `0.2.0`
- No shell, no libc, no network access; mount the target file and wordlist

Pass `--user "$(id -u):$(id -g)"` if the wordlist is not world-readable.

## Arch Linux (AUR)

The package name is `password-cracking-bin`.

```bash
yay -S password-cracking-bin
# or
paru -S password-cracking-bin
```

The release job generates `PKGBUILD` and `PKGBUILD.SRCINFO` and attaches them
to the GitHub Release (the AUR checkout installs the latter as `.SRCINFO`). If the `AUR_SSH_PRIVATE_KEY` secret is configured, it also
pushes to `ssh://aur@aur.archlinux.org/password-cracking-bin.git`.

The AUR package must exist before the first automated push. To create it:

1. Download `PKGBUILD` and `PKGBUILD.SRCINFO` from the release, renaming the
   latter to `.SRCINFO`.
2. Submit `PKGBUILD` at <https://aur.archlinux.org/submit/>.
3. Add the public half of the CI key to your AUR account
   (**My Account → SSH Public Key**).
4. Store the private key as the `AUR_SSH_PRIVATE_KEY` secret.

Supported architectures: `x86_64`, `aarch64`.

## Windows (Scoop)

```powershell
scoop bucket add lianglliu https://github.com/LiangLliu/scoop-bucket
scoop install password-cracking
```

The manifest lives in
[`LiangLliu/scoop-bucket`](https://github.com/LiangLliu/scoop-bucket) and is
updated automatically. It includes `checkver`/`autoupdate` support, so
`scoop update` works without a new manifest commit.

## crates.io

```bash
cargo install password-cracking
# until the crate is published, install from git:
cargo install --git https://github.com/LiangLliu/password-cracking
```

Publishing happens automatically when `CARGO_REGISTRY_TOKEN` is configured and
the version is not already on crates.io. The job checks `Cargo.toml` against
the tag and skips gracefully when the version already exists. Create a token at
<https://crates.io/settings/tokens> and run `gh secret set CARGO_REGISTRY_TOKEN`
to enable it; the crate name is currently available.

## install.sh

```bash
curl -fsSL https://raw.githubusercontent.com/LiangLliu/password-cracking/master/scripts/install.sh | sh
```

Detects OS/architecture, prefers the static musl build on Linux, verifies the
SHA-256 checksum, and installs to `/usr/local` or `~/.local` without `sudo`.

---

## How publishing works

```text
git tag v0.2.0 && git push origin v0.2.0
        │
        ▼
CI (ci.yml)
  quality / audit / tests
        │
        ▼
build matrix (10 targets) ──► GitHub Release (tarballs, zip, deb/rpm/apk, checksums)
        │
        ▼
publish (publish.yml, reusable)
  ├── apt       built from .deb assets, pushed to gh-pages
  ├── homebrew  formula rendered from checksums, pushed to homebrew-tap
  ├── scoop     manifest rendered from checksums, pushed to scoop-bucket
  ├── aur       PKGBUILD/.SRCINFO rendered, optional AUR push
  ├── docker    multi-arch image pushed to ghcr.io
  └── crates    cargo publish (optional, needs token)
```

The publish workflow can be re-run manually:

```bash
gh workflow run publish.yml -f version=0.2.0
```

It downloads the assets from the GitHub Release, so re-publishing a past
release does not require rebuilding anything.

### Manifests

`scripts/gen-package-manifests.py` renders all manifests from a directory of
release assets plus their `.sha256` files:

```bash
gh release download v0.2.0 --dir dist
python3 scripts/gen-package-manifests.py --dist dist --version 0.2.0 --out manifests
```

It refuses to produce a manifest when an asset is missing and validates the
Scoop JSON before writing it. Templates live in `packaging/`.

---

## One-time setup

These steps are already done for `LiangLliu/password-cracking`; they are
documented for forks and for key rotation.

### 1. APT signing key

Generate an RSA 4096 signing key (no passphrase, because CI must use it
unattended):

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

Commit `packaging/apt/password-cracking.gpg`; it is copied to the site root as
`key.gpg`. **Rotating the key breaks existing user configurations** — announce
the new fingerprint and keep the old key available for a transition period.

### 2. GitHub Pages

The APT repository is served from the `gh-pages` branch:

```bash
gh api -X POST repos/LiangLliu/password-cracking/pages \
  --input - <<< '{"source":{"branch":"gh-pages","path":"/"}}'
```

The publish workflow enables Pages if needed and requests a build. The site is
`https://lianglliu.github.io/password-cracking/`.

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

### 4. Scoop bucket

Same pattern with `LiangLliu/scoop-bucket` and the
`SCOOP_BUCKET_DEPLOY_KEY` secret.

### 5. crates.io

```bash
cargo login <token>                       # https://crates.io/settings/tokens
gh secret set CARGO_REGISTRY_TOKEN
```

### 6. Container package visibility

GHCR packages created by `GITHUB_TOKEN` inherit the repository visibility on
first push for public repositories, but verify it once:

```bash
gh api user/packages/container/password-cracking/versions \
  --jq '.[0].metadata.container.tags'
```

If the package is private, open **Packages → password-cracking → Package
settings → Change visibility → Public**.

---

## Verification checklist

After a release:

```bash
# 1. APT (any Debian/Ubuntu container or VM)
docker run --rm -v "$PWD:/r:ro" debian:stable-slim bash -c '
  apt-get update -qq &&
  apt-get install -y -qq curl gnupg >/dev/null &&
  install -m 0755 -d /etc/apt/keyrings &&
  cp /r/key.gpg /etc/apt/keyrings/password-cracking.gpg &&
  echo "deb [signed-by=/etc/apt/keyrings/password-cracking.gpg] file:/r stable main" \
    > /etc/apt/sources.list.d/pc.list &&
  apt-get update -qq && apt-get install -y -qq password-cracking &&
  password-cracking --version'

# 2. Homebrew
brew install LiangLliu/tap/password-cracking
brew test LiangLliu/tap/password-cracking
brew uninstall password-cracking

# 3. Docker
docker run --rm ghcr.io/lianglliu/password-cracking --version

# 4. crates.io
cargo install password-cracking --version 0.2.0

# 5. Manifests
python3 scripts/gen-package-manifests.py --dist dist --version 0.2.0 --out /tmp/m
bash -n /tmp/m/PKGBUILD
ruby -c /tmp/m/password-cracking.rb
```

For the repository metadata itself:

```bash
curl -fsS https://lianglliu.github.io/password-cracking/dists/stable/InRelease \
  | gpg --verify
curl -fsS https://lianglliu.github.io/password-cracking/dists/stable/Release
```

---

## Troubleshooting

| Symptom | Cause / fix |
|---------|-------------|
| `apt update`: `NO_PUBKEY` | The `signed-by` path is wrong, or the key was rotated. Re-download `key.gpg`. |
| `apt update`: `403` | GitHub Pages build still in progress; wait a minute and retry. |
| `brew`: `No available formula` | The tap push failed. Check the `homebrew` job and the deploy key. |
| `brew`: `SHA256 mismatch` | The release asset was replaced after the formula was rendered. Re-run `publish.yml`. |
| GHCR `unauthorized` | The package is private; change its visibility (see above). |
| `cargo publish`: `already exists` | Expected; the job skips published versions. Bump `Cargo.toml` first. |
| AUR push fails | The AUR package does not exist yet, or the SSH key is not registered. The PKGBUILD is still attached to the release. |
| `nfpm`: wrong architecture on ARMv7 | Debian uses `armhf`, RPM uses `armv7hl`, Alpine uses `armv7`; the CI maps them per packager. |

---

## Adding another channel

`packaging/` is intentionally manifest-first:

```
packaging/
  apt/index.html               landing page for the APT site
  apt/password-cracking.gpg    public signing key
  aur/PKGBUILD.tmpl
  aur/.SRCINFO.tmpl
  homebrew/password-cracking.rb.tmpl
  scoop/password-cracking.json.tmpl
  nfpm.yaml                    deb / rpm / apk
```

To add a channel (WinGet, Nix, Chocolatey, Snap):

1. Add a template under `packaging/<channel>/`.
2. Extend `scripts/gen-package-manifests.py` with the new output.
3. Add a job to `.github/workflows/publish.yml`.
4. Verify locally with `python3 scripts/gen-package-manifests.py …`.

WinGet is the easiest follow-up: generate the three YAML manifests from the
existing Windows ZIPs and submit them with `wingetcreate update`.
