# Test Files for Password Cracking Tool

This directory contains test files for the password cracking tool. All test files use the password: **92eo**

## Existing Test Files

- `test.zip` - Password-protected ZIP file (password: 92eo)

## Creating Additional Test Files

We use UV for Python environment management and a script to generate password-protected PDF and Office files.

### Prerequisites

Install UV if you haven't already:
```bash
# On macOS/Linux
curl -LsSf https://astral.sh/uv/install.sh | sh

# On Windows
powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
```

### Setup and Run

```bash
cd examples

# First time - install dependencies
uv sync

# Create test files
uv run python create_test_files.py
```

This will create:
- `test.pdf` - Password-protected PDF file (automatic encryption)
- `test.docx` - Word document (automatic encryption)
- `test.xlsx` - Excel spreadsheet (automatic encryption)
- `test.pptx` - PowerPoint presentation (automatic encryption)

All files are automatically encrypted with password: **92eo**

### Project Structure

- `pyproject.toml` - UV project configuration with dependencies
- `create_test_files.py` - Script to generate test files
- `.venv/` - Virtual environment (created automatically by UV)

### Dependencies

The project uses these Python packages (managed by UV):
- **PyPDF2** & **reportlab** - For creating password-protected PDFs
- **python-docx** - For creating Word documents
- **openpyxl** - For creating Excel spreadsheets
- **python-pptx** - For creating PowerPoint presentations
- **msoffcrypto-tool** - For encrypting Office files

### Manual Office File Encryption (if needed)

If automatic encryption fails, you can add passwords manually:

#### On Windows/macOS with Microsoft Office:
1. Open the file in Microsoft Office
2. Go to File → Info → Protect Document/Workbook/Presentation
3. Choose "Encrypt with Password"
4. Enter password: **92eo**

#### Using LibreOffice (Cross-platform):
1. Open the file in LibreOffice
2. Go to File → Save As
3. Check "Save with password"
4. Enter password: **92eo**

## Testing Files

After creating password-protected files, test them with the password cracking tool:

```bash
# Test ZIP file
../target/release/password-cracking -f test.zip dictionary -w ../wordlists/common-passwords.txt

# Test PDF file
../target/release/password-cracking -f test.pdf dictionary -w ../wordlists/common-passwords.txt

# Test Office files
../target/release/password-cracking -f test.docx dictionary -w ../wordlists/common-passwords.txt
../target/release/password-cracking -f test.xlsx dictionary -w ../wordlists/common-passwords.txt
../target/release/password-cracking -f test.pptx dictionary -w ../wordlists/common-passwords.txt

# Or use brute force for a 4-character password
../target/release/password-cracking -f test.pdf brute-force -c "0123456789abcdefghijklmnopqrstuvwxyz" --min-length 4 --max-length 4
```

## Quick Test

Since the password "92eo" is included in `../wordlists/common-passwords.txt`, dictionary attacks should find it quickly!

## UV Commands Reference

```bash
# Install/update dependencies
uv sync

# Add a new dependency
uv add package-name

# Run a script
uv run python script.py

# Run Python REPL with project dependencies
uv run python

# Update all dependencies
uv sync --upgrade
```