# Password Cracking Tool

**English** | [中文](README_CN.md)

A high-performance document password cracker written in Rust. It recovers
passwords for ZIP, PDF and Office files using dictionary, brute-force, mask and
hybrid attacks, and runs entirely on the CPU with no external services.

> **Authorized use only.** Use this tool to recover your own files or during
> authorized security testing. Do not use it on data you do not own or have
> permission to test.

## Features

- **Formats**: ZIP (ZipCrypto, AES-128/192/256), PDF (RC4-40/128, AES-128,
  AES-256 R5/R6, user and owner passwords), Office (Agile AES-128/192/256)
- **Four attack modes**: dictionary, brute-force, mask (`?d?d?l?l`) and hybrid
  (dictionary + mutation rules)
- **Fast**: FIFO worker pool with adaptive batching; 11M+ ZIP passwords/sec on
  a 10-core laptop CPU
- **Streaming**: memory-mapped wordlists and batched generation keep memory flat
  for arbitrarily large inputs
- **Resumable**: `--checkpoint` / `--resume` never re-checks verified candidates
- **Format detection**: magic-byte detection, not just file extensions
- **Two-phase verification**: ZIP's 12-byte header check rejects almost all
  wrong passwords before full decryption
- **Linux and macOS**: static musl Linux builds, native macOS builds, man page,
  shell completions, `.deb`/`.rpm`/`.apk`, Homebrew and Docker

## Install

```bash
# Homebrew (macOS / Linux)
brew install LiangLliu/tap/password-cracking

# Debian / Ubuntu / Raspberry Pi OS (signed APT repository)
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://lianglliu.github.io/password-cracking/key.gpg \
  | sudo tee /etc/apt/keyrings/password-cracking.gpg > /dev/null
echo "deb [signed-by=/etc/apt/keyrings/password-cracking.gpg] https://lianglliu.github.io/password-cracking stable main" \
  | sudo tee /etc/apt/sources.list.d/password-cracking.list
sudo apt update && sudo apt install password-cracking

# Docker
docker run --rm -v "$PWD:/data" ghcr.io/lianglliu/password-cracking --version

# Static binary installer (Linux / macOS)
curl -fsSL https://raw.githubusercontent.com/LiangLliu/password-cracking/master/scripts/install.sh | sh

# From source
cargo install --git https://github.com/LiangLliu/password-cracking
```

Arch Linux (`password-cracking-bin`) and crates.io publishing are prepared but
need a one-time account/token setup. See
[Distribution Channels](docs/DISTRIBUTION.md) for the current status, signing
keys and troubleshooting.

## Usage

```bash
# Dictionary
password-cracking -f document.zip dictionary -w passwords.txt

# Brute-force (4-6 digit PIN)
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 6

# Mask: two digits followed by two lowercase letters (92eo)
password-cracking -f document.zip mask -m "?d?d?l?l"

# Hybrid: dictionary words plus mutations
password-cracking -f document.docx hybrid -w dict.txt --capitalize --l33t --append-digits 99

# Resume an interrupted brute-force run
password-cracking -f document.pdf brute-force -c alnum --min-length 1 --max-length 8 \
  --checkpoint progress.ck --resume

# Quiet mode for scripting: prints only the password
password-cracking -q -f document.zip dictionary -w passwords.txt
```

## Performance

Measured on an Apple M2 Pro (10-core CPU), release build:

| Format | Encryption | Speed |
|--------|-----------|-------|
| ZIP | ZipCrypto | 11.0M–11.5M passwords/sec |
| PDF | RC4-128 | 510K–540K passwords/sec |
| PDF (owner) | RC4-128 | ~150K passwords/sec (`--pdf-owner`) |
| Office | Agile AES-256 | 500–800 passwords/sec |

Office is slow by design: every candidate costs ~100,000 SHA-512 iterations.
Always start with a dictionary or hybrid attack for Office files. See the
[Performance Guide](docs/PERFORMANCE.md) for keyspace tables and tuning.

## How it works

```
CLI (clap)
  └── CrackerEngine
        ├── PasswordSource   dictionary | brute-force | mask | hybrid
        │     └── fill_batch(max) ──► bounded FIFO channel
        └── PasswordVerifier ZIP | PDF | Office
              ├── quick_check()  cheap filter (ZIP only)
              └── verify()       full password check
```

- A generator thread feeds bounded batches to a fixed pool of worker threads.
- Batches complete roughly in order, which makes the checkpoint frontier
  meaningful: the saved position is the start of the earliest batch that has
  not been fully verified.
- Batch size adapts from 32 to 4096, targeting ~250 ms per batch so slow
  formats checkpoint promptly and fast formats amortize synchronization.

## Documentation

| Document | English | 中文 |
|----------|---------|------|
| User guide | [USER_GUIDE.md](docs/USER_GUIDE.md) | [USER_GUIDE.zh-CN.md](docs/USER_GUIDE.zh-CN.md) |
| Distribution | [DISTRIBUTION.md](docs/DISTRIBUTION.md) | [DISTRIBUTION.zh-CN.md](docs/DISTRIBUTION.zh-CN.md) |
| Performance | [PERFORMANCE.md](docs/PERFORMANCE.md) | [PERFORMANCE.zh-CN.md](docs/PERFORMANCE.zh-CN.md) |
| Architecture | [PROJECT_STRUCTURE.md](docs/PROJECT_STRUCTURE.md) | [PROJECT_STRUCTURE.zh-CN.md](docs/PROJECT_STRUCTURE.zh-CN.md) |
| Development | [DEVELOPMENT.md](docs/DEVELOPMENT.md) | [DEVELOPMENT.zh-CN.md](docs/DEVELOPMENT.zh-CN.md) |
| CI/CD | [CI-CD.md](docs/CI-CD.md) | [CI-CD.zh-CN.md](docs/CI-CD.zh-CN.md) |
| Release engineering | [RELEASE_LINUX.md](docs/RELEASE_LINUX.md) | [RELEASE_LINUX.zh-CN.md](docs/RELEASE_LINUX.zh-CN.md) |
| ZIP internals | [ZIP_IMPLEMENTATION.md](docs/ZIP_IMPLEMENTATION.md) | [ZIP_IMPLEMENTATION.zh-CN.md](docs/ZIP_IMPLEMENTATION.zh-CN.md) |
| PDF internals | [PDF_IMPLEMENTATION.md](docs/PDF_IMPLEMENTATION.md) | [PDF_IMPLEMENTATION.zh-CN.md](docs/PDF_IMPLEMENTATION.zh-CN.md) |
| Office internals | [OFFICE_IMPLEMENTATION.md](docs/OFFICE_IMPLEMENTATION.md) | [OFFICE_IMPLEMENTATION.zh-CN.md](docs/OFFICE_IMPLEMENTATION.zh-CN.md) |
| Test fixtures | [examples/README.md](examples/README.md) | [examples/README.zh-CN.md](examples/README.zh-CN.md) |

## Limitations

- **Office**: Agile Encryption (v4, Office 2010+) only. Legacy
  CryptoAPI/ECMA-376 standard encryption is rejected with a clear error.
- **RAR5 / 7z**: not supported.
- **PDF**: user and owner passwords for RC4/AES; empty passwords and
  certificate-based encryption are out of scope.
- **ZIP**: ZipCrypto and AES; split/multi-volume archives are not supported.
- **Windows**: not supported. The code is portable, but builds and packages
  target Linux and macOS only.
- **Dictionary checkpoints** assume the wordlist is unchanged between runs.

## License

MIT — see [LICENSE](LICENSE).
