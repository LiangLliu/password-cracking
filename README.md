# Password Cracking Tool

[中文文档](./README_CN.md) | [English](./README.md)

A high-performance document password cracking tool written in Rust, supporting ZIP, PDF, and Office documents with dictionary, brute-force, and hybrid (rule-based) attacks.

> **Warning**: This tool is for legitimate purposes only — recovering your own password-protected files or authorized security testing. Do not use for illegal purposes.

## Features

- **Multi-format**: ZIP (ZipCrypto + AES), PDF (RC4-40/128, AES-128/256), Office (Agile AES-256)
- **Multi-threaded**: Rayon work-stealing parallelism across all CPU cores
- **Three attack modes**: Dictionary, brute-force, hybrid (dictionary + rules)
- **Auto-detection**: Identifies file format by magic bytes, not just extension
- **Two-phase verification**: Fast header check rejects ~99% of wrong passwords before full decryption
- **Cross-platform**: Linux, macOS, Windows

## Quick Start

```bash
# Build
cargo build --release

# Dictionary attack
./target/release/password-cracking -f document.zip dictionary -w passwords.txt

# Brute-force (4-6 digit PIN)
./target/release/password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 6

# Hybrid (dictionary + rules)
./target/release/password-cracking -f document.docx hybrid -w dict.txt --capitalize --l33t --append-digits 99

# Quiet mode (for scripting: prints only the password)
./target/release/password-cracking -q -f document.zip dictionary -w passwords.txt
```

## Performance

| Format | Encryption | Speed (passwords/sec) |
|--------|-----------|----------------------|
| ZIP | ZipCrypto | 7,000,000+ |
| PDF | RC4-128 | 470,000+ |
| Office | Agile AES-256 | 60+ |

Performance depends on CPU cores, encryption type, and password complexity.

## Architecture

```
src/
  main.rs              CLI entry point
  lib.rs               public exports
  cli/                 clap v4 argument parsing
  engine/              Rayon + crossbeam cracking engine
  formats/             PasswordVerifier trait + ZIP/PDF/Office implementations
  generators/          PasswordSource trait + dictionary/brute-force/rules
  utils/               character sets, formatting, validation
```

## License

MIT — see [LICENSE](LICENSE)
