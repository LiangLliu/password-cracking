# Test Fixtures

**English** | [中文](README.zh-CN.md)

Encrypted files used by the e2e tests and for manual checks. Unless noted, the
password is **`92eo`**.

| File | Format | Encryption / purpose |
|------|--------|----------------------|
| `test.zip` | ZIP | ZipCrypto |
| `test_aes.zip` | ZIP | AES-256 (7-Zip) |
| `test_multi.zip` | ZIP | Two encrypted entries, same password |
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
| `test_unicode.docx` | Word | Agile AES-256, password **`密码92eo`** |

Owner-only PDFs have an unknown user password; pass `--pdf-owner`. The Unicode
Office fixture exists to cover UTF-16LE password encoding.

## Running against the fixtures

```bash
# Build first
cargo build --release

# All standard fixtures
for f in test.zip test_aes.zip test_multi.zip test_mixed.zip \
         test.pdf test_aes128.pdf test_r6.pdf test_xref.pdf \
         test.docx test.xlsx test.pptx; do
    printf '%-20s ' "$f"
    ./target/release/password-cracking -q -f "examples/$f" \
        dictionary -w wordlists/common-passwords.txt
done

# Owner-password PDFs
for f in test_owner_r4.pdf test_owner_r6.pdf; do
    printf '%-20s ' "$f"
    ./target/release/password-cracking -q -f "examples/$f" \
        dictionary -w wordlists/common-passwords.txt --pdf-owner
done

# Unicode Office password
printf '密码92eo\n' > /tmp/unicode.txt
./target/release/password-cracking -q -f examples/test_unicode.docx \
    dictionary -w /tmp/unicode.txt
```

Other attack modes:

```bash
# Brute-force: "92eo" is in alnum 1-4
./target/release/password-cracking -f examples/test.zip \
    brute-force -c alnum --min-length 1 --max-length 4

# Mask: digit digit lower lower
./target/release/password-cracking -f examples/test.zip mask -m "?d?d?l?l"

# Hybrid
./target/release/password-cracking -f examples/test.zip hybrid \
    -w wordlists/common-passwords.txt --capitalize --l33t

# Directory of wordlists
./target/release/password-cracking -f examples/test.zip dictionary -w wordlists/
```

The e2e suite:

```bash
cargo test --test e2e
```

## Regenerating fixtures

The Python generator uses [uv](https://docs.astral.sh/uv/):

```bash
cd examples
uv sync
uv run python create_test_files.py
```

It regenerates `test.pdf`, `test_aes128.pdf`, `test_xref.pdf`,
`test_owner_r4.pdf`, `test_owner_r6.pdf`, `test.docx`, `test.xlsx` and
`test.pptx`. Some fixtures are intentionally not regenerated:

- `test_r6.pdf` backs a known-answer test for Algorithm 2.B.
- `test_aes.zip` needs 7-Zip (AES support):
  ```bash
  printf 'payload\n' > payload.txt
  7z a -tzip -p92eo -mem=AES256 test_aes.zip payload.txt
  ```
- `test_multi.zip` / `test_mixed.zip` need Info-ZIP `zip`:
  ```bash
  printf 'alpha\n' > a.txt; printf 'bravo\n' > b.txt
  zip -P 92eo test_multi.zip a.txt b.txt
  zip test_mixed.zip a.txt
  zip -P 92eo test_mixed.zip b.txt
  ```
- `test_unicode.docx` uses a different password:
  ```python
  import docx
  from msoffcrypto.format.ooxml import OOXMLFile

  doc = docx.Document()
  doc.add_paragraph("unicode password test")
  doc.save("tmp.docx")
  with open("tmp.docx", "rb") as f, open("test_unicode.docx", "wb") as out:
      OOXMLFile(f).encrypt("密码92eo", out)
  ```

## Requirements for regeneration

- Python 3.12+
- `uv`
- `pikepdf`, `python-docx`, `openpyxl`, `python-pptx`, `msoffcrypto-tool`
  (installed by `uv sync` from `pyproject.toml`)
- `7z` for the AES ZIP
- `zip` for the multi-entry fixtures

## Notes

- Fixtures are small and committed so `cargo test` works without Python.
- All fixtures are generated for authorized testing only.
- If a fixture is regenerated with different parameters, the e2e tests and the
  documented passwords must be updated together.
