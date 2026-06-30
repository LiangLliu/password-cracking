# ZIP Implementation Details

## Encryption Methods

### ZipCrypto (Traditional PKZIP Encryption)

- **Algorithm**: Stream cipher based on CRC32 + custom key schedule
- **Key size**: 96 bits effective (3 × 32-bit keys)
- **Compatibility**: All ZIP tools support it
- **Security**: Weak — vulnerable to known-plaintext attacks
- **Performance**: 7M+ passwords/sec (with 12-byte header quick check)

### AES (WinZip AES)

- **Algorithm**: AES-128/192/256 in CTR mode with HMAC-SHA1
- **Compatibility**: WinZip 9.0+ or compatible tools
- **Security**: Strong
- **Performance**: Slower (handled by the `zip` crate internally)

## ZipCrypto Key Schedule

The encryption uses three 32-bit key registers initialized to fixed constants:

```
key0 = 0x12345678
key1 = 0x23456789
key2 = 0x34567890
```

For each password byte, the keys are updated:

```rust
fn update(&mut self, byte: u8, table: &[u32; 256]) {
    self.k0 = crc32_byte(self.k0, byte, table);
    self.k1 = self.k1.wrapping_add(self.k0 & 0xff)
              .wrapping_mul(134_775_813).wrapping_add(1);
    self.k2 = crc32_byte(self.k2, (self.k1 >> 24) as u8, table);
}
```

The stream byte is generated from `k2`:

```rust
fn stream_byte(&self) -> u8 {
    let temp = (self.k2 | 2) & 0xffff;
    ((temp.wrapping_mul(temp ^ 1)) >> 8) as u8
}
```

Decryption: `plaintext = ciphertext ^ stream_byte()`, then update keys with plaintext.

## 12-Byte Header Quick Check

Every ZipCrypto-encrypted file starts with a 12-byte encryption header. The last byte of this decrypted header is a verification byte:

- **Standard mode** (bit 3 of flags = 0): must equal the high byte of CRC32
- **Data descriptor mode** (bit 3 of flags = 1): must equal the high byte of the last modification time (DOS format)

Our implementation reads the local file header directly to extract:
- General purpose bit flags (to determine verification mode)
- CRC32 (for standard mode check byte)
- Last modification time (for data descriptor mode check byte)
- The 12-byte encrypted header itself

This allows rejecting ~99.6% of wrong passwords by decrypting only 12 bytes instead of the entire file.

## Implementation (`src/formats/zip.rs`)

### Initialization

1. Open the ZIP archive and scan for the first encrypted entry
2. Read the local file header to extract flags, CRC32, and modification time
3. Read the 12-byte encryption header from the data area

### Quick Check (`quick_check`)

1. Initialize keys from the password
2. Decrypt the 12-byte header
3. Compare the last byte against the expected check byte

### Full Verification (`verify`)

1. Open the ZIP archive with the `zip` crate
2. Call `by_index_decrypt(index, password)` to get a decrypted reader
3. Read the entire file content (triggers CRC32 validation)
4. If read succeeds without error, the password is correct

## Cross-Platform ZIP Creation

```bash
# Traditional encryption (macOS/Linux)
zip -e archive.zip files...
zip -P password archive.zip files...

# AES-256 (using 7z, any platform)
7z a -tzip -p"password" -mem=AES256 archive.zip files...
```

## Known Limitations

1. Only the first encrypted entry is checked (multi-entry ZIPs with different passwords per entry are not supported)
2. ZIP64 (large file) support depends on the `zip` crate
3. Split/multi-volume archives are not supported
