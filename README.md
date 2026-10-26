# Password Cracking Tool

**English** | [中文](README_CN.md)

`password-cracking` recovers passwords for ZIP, PDF and Office files on Linux
and macOS. It runs on the CPU, needs no network access, and supports dictionary,
brute-force, mask and hybrid attacks.

> Use this tool only on files you own or have permission to test.

## Features

- ZIP: ZipCrypto and AES-128/192/256.
- PDF: RC4-40/128, AES-128, AES-256 R5/R6, user and owner passwords.
- Office: Agile AES-128/192/256 (Office 2010 and later).
- Dictionary, brute-force, mask and hybrid attack modes.
- Memory-mapped wordlists and batched generation keep memory flat.
- Checkpoints resume long runs without repeating verified work.
- Format detection reads magic bytes and falls back to file extensions.
- Linux packages, Homebrew, Docker and a static binary installer.

## Install

```bash
# Homebrew (macOS / Linux)
brew install LiangLliu/tap/password-cracking

# Debian, Ubuntu, Raspberry Pi OS (signed APT repository)
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

The release also includes `.deb`, `.rpm` and `.apk` packages plus a `PKGBUILD`
for Arch Linux. AUR and crates.io publishing need a one-time account setup; see
[Development](docs/DEVELOPMENT.md).

## Usage

```bash
# Dictionary
password-cracking -f document.zip dictionary -w passwords.txt

# Brute-force a 4 to 6 digit PIN
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 6

# Mask: two digits then two lowercase letters
password-cracking -f document.zip mask -m "?d?d?l?l"

# Hybrid: dictionary words with mutations
password-cracking -f document.docx hybrid -w dict.txt --capitalize --l33t --append-digits 99

# Resume an interrupted run
password-cracking -f document.pdf brute-force -c alnum --min-length 1 --max-length 8 \
  --checkpoint progress.ck --resume

# Print only the password, for scripts
password-cracking -q -f document.zip dictionary -w passwords.txt
```

Run `password-cracking --help` or `password-cracking man` for the full command
reference. The [usage guide](docs/USAGE.md) covers every attack mode, exit code
and checkpoint rule.

## Performance

Measured on an Apple M2 Pro (10 cores) with a release build:

| Format | Encryption | Speed |
|--------|------------|-------|
| ZIP | ZipCrypto | 11.0M to 11.5M passwords/sec |
| ZIP | AES-256 | about 15K passwords/sec |
| PDF | RC4-128 | 510K to 540K passwords/sec |
| PDF | AES-256 R6 | about 7.6K passwords/sec |
| PDF (owner) | RC4-128 | about 150K passwords/sec (`--pdf-owner`) |
| Office | Agile AES-256 | 500 to 800 passwords/sec |

Office spends about 100,000 SHA-512 iterations on each candidate. Start with a
dictionary or hybrid attack for Office files. Encryption settings and CPU
generation change these numbers.

## How it works

```text
CLI (clap)
  └── CrackerEngine
        ├── PasswordSource   dictionary | brute-force | mask | hybrid
        │     └── fill_batch(max) ──► bounded FIFO channel
        └── PasswordVerifier ZIP | PDF | Office
              ├── quick_check()  cheap filter (ZIP only)
              └── verify()       full password check
```

A generator thread fills bounded batches and sends them through a FIFO channel
to a fixed pool of worker threads. FIFO order gives checkpoints a stable
meaning: the file stores the start of the earliest batch that has not finished.
Batch size adapts from 32 to 4096, targeting about 250 ms of work per batch.

The engine calls `quick_check` before `verify`. ZIP ZipCrypto uses a 12-byte
header check to reject wrong passwords before full decryption. PDF and Office
skip the cheap check because verification is already the only expensive step.

## Documentation

- [Usage guide](docs/USAGE.md) / [中文](docs/USAGE.zh-CN.md)
- [Development and release](docs/DEVELOPMENT.md) / [中文](docs/DEVELOPMENT.zh-CN.md)
- [Test fixtures](examples/README.md) / [中文](examples/README.zh-CN.md)

## Limitations

- Office: Agile encryption only. The program rejects legacy CryptoAPI/ECMA-376
  files.
- RAR5 and 7z are not supported.
- PDF: empty passwords and certificate encryption are out of scope.
- ZIP: split and multi-volume archives are not supported.
- Windows is not supported.
- Dictionary checkpoints assume the wordlist did not change between runs.

## License

MIT. See [LICENSE](LICENSE).
