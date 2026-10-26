# Office Internals

**English** | [中文](OFFICE_IMPLEMENTATION.zh-CN.md)

`src/formats/office.rs` verifies passwords for Agile-encrypted Office files
(Office 2010+). Legacy CryptoAPI/ECMA-376 standard encryption is not supported.

## Containers

| Container | Extensions | Encryption |
|-----------|------------|------------|
| OLE2 (CFB) | `.doc`, `.xls`, `.ppt` (old) | Agile or legacy |
| OOXML (ZIP) | `.docx`, `.xlsx`, `.pptx` (2007+) | Agile, wrapped in OLE2 when encrypted |

An encrypted OOXML file is an OLE2 compound file containing:

- `EncryptionInfo` — version header plus Agile XML
- `EncryptedPackage` — the encrypted OOXML ZIP

The verifier only needs `EncryptionInfo`; it never decrypts the package.

## Agile encryption parameters

The XML lives at byte 8 of the `EncryptionInfo` stream:

```text
bytes 0-1: version (4 = Agile)
bytes 2-3: reserved
bytes 4-7: reserved/flags
bytes 8+ : UTF-8 XML (p:encryption)
```

Relevant attributes from `p:encryptedKey`:

| Attribute | Meaning |
|-----------|---------|
| `spinCount` | hash iterations (typically 100,000) |
| `keyBits` | AES key size: 128/192/256 |
| `hashSize` | hash output size in bytes |
| `blockSize` | AES block size (must be 16) |
| `hashAlgorithm` | SHA-1/256/384/512 |
| `saltValue` | base64 salt |
| `encryptedVerifierHashInput` | base64 encrypted verifier input |
| `encryptedVerifierHashValue` | base64 encrypted verifier hash |

`spinCount` is capped at 5,000,000. Real files use 100,000; a corrupt or
malicious file requesting billions of iterations is rejected instead of
hanging the cracker.

## Password verification

Reference: [MS-OFFCRYPTO 2.3.7.1](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-offcrypto/).

### 1. Iterated hash

```text
H = hash(salt || password_utf16le)
for i in 0..spinCount:
    H = hash(i_as_4_bytes_le || H)
```

The order matters: salt first, then password; counter first, then the previous
hash. Reversing either produces a valid-looking implementation that never
matches.

### 2. Per-purpose keys

```text
key1 = hash(H || blkKey_VerifierHashInput)[:keyBits/8]
key2 = hash(H || blkKey_EncryptedVerifierHashValue)[:keyBits/8]
```

Block-key constants:

```rust
const BLK_KEY_VERIFIER_HASH_INPUT: [u8; 8] =
    [0xFE, 0xA7, 0xD2, 0x76, 0x3B, 0x4B, 0x9E, 0x79];
const BLK_KEY_ENCRYPTED_VERIFIER_HASH_VALUE: [u8; 8] =
    [0xD7, 0xAA, 0x0F, 0x6D, 0x30, 0x61, 0x34, 0x4E];
```

### 3. Decrypt and compare

```text
verifier_input = AES-CBC-Decrypt(key1, iv=salt, encryptedVerifierHashInput)
computed       = hash(verifier_input)
stored         = AES-CBC-Decrypt(key2, iv=salt, encryptedVerifierHashValue)
match          = computed[:hashSize] == stored[:hashSize]
```

The IV is the salt, not zero. The encrypted data is block-aligned; no PKCS#7
padding is applied.

## OLE2 and XML parsing

- `cfb` opens the compound file and reads the `EncryptionInfo` stream.
- A small attribute extractor finds `name="value"` pairs in the fixed Agile
  XML. A full XML parser is unnecessary because the schema is flat and
  namespace-free in the encrypted-key element.
- Base64 values are decoded with the `base64` crate.

## AES-CBC

`aes_cbc_decrypt_raw` performs AES decryption per block (ECB) and XORs with the
previous ciphertext block, using the salt as the IV for the first block. AES-128,
AES-192 and AES-256 are selected by key length.

## Performance

Agile verification is intentionally expensive:

- ~100,000 hash iterations (SHA-512 in typical files)
- two AES-CBC decryptions
- one hash of the verifier input

Measured throughput: **~585 passwords/sec** on a 10-core M2 Pro (500–800/s
depending on `spinCount` and hash algorithm). This is why dictionary and hybrid
attacks are strongly preferred for Office files.

## Testing

Fixtures are generated with `msoffcrypto-tool`:

```python
from msoffcrypto.format.ooxml import OOXMLFile

with open("test.docx", "rb") as f, open("encrypted.docx", "wb") as out:
    OOXMLFile(f).encrypt("92eo", out)
```

The resulting files are OLE2-wrapped OOXML with Agile AES-256 and SHA-512.

Fixtures: `test.docx`, `test.xlsx`, `test.pptx` (all password `92eo`).

## Limitations

- **Agile only (v4)**: legacy CryptoAPI/ECMA-376 `.doc`/`.xls`/`.ppt` files
  are rejected with a clear error.
- **Non-ASCII passwords**: candidates that are valid UTF-8 are decoded and
  encoded as UTF-16LE, so Chinese and other Unicode passwords work
  (`test_unicode.docx` covers this). Candidates that are not valid UTF-8 fall
  back to byte-wise UTF-16LE encoding.
- **EncryptedPackage** is not decrypted; the verifier hash is authoritative.
- Only the password verifier is checked. Integrity of the decrypted package is
  not validated (and is not needed to recover a password).
