# Test Files for Password Cracking Tool

This directory contains test files for the password cracking tool. All test files use the password: **92eo**

## Existing Test Files

| File | Format | Encryption |
|------|--------|-----------|
| `test.zip` | ZIP | ZipCrypto |
| `test.pdf` | PDF | RC4-128 |
| `test.docx` | Word | Agile AES-256 |
| `test.xlsx` | Excel | Agile AES-256 |
| `test.pptx` | PowerPoint | Agile AES-256 |

## Testing with the Cracking Tool

```bash
# Build the tool first
cd .. && cargo build --release && cd examples

# Test all formats with dictionary attack
../target/release/password-cracking -f test.zip dictionary -w ../wordlists/common-passwords.txt
../target/release/password-cracking -f test.pdf dictionary -w ../wordlists/common-passwords.txt
../target/release/password-cracking -f test.docx dictionary -w ../wordlists/common-passwords.txt
../target/release/password-cracking -f test.xlsx dictionary -w ../wordlists/common-passwords.txt
../target/release/password-cracking -f test.pptx dictionary -w ../wordlists/common-passwords.txt

# Brute-force (finds "92eo" in alnum 1-4)
../target/release/password-cracking -f test.zip brute-force -c alnum --min-length 1 --max-length 4

# Brute-force digits (won't find "92eo" — it has letters)
../target/release/password-cracking -f test.pdf brute-force -c digits --min-length 1 --max-length 6

# Hybrid with rules
../target/release/password-cracking -f test.zip hybrid -w ../wordlists/common-passwords.txt --capitalize --l33t

# Quiet mode (scripting)
../target/release/password-cracking -q -f test.zip dictionary -w ../wordlists/common-passwords.txt

# Load all wordlists from directory
../target/release/password-cracking -f test.zip dictionary -w ../wordlists/
```

## Creating Additional Test Files

We use UV for Python environment management.

### Prerequisites

Install UV:
```bash
# macOS/Linux
curl -LsSf https://astral.sh/uv/install.sh | sh

# Windows
powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
```

### Generate Test Files

```bash
cd examples
uv sync              # install dependencies
uv run python create_test_files.py
```

This creates/overwrites: `test.pdf`, `test.docx`, `test.xlsx`, `test.pptx` (all encrypted with password **92eo**).

### Manual Encryption

If automatic encryption fails, add passwords manually in Microsoft Office:
1. Open the file in Office
2. File → Info → Protect Document → Encrypt with Password
3. Enter: **92eo**

Or use LibreOffice: File → Save As → check "Save with password" → enter **92eo**

## UV Commands

```bash
uv sync              # install dependencies
uv add package-name  # add a dependency
uv run python script.py  # run a script
```
