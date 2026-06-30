# User Guide

Complete usage guide for the Password Cracking Tool.

## Table of Contents

1. [Installation](#installation)
2. [Quick Start](#quick-start)
3. [File Format Support](#file-format-support)
4. [Attack Modes](#attack-modes)
   - [Dictionary Attack](#dictionary-attack)
   - [Brute-Force Attack](#brute-force-attack)
   - [Hybrid Attack](#hybrid-attack)
5. [Global Options](#global-options)
6. [Character Sets](#character-sets)
7. [Mutation Rules](#mutation-rules)
8. [Wordlists](#wordlists)
9. [Quiet Mode (Scripting)](#quiet-mode-scripting)
10. [Performance Tips](#performance-tips)
11. [Real-World Examples](#real-world-examples)

---

## Installation

```bash
git clone https://github.com/LiangLliu/password-cracking
cd password-cracking
cargo build --release
```

The binary is at `./target/release/password-cracking`.

Optional: copy to your PATH:

```bash
cp ./target/release/password-cracking /usr/local/bin/
```

---

## Quick Start

All test files in `examples/` use the password `92eo`.

```bash
# Crack a ZIP file with the bundled wordlist
./target/release/password-cracking -f examples/test.zip dictionary -w wordlists/common-passwords.txt
```

Output:

```
Password Cracker
================
Target: examples/test.zip

Format:  ZIP (ZipCrypto)
Attack:  dictionary
Threads: 10
Keyspace: 33

  [bar:40.cyan/blue] 33/33 (100%) 314/s ETA:0s ✓ Found: 92eo

Result
======
Duration: 0s
Attempts: 33
Speed:    314 passwords/sec

Password found: 92eo
```

---

## File Format Support

The tool auto-detects the file format by reading magic bytes (not just the extension).

| Format | Extensions | Encryption Types | Speed |
|--------|-----------|-----------------|-------|
| ZIP | `.zip` | ZipCrypto, AES-128/192/256 | ~7M/sec |
| PDF | `.pdf` | RC4-40, RC4-128, AES-128, AES-256 | ~500K/sec |
| Office | `.docx`, `.xlsx`, `.pptx`, `.doc`, `.xls`, `.ppt` | Agile AES-256 (SHA-512) | ~400/sec |

You don't need to specify the format — just pass `-f <file>` and the tool figures it out.

---

## Attack Modes

### Dictionary Attack

Tries each password from a wordlist file (one per line). Supports single files and directories.

```bash
# Single wordlist file
password-cracking -f document.zip dictionary -w passwords.txt

# Directory of .txt files (loads all recursively, deduplicates)
password-cracking -f document.zip dictionary -w ./wordlists/

# Use the bundled common passwords list
password-cracking -f document.pdf dictionary -w wordlists/common-passwords.txt
```

**Wordlist format**: one password per line. Lines starting with `#` are treated as comments and skipped. Empty lines are ignored.

```
# my-wordlist.txt
password
123456
admin
letmein
```

### Brute-Force Attack

Generates every possible combination of characters up to the specified length.

```bash
# 4-digit PIN (0000-9999)
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 4

# 1-6 digit numbers
password-cracking -f document.zip brute-force -c digits --min-length 1 --max-length 6

# Lowercase letters, 1-4 characters
password-cracking -f document.zip brute-force -c lower --min-length 1 --max-length 4

# Full alphanumeric, 1-6 characters (62^6 = 56 billion combinations)
password-cracking -f document.zip brute-force -c alnum --min-length 1 --max-length 6

# Custom character set
password-cracking -f document.zip brute-force -c "abc123!@#" --min-length 1 --max-length 8

# Everything: digits + letters + special chars
password-cracking -f document.zip brute-force -c all --min-length 1 --max-length 4
```

**Keyspace sizes** (for estimating time):

| Charset | Length 4 | Length 6 | Length 8 |
|---------|---------|---------|---------|
| digits (10) | 10,000 | 1M | 100M |
| lower (26) | 457K | 308M | 209B |
| alnum (62) | 15M | 56B | 218T |
| all (94) | 78M | 690B | 6,096T |

### Hybrid Attack

Applies mutation rules to dictionary words. For each word, generates the original plus all rule variants.

```bash
# Capitalize + l33t-speak
password-cracking -f document.zip hybrid -w wordlists/common-passwords.txt --capitalize --l33t

# Append digits 0-99 to each word
password-cracking -f document.zip hybrid -w wordlists/common-passwords.txt --append-digits 99

# All rules at once
password-cracking -f document.zip hybrid -w wordlists/common-passwords.txt \
  --capitalize --upper --lower --l33t --reverse --duplicate --append-digits 99
```

See [Mutation Rules](#mutation-rules) for details.

---

## Global Options

| Option | Short | Description |
|--------|-------|-------------|
| `--file <PATH>` | `-f` | Target file to crack (required) |
| `--threads <N>` | `-t` | Number of worker threads (default: all logical cores) |
| `--quiet` | `-q` | Only output the password (or "not found" to stderr) |
| `--help` | `-h` | Show help |
| `--version` | `-V` | Show version |

```bash
# Use only 4 threads
password-cracking -f document.zip -t 4 dictionary -w passwords.txt

# Quiet mode — prints just the password, nothing else
password-cracking -q -f document.zip dictionary -w passwords.txt
```

---

## Character Sets

Predefined sets for brute-force attacks:

| Name | Characters | Count |
|------|-----------|-------|
| `digits` | `0123456789` | 10 |
| `lower` | `abcdefghijklmnopqrstuvwxyz` | 26 |
| `upper` | `ABCDEFGHIJKLMNOPQRSTUVWXYZ` | 26 |
| `special` | `!@#$%^&*()-_=+[]{}\|;:'",.<>/?\`~` | 32 |
| `alnum` | digits + lower + upper | 62 |
| `all` | digits + lower + upper + special | 94 |

You can also pass any custom string as the charset:

```bash
# Only hex characters
password-cracking -f document.zip brute-force -c "0123456789abcdef" --min-length 1 --max-length 8
```

---

## Mutation Rules

Available rules for hybrid attacks:

| Rule | Flag | Example (input: `password`) |
|------|------|---------------------------|
| Capitalize | `--capitalize` | `Password` |
| Uppercase | `--upper` | `PASSWORD` |
| Lowercase | `--lower` | `password` |
| L33t-speak | `--l33t` | `p@$$w0rd` |
| Reverse | `--reverse` | `drowssap` |
| Duplicate | `--duplicate` | `passwordpassword` |
| Append digits | `--append-digits N` | `password0`, `password1`, ..., `passwordN` |

L33t-speak substitutions: `a→@`, `e→3`, `i→1`, `o→0`, `s→$`

Each dictionary word produces: the original + one variant per active rule. With `--append-digits 99`, each word also generates 100 additional variants (word+0 through word+99).

**Keyspace estimate**: if your wordlist has 1,000 words and you use `--capitalize --l33t --append-digits 99`, the keyspace is approximately `1000 × (1 + 2 + 100) = 103,000` candidates.

---

## Wordlists

The tool ships with several wordlists in the `wordlists/` directory:

| File | Contents | Count |
|------|----------|-------|
| `common-passwords.txt` | Most common passwords | 33 |
| `chinese-common.txt` | Common Chinese passwords | 40 |
| `pins-4digit.txt` | All 4-digit PINs (0000-9999) | 10,000 |
| `pins-6digit.txt` | All 6-digit PINs (000000-999999) | 1,000,000 |

**Loading a directory**: if you pass a directory path, all `.txt` files inside are loaded recursively and deduplicated:

```bash
password-cracking -f document.zip dictionary -w wordlists/
```

**Creating your own wordlist**: any text file with one password per line works. Lines starting with `#` are comments.

---

## Quiet Mode (Scripting)

Use `-q` for scripting — outputs only the password on stdout:

```bash
# Capture password in a variable
PASSWORD=$(password-cracking -q -f document.zip dictionary -w passwords.txt)
echo "Password: $PASSWORD"

# Use in a script with error handling
if PASSWORD=$(password-cracking -q -f document.zip dictionary -w passwords.txt 2>/dev/null); then
    echo "Success: $PASSWORD"
else
    echo "Not found"
fi
```

In quiet mode, if the password is found it's printed to stdout. If not found, "not found" is printed to stderr.

---

## Performance Tips

1. **Start with dictionary attacks** — much faster than brute-force
2. **Limit brute-force length** — keyspace grows exponentially
3. **Use the smallest charset that could work** — digits-only is 10x faster than alnum
4. **Let it use all cores** — don't limit threads unless needed
5. **For Office files, prefer dictionary/hybrid** — brute-force is very slow (~400/sec) due to 100K SHA-512 iterations per password

**Time estimates** (8-core CPU, ZIP ZipCrypto at 7M/sec):

| Charset | Length 4 | Length 6 | Length 8 |
|---------|---------|---------|---------|
| digits | instant | instant | 14 sec |
| lower | instant | 44 sec | 8.2 hours |
| alnum | 2 sec | 2.2 hours | 74 days |
| all | 11 sec | 27 hours | 28 years |

---

## Real-World Examples

### Recovering a forgotten ZIP password

You remember it was a 4-letter lowercase word:

```bash
password-cracking -f backup.zip brute-force -c lower --min-length 4 --max-length 4
```

You remember it started with "pass" and had 2 more characters:

```bash
# Custom charset, try pass + 2 alphanumeric chars
# (brute-force will try all combos; you can also make a targeted wordlist)
password-cracking -f backup.zip brute-force -c "pass0123456789abcdefghijklmnopqrstuvwxyz" \
  --min-length 6 --max-length 6
```

### Cracking a PDF with a PIN

PDFs are often protected with simple numeric PINs:

```bash
# Try all 4-digit PINs
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 4

# Try 4-6 digit PINs
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 6
```

### Office document with common passwords

Office Agile encryption is slow (~400/sec), so always try dictionary first:

```bash
# Try common passwords
password-cracking -f report.docx dictionary -w wordlists/common-passwords.txt

# Try with mutations (capitalize, append year digits)
password-cracking -f report.docx hybrid -w wordlists/common-passwords.txt \
  --capitalize --append-digits 99
```

### Batch cracking with a script

```bash
#!/bin/bash
# crack-all.sh — try to crack all encrypted files in a directory
for file in ~/documents/*.{zip,pdf,docx,xlsx,pptx}; do
    [ -f "$file" ] || continue
    echo "Cracking: $file"
    password-cracking -q -f "$file" dictionary -w wordlists/ 2>/dev/null && \
        echo "  -> Found!" || echo "  -> Not found, trying brute-force..."
done
```

### Using the bundled test files

```bash
# Generate test files (requires Python + UV)
cd examples && uv run python create_test_files.py && cd ..

# Test all formats
for fmt in zip pdf docx xlsx pptx; do
    echo "=== $fmt ==="
    ./target/release/password-cracking -f examples/test.$fmt \
        dictionary -w wordlists/common-passwords.txt 2>&1 | grep "found"
done
```
