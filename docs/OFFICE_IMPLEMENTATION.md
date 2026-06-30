# Office Implementation Details

## Encryption Methods

Office documents use two container formats:

| Container | Extensions | Encryption |
|-----------|-----------|------------|
| OLE2 (CFB) | `.doc`, `.xls`, `.ppt` (old) | ECMA-376 or Agile |
| OOXML (ZIP) | `.docx`, `.xlsx`, `.pptx` (2007+) | Agile (when encrypted) |

When an OOXML file is encrypted, it's wrapped in an OLE2 compound file containing:
- `EncryptionInfo` — encryption parameters
- `EncryptedPackage` — the encrypted OOXML content

### Agile Encryption (v4.0, XML-based)

Used by Office 2010+. This is the only version currently supported.

Parameters (from the `EncryptionInfo` XML):
- `spinCount`: number of hash iterations (typically 100,000)
- `keyBits`: AES key size (128/192/256, typically 256)
- `hashAlgorithm`: SHA-1/256/384/512 (typically SHA-512)
- `saltValue`: random salt (base64-encoded)
- `encryptedVerifierHashInput`: encrypted verifier input
- `encryptedVerifierHashValue`: encrypted verifier hash

## Password Verification Algorithm

Reference: [MS-OFFCRYPTO 2.3.7.1](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-offcrypto/)

### Step 1: Derive Iterated Hash

```
H = hash(salt + password_utf16le)
for i in 0..spinCount:
    H = hash(i_as_4_bytes_le + H)
```

Note the order: `salt + password` (not `password + salt`), and `i + H` (not `H + i`).

### Step 2: Derive Per-Purpose Keys

Two separate keys are derived using block key constants:

```
key1 = hash(H + blkKey_VerifierHashInput)[:keyBits/8]
key2 = hash(H + blkKey_EncryptedVerifierHashValue)[:keyBits/8]
```

Block key constants:
```rust
const BLK_KEY_VERIFIER_HASH_INPUT: [u8; 8] = [0xFE, 0xA7, 0xD2, 0x76, 0x3B, 0x4B, 0x9E, 0x79];
const BLK_KEY_ENCRYPTED_VERIFIER_HASH_VALUE: [u8; 8] = [0xD7, 0xAA, 0x0F, 0x6D, 0x30, 0x61, 0x34, 0x4E];
```

### Step 3: Decrypt and Compare

1. `verifier_input = AES_CBC_decrypt(key1, salt, encryptedVerifierHashInput)`
2. `computed_hash = hash(verifier_input)`
3. `stored_hash = AES_CBC_decrypt(key2, salt, encryptedVerifierHashValue)`
4. Password is correct if `computed_hash[:hashSize] == stored_hash[:hashSize]`

**Important**: The IV for AES-CBC is the salt value itself (not zeros), and there is no PKCS#7 padding — the encrypted data is already block-aligned.

## OLE2 Parsing

The `cfb` crate reads OLE2 compound files:

```rust
let file = File::open(path)?;
let mut comp = cfb::CompoundFile::open(file)?;
let mut stream = comp.open_stream("EncryptionInfo")?;
std::io::Read::read_to_end(&mut stream, &mut enc_info)?;
```

The `EncryptionInfo` stream has an 8-byte binary header followed by XML:
- Bytes 0-1: version (4 for Agile)
- Bytes 2-3: format version (4 for Agile)
- Bytes 4-7: XML length (unused, XML follows directly)
- Bytes 8+: XML content

## XML Parsing

A lightweight attribute extractor is used (no full XML parser):

```rust
fn extract_attr(xml: &str, attr: &str) -> Result<String> {
    let pattern = format!("{attr}=\"");
    // find attr="value" pattern
}
```

This works because the Agile Encryption XML has a fixed, well-known structure with no nested elements or namespaces that would require a real XML parser.

## AES-CBC Decryption

Raw AES-CBC without padding removal (the encrypted data is already block-aligned):

```rust
fn aes_cbc_decrypt_raw(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    // ECB decrypt each block, then XOR with previous ciphertext block (or IV for first)
    for chunk in ciphertext.chunks(16) {
        let mut block = chunk;
        cipher.decrypt_block(&mut block);  // ECB
        for i in 0..16 { block[i] ^= prev[i]; }  // CBC XOR
        prev = saved_ciphertext;
    }
}
```

Supports AES-128, AES-192, and AES-256 via a macro that generates type-specific code for each key size.

## Performance

Office Agile encryption is slow because each password verification requires:
- 100,000 SHA-512 hash iterations
- 2 AES-256-CBC decryptions
- 1 SHA-512 hash

On a 10-core CPU: ~400 passwords/sec. This is inherent to the algorithm — the 100K iterations are a deliberate key stretching measure to slow down brute-force attacks.

**Recommendation**: Always use dictionary or hybrid attacks for Office files. Brute-force is only practical for very short numeric PINs.

## Testing

Test files are created with `msoffcrypto-tool`:

```python
from msoffcrypto.format.ooxml import OOXMLFile
officefile = OOXMLFile(f)
officefile.encrypt("92eo", output)
```

This produces OLE2-wrapped encrypted files with Agile AES-256 + SHA-512.
