# Test Fixtures

**English** | [中文](README.zh-CN.md)

Encrypted files used by the e2e tests and for manual checks. The password is
`92eo` unless the table says otherwise.

| File | Format | Encryption or purpose |
|------|--------|-----------------------|
| `test.zip` | ZIP | ZipCrypto |
| `test_aes.zip` | ZIP | AES-256, created with 7-Zip |
| `test_multi.zip` | ZIP | Two encrypted entries with the same password |
| `test_mixed.zip` | ZIP | First entry unencrypted, second encrypted |
| `test.pdf` | PDF | RC4-128 (R3) |
| `test_aes128.pdf` | PDF | AES-128 (R4) |
| `test_r6.pdf` | PDF | AES-256 (R6, Algorithm 2.B) |
| `test_xref.pdf` | PDF | AES-256 R6 with an xref stream |
| `test_owner_r4.pdf` | PDF | AES-128, owner password only |
| `test_owner_r6.pdf` | PDF | AES-256, owner password only |
| `test.docx` | Word | Agile AES-256 |
| `test.xlsx` | Excel | Agile AES-256 |
| `test.pptx` | PowerPoint | Agile AES-256 |
| `test_unicode.docx` | Word | Agile AES-256, password `密码92eo` |

Owner-only PDFs need `--pdf-owner`. The Unicode fixture covers UTF-16LE
password encoding.

## Run the fixtures

```bash
cargo build --release

for f in test.zip test_aes.zip test_multi.zip test_mixed.zip \
         test.pdf test_aes128.pdf test_r6.pdf test_xref.pdf \
         test.docx test.xlsx test.pptx; do
    printf '%-20s ' "$f"
    ./target/release/password-cracking -q -f "examples/$f" \
        dictionary -w wordlists/common-passwords.txt
done

for f in test_owner_r4.pdf test_owner_r6.pdf; do
    printf '%-20s ' "$f"
    ./target/release/password-cracking -q -f "examples/$f" \
        dictionary -w wordlists/common-passwords.txt --pdf-owner
done
```

Other attack modes:

```bash
./target/release/password-cracking -f examples/test.zip \
    brute-force -c alnum --min-length 1 --max-length 4

./target/release/password-cracking -f examples/test.zip mask -m "?d?d?l?l"

./target/release/password-cracking -f examples/test.zip hybrid \
    -w wordlists/common-passwords.txt --capitalize --l33t
```

The e2e suite runs with `cargo test --test e2e`.

## Regenerate

The Python generator uses [uv](https://docs.astral.sh/uv/):

```bash
cd examples
uv sync
uv run python create_test_files.py
```

`test_r6.pdf` stays unchanged because it backs a known-answer test for
Algorithm 2.B. `test_aes.zip` needs 7-Zip:

```bash
printf 'payload\n' > payload.txt
7z a -tzip -p92eo -mem=AES256 test_aes.zip payload.txt
```

`test_multi.zip` and `test_mixed.zip` use Info-ZIP `zip`. `test_unicode.docx`
uses `密码92eo` and the `msoffcrypto-tool` Python package. If you regenerate a
fixture with different parameters, update the table and the e2e tests.
