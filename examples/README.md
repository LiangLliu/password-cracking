# Test Files for Password Cracking Tool

This directory contains test files for the password cracking tool. All test files use the password: **92eo**

## Existing Test Files

| File | Format | Encryption |
|------|--------|-----------|
| `test.zip` | ZIP | ZipCrypto |
| `test_multi.zip` | ZIP | Two encrypted entries (same password) |
| `test_mixed.zip` | ZIP | First entry unencrypted, second encrypted |
| `test.pdf` | PDF | RC4-128 (R3) |
| `test_aes128.pdf` | PDF | AES-128 (R4) |
| `test_r6.pdf` | PDF | AES-256 (R6, Algorithm 2.B) |
| `test_xref.pdf` | PDF | AES-256 (R6, xref stream, no classic trailer) |
| `test_owner_r4.pdf` | PDF | AES-128 (R4), owner password only |
| `test_owner_r6.pdf` | PDF | AES-256 (R6), owner password only |
| `test.docx` | Word | Agile AES-256 |
| `test.xlsx` | Excel | Agile AES-256 |
| `test.pptx` | PowerPoint | Agile AES-256 |

## Testing with the Cracking Tool

```bash
# Build the tool first
cd .. && cargo build --release && cd examples

# Test all formats with dictionary attack
../target/release/password-cracking -f test.zip dictionary -w ../wordlists/common-passwords.txt
../target/release/password-cracking -f test_multi.zip dictionary -w ../wordlists/common-passwords.txt
../target/release/password-cracking -f test_mixed.zip dictionary -w ../wordlists/common-passwords.txt
../target/release/password-cracking -f test.pdf dictionary -w ../wordlists/common-passwords.txt
../target/release/password-cracking -f test_aes128.pdf dictionary -w ../wordlists/common-passwords.txt
../target/release/password-cracking -f test_r6.pdf dictionary -w ../wordlists/common-passwords.txt
../target/release/password-cracking -f test_xref.pdf dictionary -w ../wordlists/common-passwords.txt
# Owner-password-only files (user password is unknown):
../target/release/password-cracking -f test_owner_r4.pdf dictionary -w ../wordlists/common-passwords.txt
../target/release/password-cracking -f test_owner_r6.pdf dictionary -w ../wordlists/common-passwords.txt
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

This creates/overwrites: `test.pdf`, `test_aes128.pdf`, `test_xref.pdf`, `test.docx`, `test.xlsx`, `test.pptx` (all encrypted with password **92eo**). `test_r6.pdf` is committed and not regenerated because it backs a known-answer test.

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
