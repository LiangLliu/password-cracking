# Performance Guide

## Benchmarks

Measured on Apple M2 Pro (10-core CPU), Rust 1.96, release build with `opt-level=3`, `lto=thin`, `codegen-units=1`.

### By Format and Encryption

| Format | Encryption | Speed (passwords/sec) | Bottleneck |
|--------|-----------|----------------------|------------|
| ZIP | ZipCrypto | 5,800,000 – 7,400,000 | CRC32 + stream cipher (12-byte header check) |
| PDF | RC4-128 | 470,000 – 550,000 | MD5 key derivation + 20-round RC4 |
| Office | Agile AES-256 | 400 – 450 | 100K SHA-512 iterations per password |

### By Attack Mode (ZIP ZipCrypto)

| Attack | Keyspace | Time | Speed |
|--------|---------|------|-------|
| Dictionary (33 words) | 33 | <0.1s | 314/sec (wordlist loading dominates) |
| Brute-force digits 1-6 | 1,111,110 | 0.2s | 5.3M/sec |
| Brute-force alnum 1-4 | 15,018,570 | 2.5s | 5.8M/sec |
| Hybrid (33 words + 2 rules) | 99 | <0.1s | 885/sec |
| Hybrid (33 words + 100 digits) | 3,366 | 0.2s | 22K/sec |

### Keyspace vs Time (ZIP ZipCrypto, 7M/sec)

| Charset | Length 4 | Length 6 | Length 8 |
|---------|---------|---------|---------|
| digits (10) | instant | instant | 14 sec |
| lower (26) | instant | 44 sec | 8.2 hours |
| alnum (62) | 2 sec | 2.2 hours | 74 days |
| all (94) | 11 sec | 27 hours | 28 years |

### Keyspace vs Time (PDF RC4-128, 500K/sec)

| Charset | Length 4 | Length 6 | Length 8 |
|---------|---------|---------|---------|
| digits (10) | instant | 2 sec | 3.3 min |
| lower (26) | 0.9 sec | 10 min | 4.8 days |
| alnum (62) | 30 sec | 31 hours | 138 days |
| all (94) | 2.6 min | 16 days | 142 years |

### Keyspace vs Time (Office Agile AES-256, 400/sec)

| Charset | Length 4 | Length 6 | Length 8 |
|---------|---------|---------|---------|
| digits (10) | 25 sec | 42 min | 3.2 days |
| lower (26) | 19 min | 9 days | 137 days |
| alnum (62) | 10.4 hours | 1.8 years | 173 years |

> **Office is ~14,000x slower than ZIP** due to 100,000 SHA-512 iterations per password. Always try dictionary/hybrid first.

## Optimization Strategies

### 1. Use Dictionary First

Dictionary attacks are instant for any format. Always try wordlists before brute-force:

```bash
# Fast: dictionary (33 common passwords)
password-cracking -f document.docx dictionary -w wordlists/common-passwords.txt

# Medium: hybrid (33 × 103 = 3,399 candidates)
password-cracking -f document.docx hybrid -w wordlists/common-passwords.txt \
  --capitalize --append-digits 99

# Slow: brute-force (use only as last resort for Office)
password-cracking -f document.docx brute-force -c digits --min-length 4 --max-length 4
```

### 2. Limit the Keyspace

The smaller the charset and length range, the exponentially faster:

```bash
# Good: digits only, 4 chars = 10,000 candidates
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 4

# Bad: all chars, 8 chars = 6 trillion candidates
password-cracking -f document.pdf brute-force -c all --min-length 1 --max-length 8
```

### 3. Thread Count

By default, the tool uses all logical CPU cores. You can limit it:

```bash
# Use 4 threads (leave cores for other work)
password-cracking -f document.zip -t 4 brute-force -c digits --min-length 1 --max-length 6
```

### 4. Build Optimization

The `Cargo.toml` release profile is already optimized:

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
panic = "abort"
strip = true
```

- `opt-level = 3`: maximum optimization
- `lto = "thin"`: cross-crate inlining without slow compile times
- `codegen-units = 1`: better optimization at cost of compile time
- `panic = "abort"`: smaller binary, no unwinding overhead
- `strip = true`: strip debug symbols from binary

## How Two-Phase Verification Works

The `PasswordVerifier` trait has two methods:

1. **`quick_check`** — fast pre-filter, rejects ~99% of wrong passwords
2. **`verify`** — full decryption + integrity check

The engine calls `quick_check` first, and only calls `verify` if `quick_check` passes. This gives a massive speedup:

| Format | quick_check | verify | Speedup |
|--------|-------------|--------|---------|
| ZIP ZipCrypto | 12-byte header decrypt + 1 byte compare | Full file decrypt + CRC32 | ~20x |
| PDF RC4-128 | MD5 + 20-round RC4 + 16-byte compare | (same as quick_check) | 1x |
| Office Agile | 100K SHA-512 + AES decrypt + compare | (same as quick_check) | 1x |

For PDF and Office, `quick_check` and `verify` do the same work (the password verification itself is the expensive step). For ZIP ZipCrypto, the 12-byte header check is dramatically cheaper than full file decryption.
