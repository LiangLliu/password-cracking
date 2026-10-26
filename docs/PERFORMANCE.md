# Performance Guide

**English** | [中文](PERFORMANCE.zh-CN.md)

Measured on an Apple M2 Pro (10-core CPU), Rust 1.97, release build with
`opt-level=3`, `lto="thin"`, `codegen-units=1`. Treat these numbers as
order-of-magnitude guidance; encryption parameters (especially Office
`spinCount`) and CPU generation change the result.

## By format and encryption

| Format | Encryption | Speed | Bottleneck |
|--------|-----------|-------|------------|
| ZIP | ZipCrypto | 11.0M–11.5M passwords/sec | CRC32 + stream cipher with 12-byte header filter |
| ZIP | AES-256 | ~15K passwords/sec | PBKDF2 + AES + HMAC per candidate (handled by `zip`) |
| PDF | RC4-128 | 510K–540K passwords/sec | MD5 key derivation + 20-round RC4 |
| PDF | AES-128 | ~515K passwords/sec | MD5 key derivation + 20-round RC4 (R4 U check) |
| PDF | AES-256 R6 | ~7.6K passwords/sec | Algorithm 2.B hash iteration |
| PDF (owner) | RC4-128 | ~150K passwords/sec | User check + owner recovery (`--pdf-owner`) |
| Office | Agile AES-256 | 500–800 passwords/sec (585 measured) | ~100K SHA-512 iterations per candidate |

`--pdf-owner` roughly triples PDF cost because every candidate runs both the
user and owner checks.

## By attack mode (ZIP ZipCrypto)

| Attack | Candidates | Time | Throughput |
|--------|-----------|------|------------|
| Dictionary, 33 words | 33 | <0.1 s | startup-dominated |
| Brute-force digits 1–6 | 1,111,110 | 0.1 s | 10.6M/sec |
| Brute-force alnum 1–4 | 15,018,570 | 1.4 s | 11.3M/sec |
| Mask `?d?d?l?l` | 67,600 | 0.01 s | ~10M/sec |
| Hybrid, 33 words + 2 rules | 99 | <0.1 s | startup-dominated |
| Hybrid, 33 words + `--append-digits 99` | 3,366 | 0.2 s | 22K/sec |

Short runs are dominated by process startup, thread creation and wordlist
mapping, not by verification.

## Keyspace vs time

Exact-length keyspaces (`C^L`). Times are approximate and assume ideal scaling.

### ZIP ZipCrypto (11M/sec)

| Charset | Length 4 | Length 6 | Length 8 |
|---------|----------|----------|----------|
| digits (10) | 10,000 | 1,000,000 | 100,000,000 |
| lower (26) | 456,976 | 308,915,776 | 208,827,064,576 |
| alnum (62) | 14,776,336 | 56,800,235,584 | 218,340,105,584,896 |
| all (93) | 74,805,201 | 646,990,183,449 | 5,595,818,096,650,401 |

| Time | Length 4 | Length 6 | Length 8 |
|------|----------|----------|----------|
| digits | instant | instant | 9 sec |
| lower | instant | 28 sec | 5.3 hours |
| alnum | 1.3 sec | 1.4 hours | 230 days |
| all | 7 sec | 16 hours | 16 years |

### PDF RC4-128 (530K/sec)

| Time | Length 4 | Length 6 | Length 8 |
|------|----------|----------|----------|
| digits | instant | 2 sec | 3.1 min |
| lower | 0.9 sec | 10 min | 4.5 days |
| alnum | 28 sec | 30 hours | 13 years |
| all | 2.4 min | 14 days | 335 years |

### Office Agile AES-256 (600/sec)

| Time | Length 4 | Length 6 | Length 8 |
|------|----------|----------|----------|
| digits | 17 sec | 28 min | 1.9 days |
| lower | 13 min | 6 days | 11 years |
| alnum | 6.8 hours | 3 years | 11,500 years |

> Office is roughly 18,000× slower than ZIP ZipCrypto. Dictionary and hybrid
> attacks are the only practical options for realistic passwords.

## Engine design

- **Persistent FIFO workers.** Batches are pulled in order from a bounded
  channel. This keeps the checkpoint frontier meaningful; a LIFO scheduler
  (such as Rayon's work-stealing queue) would leave early batches unfinished
  while later ones complete.
- **Adaptive batching.** A shared target starts at 32 and doubles or halves
  after each batch, aiming for ~250 ms and capping at 4096. Slow formats get
  small batches for timely checkpoints; fast formats get large batches to
  amortize channel synchronization.
- **Streaming sources.** Dictionary candidates are memory-mapped and read by
  byte offset. Memory stays flat regardless of wordlist size.
- **Bounded channel.** Capacity is the worker count, so the generator cannot
  run far ahead of verification.

## Two-phase verification

`PasswordVerifier` exposes `quick_check` and `verify`. The engine calls
`quick_check` first and only calls `verify` when it returns true.

| Format | `quick_check` | `verify` | Effective gain |
|--------|---------------|----------|----------------|
| ZIP ZipCrypto | Decrypt 12-byte header, compare one byte | Decrypt all encrypted entries, validate CRC | ~20× |
| ZIP AES | `true` | AES/HMAC validation | none |
| PDF | `true` | Password hash check | none |
| Office | `true` | 100K-iteration hash + AES | none |

For ZIP ZipCrypto the header check rejects about 99.6% of wrong passwords with
12 bytes of work. For formats without a cheaper filter, returning `true` from
`quick_check` avoids paying for verification twice.

## Optimization strategies

1. **Dictionary first.** It is effectively instant for every format.
2. **Use the smallest charset.** `all` (93) is 9.3× larger than `digits` (10)
   per position.
3. **Bound the length.** Each additional position multiplies by the charset
   size.
4. **Use a mask when the shape is known.** `?d?d?l?l` is 67,600 candidates;
   `?a?a?a?a` is 74.8 million.
5. **Use all cores.** The default is all logical CPUs. Limit with `-t` only on
   shared machines.
6. **Avoid `--dedup` on huge lists unless needed.** It stores a hash per unique
   candidate; the default streaming mode uses constant memory.

## Build profile

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
panic = "abort"
strip = true
```

- `opt-level = 3` — maximum optimization for the hot hash/cipher loops.
- `lto = "thin"` — cross-crate inlining without full-LTO compile times.
- `codegen-units = 1` — better optimization, slower builds.
- `panic = "abort"` — smaller binary and no unwinding tables.
- `strip = true` — no debug symbols in release binaries.

The Linux packages are built from the static musl binaries, which have the same
optimization profile.

## Reproducing benchmarks

```bash
cargo build --release
BIN=./target/release/password-cracking

# ZIP ZipCrypto, alnum 1-4 (15M candidates)
time "$BIN" -q -f examples/test.zip brute-force -c alnum --min-length 1 --max-length 4

# PDF RC4-128, digits 1-6 (1.1M candidates)
time "$BIN" -q -f examples/test.pdf brute-force -c digits --min-length 1 --max-length 6

# Office Agile AES-256, digits 1-3 (1,110 candidates)
time "$BIN" -q -f examples/test.docx brute-force -c digits --min-length 1 --max-length 3

# Mask
time "$BIN" -q -f examples/test.zip mask -m "?d?d?l?l"
```

Use a release build. Debug builds are several times slower and are not
representative.

## Profiling

On Linux, `perf` works well:

```bash
perf record -g ./target/release/password-cracking ...
perf report
```

On macOS, Instruments (Time Profiler) or `cargo flamegraph` can be used. The hot
paths are:

- ZIP: `ZipCrypto::update`, `crc32_byte`, `quick_check`
- PDF: MD5/RC4 key derivation and `algorithm_2b`
- Office: `derive_hash` (the 100K-iteration loop), AES-CBC

Changes to these functions should be benchmarked before and after; small
regressions compound across millions of candidates.
