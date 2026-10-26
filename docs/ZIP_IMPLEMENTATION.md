# ZIP Internals

**English** | [中文](ZIP_IMPLEMENTATION.zh-CN.md)

`src/formats/zip.rs` implements `PasswordVerifier` for ZIP archives. It handles
both traditional ZipCrypto and WinZip AES.

## Encryption methods

### ZipCrypto

- Stream cipher built on CRC32 and a 96-bit key schedule (three 32-bit
  registers)
- Supported by every ZIP tool
- Cryptographically weak (known-plaintext attacks), which is why verification
  can be made very fast
- Measured throughput: **11.0M–11.5M passwords/sec** on a 10-core M2 Pro

### AES (WinZip AE-1/AE-2)

- AES-128/192/256 in CTR mode, with PBKDF2-HMAC-SHA1 key derivation and
  HMAC-SHA1 authentication
- Handled by the `zip` crate (`aes-crypto` feature)
- Measured throughput: **~15K passwords/sec**, dominated by PBKDF2
  (1,000 iterations) per candidate

The verifier does not implement AES itself; it delegates to `zip` and focuses
on detecting the encryption type and caching the archive per worker thread.

## ZipCrypto key schedule

Three 32-bit registers are initialized to fixed constants:

```text
key0 = 0x12345678
key1 = 0x23456789
key2 = 0x34567890
```

For every password byte:

```rust
fn update(&mut self, byte: u8, table: &[u32; 256]) {
    self.k0 = crc32_byte(self.k0, byte, table);
    self.k1 = self.k1.wrapping_add(self.k0 & 0xff)
              .wrapping_mul(134_775_813).wrapping_add(1);
    self.k2 = crc32_byte(self.k2, (self.k1 >> 24) as u8, table);
}
```

The keystream byte:

```rust
fn stream_byte(&self) -> u8 {
    let temp = (self.k2 | 2) & 0xffff;
    ((temp.wrapping_mul(temp ^ 1)) >> 8) as u8
}
```

Decryption: `plain = cipher ^ stream_byte()`, then update the keys with the
plaintext byte.

## 12-byte header check

Every ZipCrypto entry starts with a 12-byte encryption header. After decryption,
the last byte is a check byte:

- **Standard mode** (flag bit 3 clear): high byte of the entry's CRC32
- **Data descriptor mode** (flag bit 3 set): high byte of the DOS modification
  time

The verifier reads the local file header directly to obtain the flags, CRC32,
modification time and the 12-byte header. Decrypting only those 12 bytes rejects
about 99.6% of wrong passwords, roughly 20× faster than full decryption.

## Multi-entry archives

ZIP allows a different password per entry. The verifier collects **every**
encrypted entry and accepts a password only if it decrypts all of them:

```rust
for &index in &self.encrypted_indices {
    let mut reader = archive.by_index_decrypt(index, password)?;
    reader.read_to_end(&mut Vec::new())?;
}
```

This covers:

- archives with multiple encrypted entries using the same password
  (`test_multi.zip`);
- archives whose first entry is unencrypted (`test_mixed.zip`);
- archives where the encrypted entry is not index 0.

If a ZIP really uses different passwords per entry, no single password can
satisfy the verifier; this is a deliberate limitation.

## Implementation notes

### Detection

`ZipVerifier::new()` scans the central directory and records every encrypted
entry. AES detection uses the `0x9901` AES extra field:

```rust
let is_aes = entry.compression() == zip::CompressionMethod::Aes
    || entry.extra_data_fields()
        .any(|f| matches!(f, zip::ExtraField::AeXEncryption(_)));
```

Checking the extra field is required because the `zip` crate rewrites an AES
entry's compression method to the inner method (for example Deflate) after
parsing the AES header. Relying on `compression() == Aes` alone misclassifies
AES archives as ZipCrypto, which makes the 12-byte check reject every
candidate.

### Quick check and verify

- ZipCrypto: `quick_check` decrypts 12 bytes and compares the check byte;
  `verify` decrypts all encrypted entries and validates CRC32.
- AES: `quick_check` returns `true`; `verify` runs PBKDF2/AES/HMAC once. This
  avoids paying for AES twice per candidate.

### Archive cache

AES verification reopens the archive for every candidate. A thread-local
`HashMap<PathBuf, ZipArchive<File>>` caches one open archive per worker thread,
avoiding repeated central-directory parsing. ZIP archives are immutable during
a run.

## Creating test archives

```bash
# ZipCrypto (Info-ZIP zip)
zip -e archive.zip files...          # prompt for a password
zip -P 92eo archive.zip files...     # password on the command line

# AES-256 (7-Zip)
7z a -tzip -p92eo -mem=AES256 archive.zip files...

# Multiple encrypted entries
zip -P 92eo multi.zip a.txt b.txt

# Mixed: first entry unencrypted, second encrypted
zip mixed.zip a.txt
zip -P 92eo mixed.zip b.txt
```

## Testing

Fixtures in `examples/` (all password `92eo`):

| File | What it checks |
|------|----------------|
| `test.zip` | Single ZipCrypto entry |
| `test_aes.zip` | AES-256 detection + verification |
| `test_multi.zip` | Two encrypted entries |
| `test_mixed.zip` | Unencrypted first entry |

`tests/e2e.rs` cracks each of them through the public API.

## Limitations

- Split and multi-volume archives are not supported.
- ZIP64 support depends on the `zip` crate.
- A password must decrypt every encrypted entry; per-entry different passwords
  cannot be recovered with a single candidate.
- AES uses the `zip` crate's implementation; AE-1 and AE-2 are supported, but
  the verifier does not distinguish them in `encryption_info()`.
