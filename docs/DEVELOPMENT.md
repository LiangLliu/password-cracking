# Development Guide

**English** | [中文](DEVELOPMENT.zh-CN.md)

How to build, test, extend and release `password-cracking`.

## Requirements

- Rust **1.88+** (edition 2024, let-chains)
- `rustfmt` and `clippy` (`rustup component add rustfmt clippy`)
- Git
- Optional: `cargo-audit`, `cargo-machete`, Docker (cross builds and packaging)

## Setup

```bash
git clone https://github.com/LiangLliu/password-cracking
cd password-cracking
./scripts/install-hooks.sh   # optional pre-commit hook
cargo build --release
cargo test
```

The pre-commit hook runs formatting, clippy, tests, `cargo-audit` and
`cargo-machete` on staged Rust changes. It uses `git stash --keep-index`; if a
stash/pop conflict occurs, resolve it and run the checks manually. For
documentation-only commits, `git commit --no-verify` is acceptable.

## Repository layout

```
src/
  main.rs          process entry point, exit codes
  lib.rs           public modules
  cli/mod.rs       clap definitions and run()
  engine/mod.rs    worker pool, adaptive batches, checkpoints
  formats/         PasswordVerifier: mod.rs, zip.rs, pdf.rs, office.rs
  generators/      PasswordSource: dictionary, brute_force, mask, rules
  utils/mod.rs     charsets, formatting, validation
tests/e2e.rs       end-to-end tests over examples/
examples/          encrypted fixtures (password: 92eo) + generator
docs/              bilingual documentation
packaging/         Homebrew, AUR, APT, nfpm templates
scripts/           release, install, manifest and APT repo scripts
```

See [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) for the detailed module map.

## Commands

```bash
cargo build --release                 # optimized binary
cargo test                            # unit + e2e tests
cargo test --lib dictionary           # filter by name
cargo test --test e2e                 # e2e only
cargo fmt --all -- --check            # formatting
cargo clippy --all-targets -- -D warnings
cargo audit                           # RustSec advisories
cargo machete                         # unused dependencies
cargo publish --dry-run --allow-dirty # crate package verification
bash scripts/test-package-manifests.sh
actionlint .github/workflows/*.yml    # if installed
```

`./scripts/fmt.sh` runs formatting + clippy (+ tests unless `--skip-tests`);
`./scripts/fmt.sh --check` is a read-only check.

## Architecture invariants

These are not style preferences; changing them can break correctness or
performance.

1. **FIFO batch ordering.** Workers pull from a bounded FIFO channel. The
   checkpoint frontier is the start of the earliest unfinished batch. A LIFO
   scheduler (for example Rayon's work-stealing queue) would leave early
   batches pending while later ones complete, making the frontier useless.
2. **Verified frontier, not generated frontier.** The generator records the
   source position at the start of every batch. Only fully verified batches
   advance the checkpoint. Never write the generator's current position: it may
   be far ahead of what workers have checked.
3. **Adaptive batch size.** The shared target starts at 32 and doubles/halves
   based on batch duration, aiming for ~250 ms and capping at 4096. Slow
   verifiers (Office) need small batches for timely checkpoints; fast ones
   (ZIP) benefit from larger batches.
4. **Streaming sources.** `PasswordSource::fill_batch(batch, max)` may hold only
   the current batch. Do not collect the whole keyspace.
5. **Two-phase verification only when it pays.** `quick_check` is a real filter
   only for ZIP ZipCrypto. PDF and Office return `true` so the engine runs
   `verify` exactly once.

## Adding a format

1. Implement `PasswordVerifier` in `src/formats/`:

```rust
impl PasswordVerifier for MyVerifier {
    fn quick_check(&self, password: &[u8]) -> bool { /* cheap filter or true */ }
    fn verify(&self, password: &[u8]) -> bool { /* authoritative check */ }
    fn format_name(&self) -> &'static str { "MyFormat" }
    fn encryption_info(&self) -> &str { "..." }
}
```

2. Add magic-byte detection and the constructor to `src/formats/mod.rs`.
3. Add a fixture to `examples/` (password `92eo` unless there is a reason).
4. Add an e2e test in `tests/e2e.rs`.
5. Update `README.md`, `USER_GUIDE.md` and their Chinese versions.

## Adding a generator

1. Implement `PasswordSource`:

```rust
impl PasswordSource for MySource {
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>, max: usize) -> bool;
    fn estimated_total(&self) -> Option<u64>;
    fn checkpoint(&self) -> Option<String>;       // None if not resumable
    fn restore(&mut self, state: &str) -> Result<()>;
    fn name(&self) -> &'static str;
}
```

2. Add a variant to `GeneratorMode` and the factory in `src/generators/mod.rs`.
3. Add a CLI subcommand or flag in `src/cli/mod.rs`.
4. Add unit tests for ordering, bounds, checkpoint round-trips and edge cases.
5. Add an e2e test with a fixture.

Checkpoint format is free-form as long as `checkpoint()` and `restore()` agree.
The engine stores `source_name`, the state and the canonical target path.

## Testing

- **Unit tests** live next to the code in `#[cfg(test)] mod tests`.
- **End-to-end tests** live in `tests/e2e.rs` and drive the public API over the
  fixtures in `examples/`.
- Every fixture uses the password **`92eo`** except PDF owner-only fixtures,
  which require `VerifyOptions { try_pdf_owner: true }` (CLI: `--pdf-owner`).
- Test helper: `wordlist_with_password()` writes a temporary wordlist.
- Use `tempfile` for any on-disk test input.

```bash
cargo test
cargo test --test e2e
cargo test --lib generators::mask
```

## Code style

- Run `cargo fmt` and `cargo clippy --all-targets -- -D warnings` before
  committing.
- Naming follows Rust conventions: `snake_case` functions/modules,
  `PascalCase` types, `SCREAMING_SNAKE_CASE` constants.
- Prefer `anyhow::Result` in application code and `.context(...)` for
  user-facing errors.
- Keep public items documented with `///` where the behaviour is not obvious.
- Do not swallow errors silently. `let _ =` is only acceptable for cleanup
  operations where failure is harmless.
- Keep hot loops allocation-free. Reuse buffers; avoid per-candidate `Vec`
  allocations unless the API requires it.

## Git workflow

Use semantic commit messages:

```
feat(formats): support AES-256 R6 PDFs
fix(engine): keep the checkpoint frontier on FIFO order
docs: rewrite the bilingual user guide
```

Common types: `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build`,
`ci`, `chore`.

Before a PR:

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] new behaviour has tests
- [ ] English and Chinese docs are both updated
- [ ] `cargo audit` / `cargo machete` are clean

## Release process

1. Update the version in `Cargo.toml` (and `Cargo.lock` via `cargo check`).
2. Update `CHANGELOG`-style notes in the release draft or commit message.
3. Run the full local check suite.
4. Commit the version bump.
5. `./scripts/release.sh patch|minor|major|vX.Y.Z` — creates and pushes the tag.
6. CI builds 8 targets, creates the GitHub Release, then runs `publish.yml`.
7. Verify:
   - `SHA256SUMS` covers every artifact;
   - Homebrew, APT and Docker install the new version;
   - the AUR artifacts and manifests are attached.
8. Announce the release.

The publish workflow can be re-run without rebuilding:

```bash
gh workflow run publish.yml -f version=0.2.1
```

## Documentation policy

- Every maintained document has an English `NAME.md` and a Chinese
  `NAME.zh-CN.md`.
- Both versions must be updated in the same change. Do not leave one language
  describing an older CLI or a removed platform.
- The README pair (`README.md`, `README_CN.md`) follows the same rule.
- `docs/superpowers/` is an archive of earlier design work and is intentionally
  not translated or kept up to date.
- Prefer concrete commands and measured numbers over marketing language.

## Security

- This project is for authorized password recovery and testing only.
- Never commit credentials, private keys, tokens or real user data.
- CI secrets used by publishing: `APT_GPG_PRIVATE_KEY`,
  `HOMEBREW_TAP_DEPLOY_KEY`, optional `AUR_SSH_PRIVATE_KEY`,
  `CARGO_REGISTRY_TOKEN`.
- Report vulnerabilities privately; do not open a public issue with an exploit.
