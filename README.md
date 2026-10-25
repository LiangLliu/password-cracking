# Password Cracking Tool

[中文文档](./README_CN.md) | [English](./README.md)

A high-performance document password cracking tool written in Rust, supporting ZIP, PDF, and Office documents with dictionary, brute-force, and hybrid (rule-based) attacks.

> **Warning**: This tool is for legitimate purposes only — recovering your own password-protected files or authorized security testing. Do not use for illegal purposes.

## Features

- **Multi-format**: ZIP (ZipCrypto + AES), PDF (RC4-40/128, AES-128/256, owner passwords), Office (Agile AES-256)
- **Four attack modes**: Dictionary, brute-force, mask (`?d?d?l?l`), hybrid (dictionary + rules)
- **High performance**: FIFO worker pool with adaptive batching; 11M+ ZIP passwords/sec
- **Auto-detection**: Identifies file format by magic bytes, not just extension
- **Two-phase verification**: Fast header check rejects ~99% of wrong passwords before full decryption
- **Resumable**: `--checkpoint`/`--resume` survives interruptions
- **Streaming wordlists**: memory-mapped, flat memory for huge lists
- **Unix friendly**: man page, shell completions, `.deb`/`.rpm`/`.apk`, install script
- **Cross-platform**: Linux, macOS, Windows

## Quick Start

```bash
# Install (Linux/macOS, static binary)
curl -fsSL https://raw.githubusercontent.com/LiangLliu/password-cracking/master/scripts/install.sh | sh

# Or build from source
cargo build --release

# Dictionary attack
./target/release/password-cracking -f document.zip dictionary -w passwords.txt

# Brute-force (4-6 digit PIN)
./target/release/password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 6

# Mask (digit digit lower lower, e.g. 92eo)
./target/release/password-cracking -f document.zip mask -m "?d?d?l?l"

# Hybrid (dictionary + rules)
./target/release/password-cracking -f document.docx hybrid -w dict.txt --capitalize --l33t --append-digits 99

# Resume an interrupted brute-force run
./target/release/password-cracking -f document.pdf brute-force -c alnum --min-length 1 --max-length 8 \
  --checkpoint progress.ck --resume

# Quiet mode (for scripting: prints only the password)
./target/release/password-cracking -q -f document.zip dictionary -w passwords.txt
```

## Performance

| Format | Encryption | Speed (passwords/sec) |
|--------|-----------|----------------------|
| ZIP | ZipCrypto | 11,000,000+ |
| PDF | RC4-128 | 530,000+ |
| Office | Agile AES-256 | 500+ |

Performance depends on CPU cores, encryption type, and password complexity.

## Architecture

```
src/
  main.rs              CLI entry point
  lib.rs               public exports
  cli/                 clap v4 argument parsing
  engine/              Rayon + crossbeam cracking engine
  formats/             PasswordVerifier trait + ZIP/PDF/Office implementations
  generators/          PasswordSource trait + dictionary/brute-force/mask/rules
  utils/               character sets, formatting, validation
```

## Documentation

- **[User Guide](docs/USER_GUIDE.md)** — Complete usage instructions and examples
- **[Performance Guide](docs/PERFORMANCE.md)** — Benchmarks and optimization tips
- **[Project Structure](docs/PROJECT_STRUCTURE.md)** — Architecture overview
- **[ZIP Implementation](docs/ZIP_IMPLEMENTATION.md)** — ZipCrypto encryption details
- **[PDF Implementation](docs/PDF_IMPLEMENTATION.md)** — PDF encryption algorithms
- **[Office Implementation](docs/OFFICE_IMPLEMENTATION.md)** — Agile Encryption details
- **[Development Guide](docs/DEVELOPMENT.md)** — Coding standards and workflow
- **[Linux/Unix Release Plan](docs/RELEASE_LINUX.md)** — Packaging and distribution

## License

MIT — see [LICENSE](LICENSE)
