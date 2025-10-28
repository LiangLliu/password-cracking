# Password Cracking Tool

[中文文档](./README_CN.md) | [English](./README.md)

A high-performance document password cracking tool written in Rust, supporting multiple file formats and attack modes.

⚠️ **Warning**: This tool is for legitimate purposes only, such as recovering your own password-protected files or authorized security testing. Do not use for illegal purposes.

## ✨ Features

- 🚀 **High Performance**: Multi-threaded parallel processing
- 📄 **Multi-format Support**: ZIP, PDF, Office documents
- 🔧 **Attack Modes**: Dictionary, Brute-force, Hybrid
- 📊 **Real-time Progress**: Speed, percentage, and ETA
- 🌍 **Cross-platform**: Windows, macOS, and Linux

## 🚀 Quick Start

### Installation

```bash
# Clone and build
git clone https://github.com/LiangLliu/password-cracking
cd password-cracking
cargo build --release

# Install development hooks (optional)
./scripts/install-hooks.sh
```

### Basic Usage

```bash
# Dictionary attack
password-cracking -f document.zip dictionary -w passwords.txt

# Brute force (4-6 digit PIN)
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 6

# Hybrid attack
password-cracking -f document.docx hybrid -w dictionary.txt -m append-digits
```

## 📚 Documentation

- **[User Guide](docs/USER_GUIDE.md)** - Detailed usage instructions
- **[Development Guide](docs/DEVELOPMENT.md)** - Setup and coding standards
- **[Project Structure](docs/PROJECT_STRUCTURE.md)** - Architecture overview
- **[CI/CD Guide](docs/CI-CD.md)** - Automated workflows
- **[All Documents](docs/)** - Complete documentation

## 🛠 Development

```bash
# Run code checks
./scripts/fmt.sh

# Run tests
cargo test

# Create a release
./scripts/release.sh patch
```

See [scripts/](scripts/) for all available development tools.

## 📦 Example Files

Test files with password `92eo` are included:

```bash
cd examples
python create_test_files.py  # Generate test files
cd ..
./target/release/password-cracking -f examples/test.zip dictionary -w wordlists/common-passwords.txt
```

## ⚡ Performance

Typical speeds on an 8-core CPU:
- Dictionary attack: 100K-500K passwords/sec
- Brute force (digits): 10M+ passwords/sec
- Hybrid attack: 10K-100K passwords/sec

## 🤝 Contributing

Contributions are welcome! Please read our [Development Guide](docs/DEVELOPMENT.md) first.

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details.

## ⚖️ Legal Disclaimer

This tool is provided for legitimate purposes only. Users are responsible for complying with all applicable laws. The authors are not responsible for any misuse.