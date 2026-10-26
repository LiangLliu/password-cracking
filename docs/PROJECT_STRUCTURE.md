# Project Structure

## Directory Layout

```
password-cracking/
├── Cargo.toml                     # Rust dependencies (edition 2024)
├── Cargo.lock                     # Locked dependency versions
├── LICENSE                        # MIT License
├── README.md                      # English documentation
├── README_CN.md                   # Chinese documentation
├── rustfmt.toml                   # Rust formatting config
│
├── src/                           # Source code
│   ├── main.rs                    # Binary entry point
│   ├── lib.rs                     # Library exports
│   │
│   ├── cli/                       # Command-line interface
│   │   └── mod.rs                 # clap v4 argument parsing, run()
│   │
│   ├── engine/                    # Cracking engine
│   │   └── mod.rs                 # CrackerEngine: FIFO worker pool, adaptive batches,
│   │                              # verified-frontier checkpoints
│   │
│   ├── formats/                   # File format handlers
│   │   ├── mod.rs                 # PasswordVerifier trait + auto-detection
│   │   ├── zip.rs                 # ZIP: ZipCrypto + AES
│   │   ├── pdf.rs                 # PDF: RC4-40/128, AES-128/256
│   │   └── office.rs              # Office: Agile AES-256
│   │
│   ├── generators/                # Password generators
│   │   ├── mod.rs                 # PasswordSource trait + factory
│   │   ├── dictionary.rs          # mmap streaming dictionary (+ optional dedup)
│   │   ├── brute_force.rs         # Brute-force source (index-based)
│   │   ├── mask.rs                # Mask source (per-position charsets)
│   │   └── rules.rs               # Rule-based hybrid source
│   │
│   └── utils/                     # Utilities
│       └── mod.rs                 # Character sets, formatting, validation
│
├── packaging/                     # Native package definitions
│   ├── nfpm.yaml                  # .deb/.rpm/.apk packaging config
│   ├── apt/                       # APT landing page + public signing key
│   ├── aur/                       # PKGBUILD / .SRCINFO templates
│   └── homebrew/                  # Homebrew formula template
│
├── examples/                      # Test files and scripts
│   ├── create_test_files.py       # Generate encrypted test files (password: 92eo)
│   ├── pyproject.toml             # UV Python project config
│   ├── test.zip                   # Sample encrypted ZIP
│   ├── test.pdf                   # Sample encrypted PDF
│   ├── test.docx                  # Sample encrypted Word
│   ├── test.xlsx                  # Sample encrypted Excel
│   └── test.pptx                  # Sample encrypted PowerPoint
│
├── wordlists/                     # Password dictionaries
│   ├── common-passwords.txt       # 33 common passwords
│   ├── chinese-common.txt         # 40 common Chinese passwords
│   ├── pins-4digit.txt            # 10,000 4-digit PINs
│   └── pins-6digit.txt            # 1,000,000 6-digit PINs
│
├── docs/                          # Documentation
│   ├── USER_GUIDE.md              # Complete usage guide
│   ├── DISTRIBUTION.md            # Homebrew/APT/AUR/Docker/crates.io
│   ├── RELEASE_LINUX.md           # Build matrix and release process
│   ├── PROJECT_STRUCTURE.md       # This file
│   ├── PERFORMANCE.md             # Performance benchmarks
│   ├── ZIP_IMPLEMENTATION.md      # ZIP encryption implementation
│   ├── DEVELOPMENT.md             # Development guide
│   └── CI-CD.md                   # CI/CD workflows
│
├── scripts/                       # Development and release scripts
│   ├── fmt.sh                     # Format and lint
│   ├── release.sh                 # Version bump + tag
│   ├── install.sh                 # Static binary installer
│   ├── gen-package-manifests.py   # Homebrew/AUR manifest renderer
│   ├── build-apt-repo.sh          # Signed APT repository builder
│   └── install-hooks.sh           # Install git hooks
│
├── Dockerfile                     # scratch image around the static musl binary
│
└── .github/                       # GitHub workflows
    └── workflows/
        ├── ci.yml                 # test, build matrix, GitHub Release
        └── publish.yml            # APT/Homebrew/AUR/GHCR/crates.io
```

## Core Abstractions

### PasswordVerifier Trait (`src/formats/mod.rs`)

```rust
pub trait PasswordVerifier: Send + Sync {
    fn quick_check(&self, password: &[u8]) -> bool;  // Fast pre-filter
    fn verify(&self, password: &[u8]) -> bool;        // Full verification
    fn format_name(&self) -> &'static str;
    fn encryption_info(&self) -> &str;
}
```

Two-phase design: `quick_check` rejects ~99% of wrong passwords cheaply (e.g., ZIP's 12-byte header), `verify` does the expensive full decryption + CRC check.

### PasswordSource Trait (`src/generators/mod.rs`)

```rust
pub trait PasswordSource: Send {
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>) -> bool;
    fn estimated_total(&self) -> Option<u64>;
    fn checkpoint(&self) -> Option<String>;
    fn restore(&mut self, checkpoint: &str) -> Result<()>;
    fn name(&self) -> &str;
}
```

Streaming design: passwords are generated in batches, never holding the entire search space in memory.

### CrackerEngine (`src/engine/mod.rs`)

Pipeline:
```
PasswordSource.fill_batch(max) → bounded FIFO channel → worker threads
  worker: quick_check() → verify() → AtomicBool found
  worker: mark batch completed → checkpoint frontier advances
```

- Persistent FIFO workers (not Rayon) so batches complete roughly in order,
  which is required for a meaningful checkpoint frontier
- Adaptive batch size: starts at 128, targets ~250 ms per batch, caps at 4096
  (slow formats checkpoint promptly, fast formats keep large batches)
- Bounded channel (`thread_count`) keeps memory flat
- Checkpoint = start position of the earliest incomplete batch, so resuming
  never skips unverified candidates
- Independent progress/checkpoint thread (100ms refresh)

## Format Auto-Detection (`src/formats/mod.rs`)

| Magic Bytes | Format | Notes |
|------------|--------|-------|
| `PK\x03\x04` | ZIP | Also used for OOXML (.docx/.xlsx/.pptx) |
| `%PDF-` | PDF | |
| `\xD0\xCF\x11\xE0...` | Office | OLE2 compound file (old binary formats) |

OOXML files are ZIP containers, so extension is checked after magic bytes to distinguish from plain ZIP archives.

## Dependencies

All dependencies are at latest stable versions (as of 2026-06):

| Category | Crates |
|----------|--------|
| CLI | clap 4.6, clap_complete 4.6, clap_mangen 0.3 |
| Concurrency | crossbeam-channel 0.5, num_cpus 1.17 |
| Progress | indicatif 0.18 |
| Errors | anyhow 1.0, thiserror 2.0 |
| ZIP | zip 9.1 |
| Crypto hashes | sha2 0.11, sha1 0.11, md-5 0.11, hmac 0.13, pbkdf2 0.13 |
| Crypto ciphers | aes 0.9, cbc 0.2, rc4 0.2, cipher 0.5 |
| CRC | crc32fast 1.5 |
| OLE2 | cfb 0.14 |
| Wordlists | memmap2 0.9 |
| Memory-mapped I/O | memmap2 0.9 |
| Utils | base64 0.22 |
