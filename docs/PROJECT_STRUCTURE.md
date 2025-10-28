# Project Structure

## Directory Layout

```
password-cracking/
├── Cargo.toml                 # Rust dependencies and project metadata
├── LICENSE                    # MIT License
├── README.md                  # English documentation
├── README_CN.md               # Chinese documentation (中文文档)
├── .gitignore                # Git ignore rules
│
├── src/                      # Source code
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library exports
│   │
│   ├── cracker/             # Core cracking engine
│   │   └── mod.rs          # Multi-threaded password cracker
│   │
│   ├── formats/             # File format handlers
│   │   ├── mod.rs          # Format trait and factory
│   │   ├── archive.rs      # ZIP file support
│   │   ├── pdf.rs          # PDF file support
│   │   ├── office.rs       # Office file support
│   │   └── zip_crypto.rs   # ZIP encryption algorithms
│   │
│   ├── generator/           # Password generators
│   │   ├── mod.rs          # Generator trait
│   │   ├── brute_force.rs  # Brute force generator
│   │   └── dictionary.rs   # Dictionary-based generator
│   │
│   └── utils/              # Utilities
│       └── mod.rs          # Helper functions
│
├── examples/               # Test files and scripts
│   ├── create_test_files.py # Python script to create test files
│   ├── pyproject.toml      # UV project configuration
│   ├── README.md           # Examples documentation
│   ├── test.zip            # Sample encrypted ZIP (password: 92eo)
│   ├── test.pdf            # Sample encrypted PDF
│   ├── test.docx           # Sample encrypted Word document
│   ├── test.xlsx           # Sample encrypted Excel file
│   └── test.pptx           # Sample encrypted PowerPoint
│
├── wordlists/             # Password dictionaries
│   ├── common-passwords.txt
│   ├── chinese-common.txt
│   ├── pins-4digit.txt
│   └── pins-6digit.txt
│
└── docs/                  # Documentation
    ├── PROJECT_STRUCTURE.md    # This file
    ├── PERFORMANCE.md          # Performance benchmarks
    └── ZIP_IMPLEMENTATION.md   # ZIP encryption implementation details
```

## Key Components

### 1. Core Engine (`src/cracker/`)

- Multi-threaded password testing using Rayon
- Work-stealing parallelism for optimal CPU usage
- Real-time progress tracking with indicatif
- Batch processing for efficiency

### 2. File Format Support (`src/formats/`)

#### ZIP (`archive.rs`)
- Traditional ZipCrypto encryption
- AES encryption support
- Proper CRC32 validation
- Cross-platform compatibility

#### PDF (`pdf.rs`)
- 40-bit RC4 (PDF 1.3)
- 128-bit RC4 (PDF 1.4)
- AES-128 (PDF 1.6)
- AES-256 (PDF 2.0)

#### Office (`office.rs`)
- Office 2007+ (ECMA-376)
- AES encryption
- PBKDF2 key derivation

### 3. Password Generators (`src/generator/`)

#### Brute Force
- Configurable character sets
- Length range support
- Efficient iteration
- Total combination calculation

#### Dictionary
- File-based wordlists
- Directory support (recursively loads .txt files)
- Line-by-line streaming
- Memory efficient
- Automatic deduplication

### 4. Examples Directory (`examples/`)

Contains test files and utilities for tool validation:
- **UV Python Project**: Modern Python environment management
- **Test File Generator**: Creates password-protected PDF, Word, Excel, and PowerPoint files
- **Sample Files**: Pre-created test files with password "92eo"
- **Documentation**: Setup and usage instructions

Uses UV for dependency management - no requirements.txt or manual venv needed.

### 5. Command Line Interface (`src/main.rs`)

Built with clap v4:
- Subcommands: dictionary, brute-force, hybrid
- Global options: file, threads, performance
- Help and version information
- Input validation

## Architecture Decisions

1. **Rust**: Chosen for memory safety and performance
2. **Rayon**: Work-stealing parallelism scales automatically
3. **Channels**: Bounded channels prevent memory overflow
4. **Batch Processing**: Reduces synchronization overhead
5. **Progress Bars**: Non-blocking updates for smooth UI

## Performance Characteristics

- **ZIP (Traditional)**: 40,000+ passwords/second
- **ZIP (AES)**: 10,000+ passwords/second
- **PDF**: 1,000-5,000 passwords/second
- **Office**: 100-1,000 passwords/second

Performance varies based on:
- CPU cores and speed
- Password complexity
- File encryption type
- System memory

## Future Enhancements

1. GPU acceleration for suitable algorithms
2. Network distributed cracking
3. More file formats (RAR5, 7z)
4. Advanced rule-based mutations
5. Machine learning password generation