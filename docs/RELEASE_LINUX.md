# Linux/Unix Release Plan

Status: **build matrix, packages, completions, man page and installer are
implemented**; remaining items are listed at the end.

## 1. Current pipeline (implemented)

On a `v*` tag (or a manual `workflow_dispatch` run), CI builds and uploads:

| Artifact | Target triple | Notes |
|----------|---------------|-------|
| `password-cracking-linux-amd64.tar.gz` | `x86_64-unknown-linux-gnu` | glibc 2.17+ |
| `password-cracking-linux-amd64-musl.tar.gz` | `x86_64-unknown-linux-musl` | **static, recommended** |
| `password-cracking-linux-arm64.tar.gz` | `aarch64-unknown-linux-gnu` | cross |
| `password-cracking-linux-arm64-musl.tar.gz` | `aarch64-unknown-linux-musl` | cross, static |
| `password-cracking-linux-armv7.tar.gz` | `armv7-unknown-linux-gnueabihf` | Raspberry Pi 32-bit |
| `password-cracking-macos-{amd64,arm64}.tar.gz` | Apple targets | |
| `password-cracking-windows-{amd64,i686}.zip` | MSVC targets | |
| `password-cracking_<ver>_{amd64,arm64}.deb` | built from the gnu/musl binary | Debian/Ubuntu |
| `password-cracking-<ver>-1.<arch>.rpm` | | Fedora/RHEL/openSUSE |
| `password-cracking_<ver>_<arch>.apk` | | Alpine |

Every archive/package has a `.sha256`; the release also ships an aggregated
`SHA256SUMS` and `install.sh`.

Also included in packages:

- man page: `/usr/share/man/man1/password-cracking.1`
- bash completion: `/usr/share/bash-completion/completions/password-cracking`
- zsh completion: `/usr/share/zsh/site-functions/_password-cracking`
- fish completion: `/usr/share/fish/vendor_completions.d/password-cracking.fish`

Generate them ad hoc:

```bash
password-cracking man > password-cracking.1
password-cracking completions bash > password-cracking.bash
password-cracking completions zsh  > _password-cracking
password-cracking completions fish > password-cracking.fish
```

## 2. Installation paths for users

```bash
# Static binary, no package manager (works on any distro)
curl -fsSL https://raw.githubusercontent.com/LiangLliu/password-cracking/master/scripts/install.sh | sh

# Debian / Ubuntu
sudo dpkg -i password-cracking_<ver>_amd64.deb

# Fedora / RHEL / openSUSE
sudo rpm -i password-cracking-<ver>-1.x86_64.rpm

# Alpine
sudo apk add --allow-untrusted password-cracking_<ver>_x86_64.apk

# From source
cargo install --path .        # or: cargo install password-cracking (after publishing)
```

## 3. Why musl is the primary Linux artifact

- The `x86_64-unknown-linux-gnu` binary is linked against the runner's glibc.
  Users on older distributions (CentOS 7, Debian 10) may see
  `GLIBC_2.xx not found`.
- The musl build is fully static (`ldd` reports "not a dynamic executable") and
  runs on any x86_64/aarch64 Linux.
- `.deb`/`.rpm` packages can use the musl binary too; the plan is to publish
  both (glibc for distro purity, musl for portability). The installer prefers
  musl on Linux.

## 4. Verification strategy

Add these checks to the release job (partially implemented):

1. **Smoke test** each native artifact: run `password-cracking --version` and
   `password-cracking man | head`.
2. **Container test** for Linux packages (matrix):
   ```yaml
   strategy:
     matrix:
       image: [debian:stable-slim, ubuntu:24.04, alpine:3.20, fedora:latest]
   steps:
     - run: |
         docker run --rm -v "$PWD/release:/r" ${{ matrix.image }} sh -c \
           'ls /r && (/r/password-cracking --version || true)'
   ```
   For deb/rpm/apk, install into the container and run `--version`.
3. **Cross-arch execution** via `qemu-user-static`:
   ```bash
   docker run --rm --privileged multiarch/qemu-user-static --reset -p yes
   docker run --rm -v "$PWD/release:/r" arm64v8/alpine:3.20 \
     /r/password-cracking-linux-arm64-musl --version
   ```
4. **Installer test**: run `install.sh --prefix /tmp/prefix` against the built
   artifacts (serve them locally or use the draft release URL).

## 5. Release checklist

1. `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
2. `cargo test && cargo audit && cargo machete`
3. Update `CHANGELOG.md` / release notes.
4. Bump `Cargo.toml` version and run `cargo check` (updates `Cargo.lock`).
5. Commit, `./scripts/release.sh patch|minor|major`.
6. Watch the CI build job; verify every artifact + package is attached.
7. Download and verify `SHA256SUMS` (`sha256sum -c SHA256SUMS`).
8. Test `install.sh` on a clean macOS/Linux machine or container.
9. Install the `.deb`/`.rpm`/`.apk` in the matching container.
10. Announce (release notes, Homebrew/AUR if set up).

## 6. Remaining work (prioritized)

| Priority | Task | Notes |
|----------|------|-------|
| P0 | Container smoke tests in CI | catches glibc/musl and packaging mistakes |
| P0 | Verify `workflow_dispatch` build path | manually trigger once before the next tag |
| P1 | Publish to crates.io | add `repository`, `readme`, `keywords`, `categories` to `Cargo.toml`; then `cargo install password-cracking` |
| P1 | Sign artifacts | `actions/attest-build-provenance` (SLSA) and/or Sigstore `cosign` |
| P2 | Homebrew tap | generate a formula pointing at the release tarballs (or adopt `cargo-dist`) |
| P2 | AUR `PKGBUILD` | Arch users |
| P2 | `aarch64-unknown-linux-gnu` package | build `.deb`/`.rpm` from the arm64 binary in an arm runner (or use cross + nfpm arch=arm64) |
| P3 | Reproducible builds | pin toolchain, `SOURCE_DATE_EPOCH`, verify byte-identical artifacts |
| P3 | Reduce binary size | `opt-level="z"` variant? Keep `3` for cracking speed; musl binary is ~1.1 MB stripped |
| P3 | Static GCC runtime for gnu builds | `-C target-feature=+crt-static` (musl is simpler) |

## 7. Alternative: `cargo-dist`

`cargo-dist` can generate most of the release workflow (archives, shell
installer, Homebrew formula, npm/pip wrappers, provenance) from
`[workspace.metadata.dist]`. Trade-offs:

- **Pro**: less bespoke YAML, consistent installers, easy Homebrew.
- **Con**: no native deb/rpm/apk support (would still need nfpm), and it
  rewrites `.github/workflows/release.yml`, which conflicts with the current
  combined CI file.

Recommendation: keep the current workflow + nfpm for distro packages, and
optionally add `cargo-dist` later only for the Homebrew formula. Do **not**
migrate wholesale unless the release workflow is simplified first.

## 8. Notes and risks

- `cross` requires Docker on the runner; GitHub-hosted Ubuntu runners provide
  it. Pin the `cross` version (`taiki-e/install-action` already does).
- armv7 support assumes the target has hardware floating point (Raspberry Pi
  2+). Raspberry Pi Zero/1 (armv6) is not covered; add
  `arm-unknown-linux-gnueabihf` + `arm-unknown-linux-musleabihf` if needed.
- The gnu Linux binaries require glibc >= the runner's (currently Ubuntu
  latest, glibc 2.39). The musl build avoids this entirely.
- `version` in nfpm is passed without the `v` prefix; the workflow strips it.
- `.deb` post-install scripts are not needed (single static binary).
- Package signing (dpkg-sig/rpm signing) is not configured; add a GPG key
  secret to the release job if distributing through repositories.
