# Full Rewrite Design Spec

## Goal

Full rewrite of the password-cracking tool with:
- Latest Rust stable (1.96+) and latest dependencies
- Clean code, best practices
- Pure CPU extreme optimization
- Better CLI UX (auto-detection, config, resume)
- All formats equally prioritized: ZIP, PDF, Office

## Architecture: Adaptive Batch + Streaming Engine

### Core Traits

```rust
pub trait PasswordVerifier: Send + Sync {
    fn quick_check(&self, password: &[u8]) -> bool;
    fn verify(&self, password: &[u8]) -> bool;
    fn format_name(&self) -> &'static str;
    fn encryption_info(&self) -> &str;
}

pub trait PasswordSource: Send {
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>) -> bool;
    fn estimated_total(&self) -> Option<u64>;
    fn checkpoint(&self) -> Option<String>;
    fn restore(&mut self, checkpoint: &str) -> Result<()>;
}
```

### Engine Pipeline

```
PasswordSource.fill_batch() -> crossbeam channel -> rayon scope
  worker: quick_check() -> verify() -> AtomicBool found
```

- Adaptive batch: fast formats (ZIP) = 2000/batch, slow (Office) = 100/batch
- Passwords as `Box<[u8]>` to minimize allocation
- Progress on independent thread

## Module Structure

```
src/
  main.rs              CLI entry (clap v4)
  lib.rs               public exports
  engine/
    mod.rs             CrackerEngine, adaptive batching, progress
  formats/
    mod.rs             PasswordVerifier trait + factory + auto-detect
    zip.rs             ZIP Traditional (ZipCrypto) + AES
    pdf.rs             PDF RC4-40/128, AES-128/256
    office.rs          OOXML ECMA-376, Agile Encryption
  generators/
    mod.rs             PasswordSource trait + factory
    dictionary.rs      streaming wordlist (memmap)
    brute_force.rs     index-based, zero-alloc
    rules.rs           hashcat-style rule engine (hybrid)
  cli/
    mod.rs             CLI args, config, UX helpers
  utils/
    mod.rs             formatting, validation
```

## File Format Implementation Plan

### ZIP (zip.rs)
- Traditional ZipCrypto: own implementation with SIMD CRC32 (crc32fast crate)
- AES-128/192/256: use `zip` crate's built-in or implement via `aes` crate
- Verify by decrypting first entry + CRC check
- Support multi-entry ZIP (verify all encrypted entries)

### PDF (pdf.rs)
- Parse PDF encryption dict (/Encrypt object)
- Algorithms 1-2: RC4 40-bit (PDF 1.3)
- Algorithms 3-4: RC4 128-bit (PDF 1.4)
- Algorithm 5: AES-128 (PDF 1.6)
- Algorithm 6: AES-256 (PDF 2.0) - PBKDF2 + SHA-256
- Use `lopdf` for parsing, `rc4` + `aes` + `sha2` + `sha1` + `md5` for crypto
- quick_check: compute key + check first bytes of /U entry

### Office (office.rs)
- Parse EncryptionInfo stream from OOXML (ZIP-based) or OLE (CFB-based)
- ECMA-376 (2007): AES-128, SHA-1, PBKDF2
- Agile Encryption (2010+): configurable hash/cipher, default AES-256 + SHA-512
- quick_check: decrypt verifier, compare hash

## Password Generators

### Dictionary
- memmap2 for large wordlists (zero-copy read)
- Streaming: never load all into memory
- Directory support: recursive .txt
- Dedup via bloom filter (optional) or HashSet

### Brute Force
- Index-based counter, O(1) per password
- Configurable charset, length range
- Total combinations with overflow protection

### Rules (Hybrid)
- hashcat-style rule engine
- Common rules: capitalize, l33t, append digits, toggle case
- Compose with dictionary source

## CLI UX

- Auto-detect file type by magic bytes (not just extension)
- Suggest attack mode based on file type
- `--resume` from checkpoint file
- `--config` TOML config for presets
- Verbose/quiet modes
- Clean output: no debug spam, clear progress

## Performance Targets

| Format | Target speed | Notes |
|--------|-------------|-------|
| ZIP Traditional | 100K+/sec | SIMD CRC32 |
| ZIP AES | 20K+/sec | AES-NI auto |
| PDF RC4 | 50K+/sec | RC4 is fast |
| PDF AES | 10K+/sec | |
| Office | 1K+/sec | PBKDF2 bound |

## Dependencies (latest stable)

- clap 4.6, rayon 1.12, indicatif 0.18, anyhow 1.0
- crossbeam-channel 0.5, num_cpus 1.17
- zip 8.6 (aes-crypto), crc32fast 1.5
- aes 0.9, cbc 0.2, rc4 0.2, cipher 0.5
- sha2 0.11, sha1 0.11, md5 0.8, hmac 0.13, pbkdf2 0.13
- lopdf 0.43, cfb 0.14, memmap2 0.9
- thiserror 2.0, flate2 1.1
- tempfile 3.27 (dev)

## Implementation Phases

1. Scaffolding: Cargo.toml, module structure, traits
2. Utils + CLI skeleton
3. ZIP format (Traditional + AES)
4. PDF format (all algorithms)
5. Office format (ECMA-376 + Agile)
6. Generators (dictionary, brute force, rules)
7. Engine (adaptive batch, progress, resume)
8. CLI polish (auto-detect, config, UX)
9. Tests + examples + docs
