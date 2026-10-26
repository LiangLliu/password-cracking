# Documentation

**English** | [中文](README.zh-CN.md)

Every maintained document is available in English and Chinese. English files
use `NAME.md`, Chinese files use `NAME.zh-CN.md`.

| Document | What it covers |
|----------|----------------|
| [User Guide](USER_GUIDE.md) | Installation, attack modes, options, examples, troubleshooting |
| [Distribution Channels](DISTRIBUTION.md) | Homebrew, APT, AUR, Docker, crates.io, signing keys, verification |
| [Performance Guide](PERFORMANCE.md) | Benchmarks, keyspace tables, engine design, tuning |
| [Project Structure](PROJECT_STRUCTURE.md) | Module layout, traits, data flow, dependencies |
| [Development Guide](DEVELOPMENT.md) | Setup, coding style, tests, Git workflow, release process |
| [CI/CD](CI-CD.md) | Workflows, build matrix, publish jobs, secrets |
| [Release Engineering](RELEASE_LINUX.md) | Linux/macOS artifacts, packaging, checklist, risks |
| [ZIP Internals](ZIP_IMPLEMENTATION.md) | ZipCrypto/AES verification, 12-byte header check |
| [PDF Internals](PDF_IMPLEMENTATION.md) | RC4/AES, R5/R6 Algorithm 2.B, owner passwords, xref streams |
| [Office Internals](OFFICE_IMPLEMENTATION.md) | Agile encryption, hash iteration, OLE2/XML parsing |
| [Test Fixtures](../examples/README.md) | Fixture list, how to run them, how to regenerate |

## Version

These documents describe **v0.2.1**. The CLI help (`password-cracking --help`)
and the generated man page are the authoritative reference for flags.

## Historical material

`docs/superpowers/` contains design and planning records from earlier
iterations. It is kept for history and is **not** maintained or translated.
Current behaviour is defined by the source code and the documents above.
