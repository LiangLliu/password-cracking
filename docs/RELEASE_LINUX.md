# Release Engineering

**English** | [中文](RELEASE_LINUX.zh-CN.md)

How Linux and macOS artifacts are built, packaged and published. Distribution
channels and signing-key setup live in [DISTRIBUTION.md](DISTRIBUTION.md).

## Build matrix

A `v*` tag (or a manual `workflow_dispatch` run) builds **8 targets**:

| Artifact | Target triple | Notes |
|----------|---------------|-------|
| `password-cracking-linux-amd64.tar.gz` | `x86_64-unknown-linux-gnu` | glibc 2.39+ |
| `password-cracking-linux-amd64-musl.tar.gz` | `x86_64-unknown-linux-musl` | static, recommended |
| `password-cracking-linux-arm64.tar.gz` | `aarch64-unknown-linux-gnu` | cross |
| `password-cracking-linux-arm64-musl.tar.gz` | `aarch64-unknown-linux-musl` | cross, static |
| `password-cracking-linux-armv7.tar.gz` | `armv7-unknown-linux-gnueabihf` | Raspberry Pi 2+ |
| `password-cracking-linux-armv7-musl.tar.gz` | `armv7-unknown-linux-musleabihf` | static |
| `password-cracking-macos-amd64.tar.gz` | `x86_64-apple-darwin` | Intel Mac |
| `password-cracking-macos-arm64.tar.gz` | `aarch64-apple-darwin` | Apple Silicon |

Every archive contains a flat `password-cracking` binary plus `README.md` and
`LICENSE`. Archives are built on the native runner (macOS) or with `cross`
(Linux cross targets).

## Native packages

Built with [nfpm](https://nfpm.goreleaser.com/) from the **musl** binaries:

| Format | Architectures | Notes |
|--------|---------------|-------|
| `.deb` | `amd64`, `arm64`, `armhf` | Debian, Ubuntu, Raspberry Pi OS |
| `.rpm` | `x86_64`, `aarch64`, `armv7hl` | Fedora, RHEL, openSUSE |
| `.apk` | `x86_64`, `aarch64`, `armv7` | Alpine |

ARMv7 uses a different name in each format; CI maps them per packager:

```text
deb: armv7 -> armhf
rpm: armv7 -> armv7hl
apk: armv7 -> armv7
```

Packages include the binary, man page, bash/zsh/fish completions and the
license. There are no maintainer scripts; the binary is static for musl builds.

## Why musl is the primary Linux binary

- The `x86_64-unknown-linux-gnu` binary is linked against the runner's glibc.
  Older distributions can fail with `GLIBC_2.xx not found`.
- The musl build is fully static (`file` reports "statically linked") and runs
  on any x86_64/aarch64/armv7 Linux with a compatible kernel.
- Distro packages use the musl binary, so a `.deb` installed on Debian 12 and a
  `.rpm` installed on Fedora behave identically.
- The static binary is ~1.1 MB stripped; the scratch Docker image is ~1.7 MB.

GNU tarballs are still published for users who prefer a dynamically linked
binary and for the AUR package (Arch ships a recent glibc).

## Release pipeline

```text
tag v0.2.1
  │
  ├── quality-check (fmt, clippy, manifest templates)
  ├── security-audit (RustSec)
  ├── test (Ubuntu stable/beta, macOS stable)
  │
  ├── build × 8 ──► tarballs + deb/rpm/apk + per-file .sha256
  │
  ├── GitHub Release ──► 39 assets + SHA256SUMS + install.sh
  │
  └── publish.yml
        ├── apt       signed repository on gh-pages
        ├── homebrew  formula in homebrew-tap
        ├── aur       PKGBUILD + PKGBUILD.SRCINFO (optional push)
        ├── docker    linux/amd64 + linux/arm64 on GHCR
        └── crates    cargo publish (optional)
```

The publish job is a reusable workflow (`workflow_call`) and can be run
manually with `gh workflow run publish.yml -f version=0.2.1`.

## Verification

Performed for v0.2.1:

1. **Checksums**: all release assets pass `sha256sum -c SHA256SUMS`.
2. **APT**: Debian container, signature verified with the published key,
   `apt install password-cracking` → `0.2.1`.
3. **Homebrew**: `brew install`, `brew test`, completions and man page, real
   fixture crack on macOS arm64.
4. **Docker**: `docker run … --version` for `0.2.1` and `latest`; multi-arch
   manifest lists `linux/amd64` and `linux/arm64`.
5. **Installer**: `install.sh --prefix …` verifies SHA-256 and installs 0.2.1.
6. **AUR**: `PKGBUILD` passes `bash -n`; checksums match the release tarballs.
7. **Manifests**: Homebrew and AUR hashes match the published assets.

Automated container tests are not yet part of CI; see
[Remaining work](#remaining-work).

## Release checklist

1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test` (25 unit + 19 e2e at the time of writing)
4. `cargo audit && cargo machete`
5. `bash scripts/test-package-manifests.sh`
6. `cargo publish --dry-run --allow-dirty`
7. Update both English and Chinese docs for user-visible changes.
8. Bump `Cargo.toml`; run `cargo check` to update `Cargo.lock`.
9. Commit and run `./scripts/release.sh patch|minor|major|vX.Y.Z`.
10. Watch the tag CI run; verify 8 build jobs, the release assets and the
    publish jobs.
11. Run the verification steps above.
12. Announce the release.

## Remaining work

| Priority | Task | Notes |
|----------|------|-------|
| P0 | Container smoke tests in CI | install `.deb`/`.rpm`/`.apk` on every release |
| P1 | Artifact signing / provenance | `actions/attest-build-provenance` or Sigstore |
| P1 | APT verification job | run the signed-repo install in CI |
| P2 | AUR submission | `PKGBUILD` is generated; submit once, then set `AUR_SSH_PRIVATE_KEY` |
| P2 | crates.io token | set `CARGO_REGISTRY_TOKEN` |
| P3 | Reproducible builds | pin the toolchain, `SOURCE_DATE_EPOCH` |
| P3 | Reduce binary size | keep `opt-level=3`; static musl binary is ~1.1 MB |

Completed: 8-target matrix, musl packages, Homebrew tap, APT repository on
GitHub Pages, GHCR multi-arch image, crates.io metadata and publish job,
per-format ARMv7 architecture mapping, bilingual documentation.

## Notes and risks

- `cross` needs Docker on the runner; GitHub-hosted Ubuntu runners provide it.
  The version is pinned by `taiki-e/install-action`.
- armv7 builds assume hardware floating point (Raspberry Pi 2+). Raspberry Pi
  Zero/1 (armv6) is not covered.
- GNU Linux binaries require glibc >= the runner's glibc (currently 2.39). The
  musl builds avoid this.
- The nfpm version is passed without the `v` prefix; the workflow strips it.
- APT metadata is signed (`InRelease`, `Release.gpg`). Individual `.deb`/`.rpm`
  files are not signed; repository metadata is the trust anchor.
- The GitHub Release is the only source of binaries. Publishing jobs download
  from it, so re-running a publish never rebuilds.
- GitHub Pages serves the APT repository; a build usually completes in under a
  minute.

## Alternative: cargo-dist

`cargo-dist` can generate archives, a shell installer, a Homebrew formula and
provenance from `[workspace.metadata.dist]`.

- **Pros**: less bespoke YAML, consistent installers.
- **Cons**: no native deb/rpm/apk support (nfpm is still needed), and it
  rewrites the release workflow.

Recommendation: keep the current workflow and nfpm. Revisit `cargo-dist` only
if the release workflow needs to be simplified or a Homebrew-core formula is
required.
