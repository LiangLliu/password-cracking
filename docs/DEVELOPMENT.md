# Development

**English** | [中文](DEVELOPMENT.zh-CN.md)

## Setup

You need Rust 1.89 or newer, `rustfmt` and `clippy`:

```bash
rustup component add rustfmt clippy
git clone https://github.com/LiangLliu/password-cracking
cd password-cracking
cargo build --release
cargo test
```

`./scripts/install-hooks.sh` installs an optional pre-commit hook that runs
`./scripts/fmt.sh --check --skip-tests` whenever Rust files are staged.

## Layout

```text
src/
  main.rs          entry point and exit codes
  cli/mod.rs       clap definitions and run()
  engine/mod.rs    FIFO worker pool, adaptive batches, checkpoints
  formats/         ZIP (zip/), PDF (pdf/) and Office (office.rs) verifiers
  generators/      dictionary, brute-force, mask and hybrid sources
  utils/           charsets and formatting
tests/e2e.rs       end-to-end tests over examples/
examples/          encrypted fixtures and the Python generator
docs/              usage and development guides
packaging/         Homebrew, AUR, APT and nfpm templates
scripts/           release, install, manifest and APT helpers
```

## Checks

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo audit
cargo machete
bash scripts/test-package-manifests.sh
cargo publish --dry-run --allow-dirty
```

`./scripts/fmt.sh` runs formatting, clippy and tests. Add `--check` for a
read-only pass.

## Architecture

- `PasswordSource` produces candidates via `next_shard` (compact range descriptors
  for brute-force, mask, and mmap dictionary slices) or `fill_batch` (flat
  contiguous `CandidateBatch` buffer), never materializing the full keyspace.
- `PasswordVerifier` checks candidates via `quick_check`, `verify`,
  `verify_batch`, and `verify_prefix_group`.
- A generator thread sends shards or batches through a bounded FIFO channel to a
  fixed worker pool. FIFO order lets the checkpoint store the start of the
  earliest unfinished item.
- The engine writes a checkpoint only after an item finishes. The saved value is
  the verified frontier, not the generator's current position.
- Batch size starts at 32, adjusts toward 250 ms of work, and caps at 65,536.
- `quick_check` filters ZIP ZipCrypto (12-byte header) and WinZip AES (single
  PBKDF2-HMAC-SHA1 block for the 2-byte verification value). PDF and Office use
  the default `quick_check() -> true` and perform verification directly in
  `verify_batch`, `verify_prefix_group`, and `verify`.

## Format verifiers

- ZIP (`src/formats/zip/`): reads every encrypted entry. ZipCrypto uses the
  12-byte header check and incremental prefix-group key updates; AES uses a
  single-block PBKDF2-HMAC-SHA1 filter before full entry decompression via the
  `zip` crate. A password must open all encrypted entries.
- PDF (`src/formats/pdf/`): parses the trailer, `/Encrypt` dictionary and `/ID`,
  then checks the `/U` hash (with 4-way SIMD MD5 for R2–R4 and zero-allocation
  stack buffers for R6 Algorithm 2.B). `--pdf-owner` also checks `/O`.
- Office (`src/formats/office.rs`): reads the Agile `EncryptionInfo` stream from
  the OLE2 container and runs the iterated hash check using low-level SHA
  compression blocks. It encodes candidates as UTF-16LE, so Unicode passwords
  work. The verifier rejects legacy CryptoAPI/ECMA-376 and Office 97–2003 files.

## Adding a format

1. Implement `PasswordVerifier` in `src/formats/`.
2. Add magic-byte detection and construction in `src/formats/mod.rs`.
3. Add a fixture to `examples/` and an e2e test to `tests/e2e.rs`.
4. Update the usage guide and both language versions.

```rust
impl PasswordVerifier for MyVerifier {
    fn verify(&self, password: &[u8]) -> bool { /* final check */ }
    fn format_name(&self) -> &'static str { "MyFormat" }
    fn encryption_info(&self) -> &str { "..." }
}
```

## Adding a generator

1. Implement `PasswordSource`.
2. Add a `GeneratorMode` variant and a factory arm in `src/generators/mod.rs`.
3. Add the CLI subcommand or flag in `src/cli/mod.rs`.
4. Add unit tests for ordering, bounds and checkpoint round-trips.
5. Add an e2e test.

`checkpoint()` and `restore()` use an opaque string. The engine also stores the
source name, canonical target path, and `config_signature()`, and refuses a
mismatched resume.

## Tests

Unit tests sit next to the code in `#[cfg(test)] mod tests`. End-to-end tests
live in `tests/e2e.rs` and use the fixtures in `examples/`.

Every fixture uses the password `92eo` except `test_unicode.docx`, which uses
`密码92eo`. Owner-only PDFs need `VerifyOptions { try_pdf_owner: true }` or
`--pdf-owner`.

```bash
cargo test
cargo test --test e2e
cargo test --lib generators::mask
```

## Release

1. Bump the version in `Cargo.toml` and run `cargo check` to update
   `Cargo.lock`.
2. Update both language versions of any user-facing documentation.
3. Run the checks above.
4. Commit the version bump.
5. Run `./scripts/release.sh patch`, `minor`, `major` or `vX.Y.Z`.

The tag starts eight builds: six Linux targets and two macOS targets. CI creates
the GitHub Release, then runs `publish.yml` to update the package channels.

Do not create a release for documentation-only changes. Pushing to `master`
runs the checks but no tag, so no release or package update happens.

| Channel | Status |
|---------|--------|
| Homebrew | live |
| APT repository on GitHub Pages | live, signed |
| GHCR multi-arch image | live |
| `install.sh` | live |
| GitHub Release archives and packages | live |
| AUR `password-cracking-bin` | generated, submission pending |
| crates.io | pipeline ready, token required |

Re-run publishing for an existing release without rebuilding:

```bash
gh workflow run publish.yml -f version=0.2.1
```

Publishing jobs download assets from the GitHub Release. Missing credentials
skip a channel instead of failing the release.

### Secrets

| Secret | Purpose |
|--------|---------|
| `APT_GPG_PRIVATE_KEY` | sign APT repository metadata |
| `HOMEBREW_TAP_DEPLOY_KEY` | push the formula to `homebrew-tap` |
| `AUR_SSH_PRIVATE_KEY` | push the AUR package (optional) |
| `CARGO_REGISTRY_TOKEN` | publish to crates.io (optional) |

The public APT key is at
`https://lianglliu.github.io/password-cracking/key.gpg`. Rotating it breaks
existing user configurations, so announce the new key and keep the old one
available during the transition.

## Style

- Run `cargo fmt` and `cargo clippy --all-targets -- -D warnings` before a
  commit.
- Follow Rust naming conventions and keep hot loops free of per-candidate
  allocations.
- Use `anyhow::Result` and `.context(...)` for errors the user can act on.
- Write semantic commit messages, for example `fix(pdf): parse
  /EncryptMetadata` or `docs: trim the usage guide`.
- Update the English and Chinese files in the same change.
- Prefer measured numbers and concrete commands over broad claims.

## Security

Use this project only for authorized recovery and testing. Do not commit
credentials, private keys, tokens or real user data. Report vulnerabilities in
private instead of opening a public issue with an exploit.
