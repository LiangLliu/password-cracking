# PDF Internals

**English** | [中文](PDF_IMPLEMENTATION.zh-CN.md)

`src/formats/pdf.rs` verifies PDF user and owner passwords without decrypting
page content. The password check itself is authoritative: a correct `/U` (user)
or `/O` (owner) match is sufficient.

## Supported algorithms

| V | R | Algorithm | Key size | PDF version |
|---|---|-----------|----------|-------------|
| 1 | 2 | RC4 | 40-bit | 1.3 |
| 2 | 3 | RC4 | 128-bit | 1.4 |
| 4 | 4 | AES-128 (content), RC4 U check | 128-bit | 1.6 |
| 5 | 5 | AES-256 (SHA-256) | 256-bit | 2.0 |
| 5 | 6 | AES-256 (Algorithm 2.B) | 256-bit | 2.0 |

Owner passwords are supported for R2–R4 and R5/R6 and are opt-in on the CLI
(`--pdf-owner`) because they roughly triple the per-candidate cost.

## Parsing

A full PDF parser is unnecessary: only the trailer, the `/Encrypt` dictionary
and `/ID` are needed. The implementation is a few hundred lines and avoids a
large dependency (and the `lopdf`/`time` compatibility issue that motivated the
custom parser).

1. Verify that `/Encrypt` exists.
2. Find the trailer:
   - classic `trailer` keyword, or
   - the xref stream dictionary containing `/Encrypt` (PDF 1.5+ files often have
     no classic trailer).
3. Extract `/Encrypt` (an indirect reference or inline dictionary) and
   `/ID[0]`.
4. Resolve the encryption object (`N 0 obj … endobj`).
5. Parse `/V`, `/R`, `/Length`, `/O`, `/U`, `/P`, `/EncryptMetadata`.
6. For V ≥ 5, split `/U` and `/O` into `[32-byte hash][8-byte validation
   salt][8-byte key salt]`.

Parsing details that matter:

- **Top-level `/Length`**: `/CF` contains nested `/Length` values. The parser
  tracks dictionary depth and only reads the top-level value; otherwise an
  AES-128 file may be treated as 40-bit.
- **Signed `/P`**: permissions are a signed 32-bit integer in PDF, not unsigned.
- **Hex strings**: `/O` and `/U` are hex strings; whitespace inside them is
  ignored.
- **Indirect references**: `/Encrypt 4 0 R` and inline dictionaries are both
  accepted.
- **`/EncryptMetadata`**: for R ≥ 4, a `false` value appends `0xFFFFFFFF` to the
  key-derivation input.

## RC4 key derivation (R2–R4)

```text
1. pad or truncate the password to 32 bytes (ISO 32000-1 padding)
2. append /O (32 bytes)
3. append /P as 4-byte little-endian
4. append /ID[0]
5. if R >= 4 and /EncryptMetadata is false, append 0xFFFFFFFF
6. MD5 the result
7. if R >= 3, run MD5 50 times on the first N bytes (N = key length)
8. truncate to N bytes
```

## User password check

**R = 2**: RC4-encrypt the 32-byte padding with the file key and compare the
result with `/U`.

**R ≥ 3**:

```text
1. H = MD5(padding + /ID[0])
2. R0 = RC4(key, H)
3. for i in 1..=19: Ri = RC4(key XOR i, R(i-1))
4. compare R19 with /U[0..16]
```

This is the check used for RC4-40, RC4-128 and AES-128 (R4). AES-128 encrypts
content with AES, but the `/U` validation remains RC4-based.

## AES-256 R5

```text
user:  SHA-256(password + U_validation_salt)              == U[0..32]
owner: SHA-256(password + O_validation_salt + full_U)     == O[0..32]
```

## AES-256 R6 (Algorithm 2.B)

R6 replaces the single hash with an iterative construction:

```text
K = SHA-256(password || salt || udata)
round = 0
loop:
    K1 = (password || K || udata) repeated 64 times
    E  = AES-128-CBC-Encrypt(key=K[0..16], iv=K[16..32], K1)   // no padding
    K  = SHA-256/384/512(E), selected by sum(E[0..16]) % 3
    round += 1
    stop when round >= 64 and E[last] <= round - 32
return K[0..32]
```

- User password: `salt = U_validation_salt`, `udata = []`.
- Owner password: `salt = O_validation_salt`, `udata = full 48-byte U`.

The implementation includes a known-answer test for `92eo` and is verified
against `test_r6.pdf` and `test_xref.pdf`.

## Owner passwords (R2–R4, Algorithm 3)

```text
1. H = MD5(pad(owner_password)); if R >= 3, 50 rounds
2. key = H[0..N]
3. if R == 2:  O_padded = RC4(key, /O)
   else:        for i in 19..=0: O_padded = RC4(key XOR i, O_padded)
4. verify O_padded as a user password against /U
```

For R5/R6, the owner hash is computed directly (see above) using the full `/U`
entry as extra data.

## RC4 and AES primitives

- RC4 is implemented inline (KSA + PRGA) because the PDF path only needs
  encryption/decryption of small buffers and avoiding a dependency keeps the
  hot loop simple.
- AES-128-CBC for Algorithm 2.B is implemented without padding: the input is
  always block-aligned and the output is not PKCS#7-padded.
- SHA-256/384/512 come from the `sha2` crate; MD5 from `md-5`.

## Performance

Measured on a 10-core M2 Pro:

| Case | Speed |
|------|-------|
| RC4-128 (R3) | ~529K passwords/sec |
| AES-128 (R4) | ~515K passwords/sec |
| AES-256 (R6) | ~7.6K passwords/sec |
| RC4-128 with `--pdf-owner` | ~150K passwords/sec |

R6 is slower because Algorithm 2.B performs dozens of AES-CBC and hash
operations per candidate.

## Testing and fixtures

| File | Encryption |
|------|------------|
| `test.pdf` | RC4-128 (R3) |
| `test_aes128.pdf` | AES-128 (R4) |
| `test_r6.pdf` | AES-256 (R6) |
| `test_xref.pdf` | AES-256 (R6), xref stream, no classic trailer |
| `test_owner_r4.pdf` | AES-128 (R4), owner password only |
| `test_owner_r6.pdf` | AES-256 (R6), owner password only |

All use password `92eo`. Owner-only files require `--pdf-owner` (CLI) or
`VerifyOptions { try_pdf_owner: true }` (library).

Fixtures are generated with `pikepdf`; see `examples/create_test_files.py`.

## Limitations

- Empty-password PDFs are not treated specially.
- Certificate-based encryption is not supported.
- Content is not decrypted, so malformed encrypted streams are not validated;
  the `/U` or `/O` check is the authoritative test.
- R5/R6 files with unusual salts or truncated `/U`/`/O` entries fail cleanly
  rather than guessing.
