# CI/CD

**English** | [中文](CI-CD.zh-CN.md)

The project uses three GitHub Actions workflows. All release work is driven by a
single tag push.

## Workflows

### 1. `ci.yml` — CI and release

Triggers:

- push to `main`, `master` or `develop`
- tags matching `v*.*.*`
- pull requests targeting those branches
- manual `workflow_dispatch` (build jobs can be tested without a tag)

Jobs:

| Job | Runs on | What it does |
|-----|---------|--------------|
| `quality-check` | every trigger | `cargo fmt --check`, `cargo clippy -D warnings`, manifest-template smoke test |
| `security-audit` | every trigger | `rustsec/audit-check` |
| `test` | every trigger | Ubuntu stable + beta, macOS stable |
| `build` | tags and manual runs | 8-target matrix (6 Linux + 2 macOS) |
| `release` | tags only | creates the GitHub Release and uploads assets |
| `publish` | tags only | calls `publish.yml` after the release |

Build matrix:

| Platform | Target | Artifact | Packages |
|----------|--------|----------|----------|
| Linux | `x86_64-unknown-linux-gnu` | `linux-amd64` | |
| Linux | `x86_64-unknown-linux-musl` | `linux-amd64-musl` | deb/rpm/apk |
| Linux | `aarch64-unknown-linux-gnu` | `linux-arm64` | |
| Linux | `aarch64-unknown-linux-musl` | `linux-arm64-musl` | deb/rpm/apk |
| Linux | `armv7-unknown-linux-gnueabihf` | `linux-armv7` | |
| Linux | `armv7-unknown-linux-musleabihf` | `linux-armv7-musl` | deb(armhf)/rpm(armv7hl)/apk(armv7) |
| macOS | `x86_64-apple-darwin` | `macos-amd64` | |
| macOS | `aarch64-apple-darwin` | `macos-arm64` | |

Packages are built only from musl binaries, so the three formats never collide
on the same architecture name.

### 2. `publish.yml` — distribution

Reusable workflow called by `ci.yml`, or run manually:

```bash
gh workflow run publish.yml -f version=0.2.1
```

| Job | Target |
|-----|--------|
| `prepare` | normalize `v0.2.1` → `0.2.1` |
| `apt` | rebuild the signed APT repository and push `gh-pages` |
| `homebrew` | render the formula and push `homebrew-tap` |
| `aur` | render `PKGBUILD` / `PKGBUILD.SRCINFO`; push if configured |
| `docker` | build `linux/amd64` + `linux/arm64` and push GHCR |
| `crates` | `cargo publish` when a token is configured |

All jobs download assets from the GitHub Release, so re-running never rebuilds
binaries. Missing credentials produce a notice, not a failure.

### 3. `release-drafter.yml`

Updates a draft release with merged PR titles and labels. It does not publish
anything.

## Release process

```bash
# Recommended: bump Cargo.toml, commit, tag and push
./scripts/release.sh patch     # 0.2.1 -> 0.2.1
./scripts/release.sh minor     # 0.2.1 -> 0.3.0
./scripts/release.sh major     # 0.3.0 -> 1.0.0
./scripts/release.sh v0.4.0    # explicit version
```

Manual equivalent:

```bash
# Update Cargo.toml, run cargo check, commit
git tag -a v0.2.1 -m "Release v0.2.1"
git push origin v0.2.1
```

The tag triggers `ci.yml` → `build` → `release` → `publish`. Watch it with:

```bash
gh run list --workflow=ci.yml --limit 5
gh run watch
```

Re-publish an existing release without rebuilding:

```bash
gh workflow run publish.yml -f version=0.2.1
```

## Secrets

| Secret | Required | Purpose |
|--------|----------|---------|
| `GITHUB_TOKEN` | automatic | release assets, `gh-pages`, GHCR |
| `APT_GPG_PRIVATE_KEY` | for signing | signs `InRelease` and `Release.gpg` |
| `HOMEBREW_TAP_DEPLOY_KEY` | for Homebrew | write access to `homebrew-tap` |
| `AUR_SSH_PRIVATE_KEY` | optional | push `password-cracking-bin` to AUR |
| `CARGO_REGISTRY_TOKEN` | optional | publish to crates.io |

`GITHUB_TOKEN` permissions used by `publish.yml`:

```yaml
contents: write   # release assets and gh-pages
packages: write   # GHCR
pages: write      # enable/refresh Pages
```

## Caching and performance

- Cargo registry and `target/` are cached per OS/target keyed by `Cargo.lock`.
- `cross` images are pulled on demand for the Linux cross targets.
- The full matrix takes roughly 5–8 minutes on GitHub-hosted runners.
- The musl jobs are the slowest because they run inside the cross container.

## Dependabot

Dependabot checks Cargo and GitHub Actions weekly and opens PRs. Apply updates
on `master` in one commit when several are pending, then close the PRs with a
reference to the commit.

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `Quality Check` fails on formatting | `cargo fmt --all` |
| `Quality Check` fails on clippy | `cargo clippy --all-targets -- -D warnings` |
| Manifest smoke test fails | Inspect `scripts/gen-package-manifests.py` output |
| Cross build fails | Check the `cross` image for the target; rerun the job |
| Release job cannot find assets | Ensure every build job uploaded artifacts |
| Publish job cannot download the release | The release must exist and be non-draft |
| APT job fails to push | Check the `pages: write` permission and branch protection |
| Homebrew job fails to clone | Check `HOMEBREW_TAP_DEPLOY_KEY` and the deploy key |
| GHCR push unauthorized | Check `packages: write` and repository settings |

## Badges

```markdown
[![CI](https://github.com/LiangLliu/password-cracking/actions/workflows/ci.yml/badge.svg)](https://github.com/LiangLliu/password-cracking/actions/workflows/ci.yml)
```

## Notes

- Windows is not built or tested. The workflow intentionally has no
  `windows-latest` runner.
- Release jobs are tag-gated; branch pushes only run quality, audit and tests.
- `workflow_dispatch` can run the build matrix without creating a release, which
  is useful for validating packaging changes.
