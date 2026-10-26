# Project Structure

**English** | [中文](PROJECT_STRUCTURE.zh-CN.md)

## Directory layout

```text
password-cracking/
├── Cargo.toml                     Rust manifest (edition 2024, MSRV 1.88)
├── Cargo.lock                     Locked dependency graph
├── rustfmt.toml                   Formatting configuration
├── LICENSE                        MIT
├── README.md / README_CN.md       English / Chinese overview
├── Dockerfile                     scratch image around the static musl binary
│
├── src/
│   ├── main.rs                    Process entry point, exit codes 0/1/2
│   ├── lib.rs                     Public modules
│   ├── cli/mod.rs                 clap definitions and run()
│   ├── engine/mod.rs              Worker pool, adaptive batches, checkpoints
│   ├── formats/
│   │   ├── mod.rs                 PasswordVerifier trait, VerifyOptions, auto-detection
│   │   ├── zip.rs                 ZipCrypto + AES ZIP
│   │   ├── pdf.rs                 RC4/AES PDF, user + owner passwords
│   │   └── office.rs              Agile-encrypted Office (OLE2 + XML)
│   ├── generators/
│   │   ├── mod.rs                 PasswordSource trait, GeneratorMode, factory
│   │   ├── dictionary.rs          mmap streaming dictionary, optional dedup
│   │   ├── brute_force.rs         Index-based brute force
│   │   ├── mask.rs                Per-position mask source
│   │   └── rules.rs               Hybrid dictionary + mutation rules
│   └── utils/mod.rs               Charsets, formatting, validation
│
├── tests/e2e.rs                   End-to-end tests over examples/
│
├── examples/
│   ├── create_test_files.py       Fixture generator (pikepdf, msoffcrypto-tool)
│   ├── README.md / README.zh-CN.md
│   ├── test.zip                    ZipCrypto
│   ├── test_aes.zip                AES-256 ZIP (7z)
│   ├── test_multi.zip              Two encrypted entries
│   ├── test_mixed.zip              Unencrypted first entry, encrypted second
│   ├── test.pdf                    RC4-128 (R3)
│   ├── test_aes128.pdf             AES-128 (R4)
│   ├── test_r6.pdf                 AES-256 (R6, Algorithm 2.B)
│   ├── test_xref.pdf               AES-256 R6 with xref stream
│   ├── test_owner_r4.pdf           AES-128, owner password only
│   ├── test_owner_r6.pdf           AES-256, owner password only
│   ├── test.docx / .xlsx / .pptx   Agile AES-256
│   └── pyproject.toml / uv.lock    Python tooling for fixture generation
│
├── wordlists/
│   ├── common-passwords.txt       33 common passwords
│   ├── chinese-common.txt         40 common Chinese passwords
│   ├── pins-4digit.txt            10,000 four-digit PINs
│   └── pins-6digit.txt            1,000,000 six-digit PINs
│
├── docs/
│   ├── README.md / README.zh-CN.md
│   ├── USER_GUIDE.md / .zh-CN.md
│   ├── DISTRIBUTION.md / .zh-CN.md
│   ├── PERFORMANCE.md / .zh-CN.md
│   ├── PROJECT_STRUCTURE.md / .zh-CN.md
│   ├── DEVELOPMENT.md / .zh-CN.md
│   ├── CI-CD.md / .zh-CN.md
│   ├── RELEASE_LINUX.md / .zh-CN.md
│   ├── ZIP_IMPLEMENTATION.md / .zh-CN.md
│   ├── PDF_IMPLEMENTATION.md / .zh-CN.md
│   ├── OFFICE_IMPLEMENTATION.md / .zh-CN.md
│   └── superpowers/               Historical design records (not maintained)
│
├── packaging/
│   ├── nfpm.yaml                  deb / rpm / apk configuration
│   ├── apt/index.html             APT landing page
│   ├── apt/password-cracking.gpg  Public signing key
│   ├── aur/PKGBUILD.tmpl
│   ├── aur/.SRCINFO.tmpl
│   └── homebrew/password-cracking.rb.tmpl
│
├── scripts/
│   ├── fmt.sh                     Format + clippy (+ tests)
│   ├── release.sh                 Version bump + tag
│   ├── install.sh                 Static binary installer
│   ├── gen-package-manifests.py   Homebrew/AUR manifest renderer
│   ├── build-apt-repo.sh          Signed APT repository builder
│   ├── test-package-manifests.sh  Manifest smoke test
│   ├── check-large-files.sh
│   └── install-hooks.sh
│
└── .github/workflows/
    ├── ci.yml                     Quality, tests, 8-target build, release
    ├── publish.yml                APT/Homebrew/AUR/GHCR/crates.io
    └── release-drafter.yml
```

## Core abstractions

### `PasswordVerifier` (`src/formats/mod.rs`)

```rust
pub trait PasswordVerifier: Send + Sync {
    /// Cheap filter. Return true if the password may be correct.
    fn quick_check(&self, password: &[u8]) -> bool;
    /// Authoritative check.
    fn verify(&self, password: &[u8]) -> bool;
    fn format_name(&self) -> &'static str;
    fn encryption_info(&self) -> &str;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VerifyOptions {
    pub try_pdf_owner: bool,
}
```

`quick_check` is a real filter only for ZIP ZipCrypto (12-byte header). PDF and
Office return `true` so the engine calls `verify` once. `VerifyOptions` carries
CLI-level policy into the verifier constructor.

### `PasswordSource` (`src/generators/mod.rs`)

```rust
pub trait PasswordSource: Send {
    /// Fill up to `max` candidates. Returns false when exhausted.
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>, max: usize) -> bool;
    fn estimated_total(&self) -> Option<u64>;
    /// Opaque position; None if the source is not resumable.
    fn checkpoint(&self) -> Option<String>;
    fn restore(&mut self, state: &str) -> anyhow::Result<()>;
    fn name(&self) -> &'static str;
}
```

Sources never materialize the full keyspace. The dictionary source reads
memory-mapped files by byte offset; brute force and mask compute candidates
from indices; hybrid keeps a bounded pending queue.

### `CrackerEngine` (`src/engine/mod.rs`)

```text
fill_batch(max) ──► bounded FIFO channel ──► worker threads
                                              │
                        quick_check() ──► verify()
                                              │
                              mark batch completed
                                              │
                              verified checkpoint frontier
```

- Persistent FIFO workers keep completion roughly ordered, which is required
  for a meaningful checkpoint frontier.
- The batch target starts at 32, adapts toward ~250 ms per batch, and caps at
  4096.
- The channel capacity equals the worker count, so the generator cannot run far
  ahead.
- A batch tracker records the source position at each batch's start. The
  checkpoint is the start of the earliest incomplete batch.
- A progress thread updates the bar and writes checkpoints every interval.

## Format detection

| Magic bytes | Format | Notes |
|-------------|--------|-------|
| `PK\x03\x04` | ZIP | Also OOXML containers |
| `%PDF-` | PDF | |
| `D0 CF 11 E0 A1 B1 1A E1` | Office (OLE2) | Agile-encrypted OOXML is wrapped in OLE2 |
| `PK\x03\x04` + OOXML part names | Office (unencrypted OOXML) | Not encrypted; rejected as unsupported |

Encrypted Office files are OLE2 compound files; the extension distinguishes
`.docx/.xlsx/.pptx` from `.doc/.xls/.ppt`, but both use the same `EncryptionInfo`
/ `EncryptedPackage` streams for Agile encryption.

## Data flow

1. `cli::run()` validates the target and builds a `GeneratorMode`.
2. `create_source()` returns a concrete `PasswordSource`.
3. `create_verifier_with_options()` sniffs the file and returns a
   `PasswordVerifier`.
4. `CrackerEngine::crack()` spawns the generator, workers and progress thread.
5. On success, the worker stores the password, sets the stop flag and finishes
   the progress bar.
6. The CLI prints the result and exits 0 (found), 1 (not found) or 2 (error).

## Dependencies

| Category | Crates |
|----------|--------|
| CLI | `clap` 4.6 (derive, cargo), `clap_complete` 4.6, `clap_mangen` 0.3 |
| Concurrency | `crossbeam-channel` 0.5, `num_cpus` 1.17 |
| Progress | `indicatif` 0.18 |
| Errors | `anyhow` 1.0 |
| ZIP | `zip` 9.1 (`aes-crypto`, `deflate`) |
| Hashes | `sha2` 0.11, `sha1` 0.11, `md-5` 0.11 |
| Ciphers | `aes` 0.9, `cipher` 0.5 |
| OLE2 | `cfb` 0.15 |
| Wordlists | `memmap2` 0.9 |
| Utils | `base64` 0.23 |
| Dev | `tempfile` 3.27 |

All dependencies are kept at the latest compatible versions; Dependabot tracks
updates weekly.

## Extension points

- **New format**: implement `PasswordVerifier`, add detection and construction
  in `src/formats/mod.rs`, add a fixture and an e2e test.
- **New generator**: implement `PasswordSource`, add a `GeneratorMode` variant,
  a CLI subcommand and unit/e2e tests.
- **New package channel**: add a template under `packaging/`, extend
  `scripts/gen-package-manifests.py`, add a job to `publish.yml`.

See [DEVELOPMENT.md](DEVELOPMENT.md) for the full workflow.
