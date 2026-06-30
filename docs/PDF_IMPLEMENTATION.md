# PDF Implementation Details

## Encryption Algorithms

The PDF standard supports 4 encryption algorithms, identified by the `/V` and `/R` values in the `/Encrypt` dictionary:

| V | R | Algorithm | Key Size | PDF Version |
|---|---|-----------|----------|-------------|
| 1 | 2 | RC4 | 40-bit | PDF 1.3 |
| 2 | 3 | RC4 | 128-bit | PDF 1.4 |
| 4 | 4 | AES (CBC) | 128-bit | PDF 1.6 |
| 5 | 5 | AES (CBC) | 256-bit | PDF 2.0 (R5) |
| 5 | 6 | AES (CBC) | 256-bit | PDF 2.0 (R6) |

## Password Verification

### RC4 40/128-bit (V=1,2 / R=2,3)

#### Key Derivation (MD5-based)

1. Pad the password to 32 bytes using a fixed padding constant
2. Append the 32-byte `/O` (owner hash) entry
3. Append the 4-byte `/P` (permissions) value (little-endian)
4. Append the first element of the `/ID` array
5. If R ≥ 4 and EncryptMetadata is false, append `0xFFFFFFFF`
6. Compute MD5 hash of the result
7. For R ≥ 3: do 50 additional rounds of MD5 on the first N bytes (N = key length / 8)
8. Truncate to the key length

#### User Password Verification

**R=2**: RC4-encrypt the 32-byte padding constant with the derived key. Compare with the `/U` entry.

**R≥3**:
1. MD5(padding + ID[0]) → 16-byte hash
2. RC4-encrypt the hash with the derived key (round 0)
3. For rounds 1-19: RC4-encrypt the result with key XORed with the round number
4. Compare the first 16 bytes with the `/U` entry

### AES-256 (V=5 / R=5)

1. `H = SHA256(password + validation_salt)` (salt extracted from bytes 32-40 of `/U`)
2. Compare H with the first 32 bytes of `/U`

### AES-256 (V=5 / R=6)

R=6 uses a more complex iterative hash with XOR-based mixing. The current implementation provides a simplified version.

## PDF Parsing (`src/formats/pdf.rs`)

The implementation uses a custom minimal PDF parser (no external PDF library dependency):

1. **Find the trailer**: search for `trailer` keyword from the end of the file
2. **Extract `/Encrypt` reference**: parse the trailer dictionary for the indirect object reference
3. **Extract `/ID` array**: parse the hex string for the first ID
4. **Resolve the Encrypt object**: find `N 0 obj` in the file and read until `endobj`
5. **Parse encryption parameters**: extract `/V`, `/R`, `/Length`, `/O`, `/U`, `/P` from the dictionary

### Why a custom parser?

- `lopdf` 0.43 has a compatibility issue with the latest `time` crate
- We only need to extract a few fields from the trailer and one object — a full PDF parser is overkill
- The custom parser is ~100 lines and handles the common cases

## RC4 Implementation

A pure-Rust RC4 stream cipher is included (the `rc4` crate is also a dependency but not used for PDF):

```rust
fn rc4_encrypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    // KSA (Key Scheduling Algorithm)
    let mut s: Vec<u8> = (0..=255).collect();
    let mut j: u8 = 0;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    // PRGA (Pseudo-Random Generation Algorithm)
    let mut i: u8 = 0;
    let mut j: u8 = 0;
    data.iter().map(|&byte| {
        i = i.wrapping_add(1);
        j = j.wrapping_add(s[i as usize]);
        s.swap(i as usize, j as usize);
        byte ^ s[(s[i as usize].wrapping_add(s[j as usize])) as usize]
    }).collect()
}
```

## Testing

Test files are created with PyPDF2:

```python
from PyPDF2 import PdfWriter
writer.encrypt("92eo")  # creates RC4-128 (V=2, R=3)
```

For AES-256 testing, use `qpdf`:

```bash
qpdf --encrypt=92eo 92eo 256 -- test.pdf test_aes256.pdf
```
