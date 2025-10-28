# Password Cracking Tool

[中文文档](./README_CN.md) | [English](./README.md)

A high-performance document password cracking tool written in Rust, supporting multiple file formats and attack modes.

⚠️ **Warning**: This tool is for legitimate purposes only, such as recovering your own password-protected files or authorized security testing. Do not use for illegal purposes.

## Features

- 🚀 **High Performance**: Written in Rust with multi-threaded parallel processing
- 📄 **Multi-format Support**: ZIP, PDF, Office (Word, Excel, PowerPoint)
- 🔧 **Attack Modes**: Dictionary, Brute-force, Hybrid
- 📊 **Real-time Progress**: Shows speed, percentage, and ETA
- 🎯 **Smart Optimization**: Auto-detects CPU cores and adjusts batch size
- ⚡ **Performance Modes**: Aggressive and balanced modes
- 🌍 **Cross-platform**: Works on Windows, macOS, and Linux

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/LiangLliu/password-cracking
cd password-cracking

# Build release version
cargo build --release

# Binary location: target/release/password-cracking
```

## Usage

### Basic Syntax

```bash
password-cracking -f <FILE> <COMMAND> [OPTIONS]
```

### Global Options

- `-f, --file <FILE>`: Target file to crack (required)
- `-t, --threads <THREADS>`: Number of threads (default: auto-detect)
- `-p, --performance <MODE>`: Performance mode: balanced, aggressive (default: aggressive)

### Commands

#### 1. Dictionary Attack

Try passwords from a wordlist file or directory:

```bash
# Single file
password-cracking -f document.zip dictionary -w passwords.txt

# Directory (recursively loads all .txt files)
password-cracking -f document.zip dictionary -w wordlists/
```

Options:
- `-w, --wordlist <PATH>`: Path to wordlist file or directory (required)

Features:
- Single file mode: Loads passwords from one file
- Directory mode: Recursively scans for all `.txt` files
- Automatic duplicate removal
- Skip empty lines and comments (lines starting with #)

#### 2. Brute Force Attack

Try all possible combinations:

```bash
# 4-6 digit PIN
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 6

# Custom character set
password-cracking -f document.zip brute-force -c "abc123" --max-length 8
```

Options:
- `-c, --charset <CHARSET>`: Character set to use
- `--min-length <N>`: Minimum password length (default: 1)
- `--max-length <N>`: Maximum password length (required)

Available charsets:
- `digits`: 0-9
- `lowercase`: a-z
- `uppercase`: A-Z
- `letters`: a-zA-Z
- `alnum`: a-zA-Z0-9
- `special`: !@#$%^&*()_+-=[]{}|;:,.<>?
- `all`: All printable ASCII
- Custom: Any string (e.g., "abc123!@#")

#### 3. Hybrid Attack

Dictionary words with variations:

```bash
password-cracking -f document.xlsx hybrid -w base_words.txt
```

Options:
- `-w, --wordlist <FILE>`: Base wordlist (required)

## Examples

### Common Use Cases

```bash
# Crack ZIP with common passwords
password-cracking -f archive.zip dictionary -w common-passwords.txt

# Crack PDF with 4-digit PIN
password-cracking -f invoice.pdf brute-force -c digits --min-length 4 --max-length 4

# Crack Excel with 6-8 alphanumeric password
password-cracking -f data.xlsx brute-force -c alnum --min-length 6 --max-length 8

# Use all CPU cores aggressively
password-cracking -f file.zip -p aggressive dictionary -w wordlist.txt

# Use balanced mode (leaves 1 core free)
password-cracking -f file.pdf -p balanced brute-force -c lowercase --max-length 6
```

## Performance Tips

1. **Character Set**: Smaller sets = faster cracking
   - 10 chars (digits): 10^n combinations
   - 26 chars (lowercase): 26^n combinations
   - 62 chars (alnum): 62^n combinations

2. **Password Length**: Time grows exponentially
   - 4 chars: seconds
   - 6 chars: minutes to hours
   - 8+ chars: days to years

3. **File Formats** (fastest to slowest):
   - ZIP (traditional): Very fast
   - ZIP (AES): Moderate
   - PDF: Slow
   - Office: Very slow

4. **CPU Usage**:
   - Aggressive: Uses all cores
   - Balanced: Leaves 1 core free for system

## Supported Formats

| Format | Extensions | Encryption Types | Speed |
|--------|------------|------------------|-------|
| ZIP | .zip | Traditional (ZipCrypto), AES | Fast/Moderate |
| PDF | .pdf | RC4, AES-128, AES-256 | Slow |
| Office | .docx, .xlsx, .pptx | Office 2007+ (AES) | Very Slow |
| RAR | .rar | RAR3, RAR5 (planned) | - |

## Project Structure

```
password-cracking/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── cracker/          # Core cracking engine
│   ├── formats/          # File format handlers
│   │   ├── zip.rs        # ZIP support
│   │   ├── pdf.rs        # PDF support
│   │   └── office.rs     # Office support
│   ├── generator/        # Password generators
│   │   ├── brute_force.rs
│   │   └── dictionary.rs
│   └── utils/           # Utilities
├── examples/            # Test files and scripts
│   ├── create_test_files.py  # Script to create test files
│   ├── pyproject.toml        # UV project configuration
│   └── test.zip              # Sample encrypted file
├── wordlists/           # Password dictionaries
├── docs/               # Documentation
└── Cargo.toml          # Dependencies
```

## Test Files

The `examples/` directory contains test files for validating the tool:

### Quick Test

```bash
cd examples

# Install UV (Python package manager) if needed
curl -LsSf https://astral.sh/uv/install.sh | sh

# Create test files (PDF, Word, Excel, PowerPoint)
uv sync
uv run python create_test_files.py

# Test with the password cracking tool
cd ..
./target/release/password-cracking -f examples/test.zip dictionary -w wordlists/common-passwords.txt
```

All test files use the password: **92eo**

## Technical Details

- **Language**: Rust (memory safe, zero-cost abstractions)
- **Parallelization**: Rayon for work-stealing parallelism
- **Progress**: indicatif for real-time progress bars
- **ZIP**: Proper CRC32 validation for accurate results
- **Cross-platform**: Pure Rust, no platform-specific code

## Documentation

- [Performance Analysis](./docs/PERFORMANCE.md)
- [Project Structure](./docs/PROJECT_STRUCTURE.md)
- [ZIP Implementation Details](./docs/ZIP_IMPLEMENTATION.md)

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Legal Disclaimer

This tool is provided for legitimate purposes only. Users are responsible for complying with all applicable laws. The authors are not responsible for any misuse.

## License

MIT License - see LICENSE file for details