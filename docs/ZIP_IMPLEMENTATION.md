# ZIP Implementation Details

## Overview

This document covers the ZIP format implementation, including encryption methods, cross-platform compatibility, and bug fixes.

## ZIP Encryption Methods

### 1. Traditional Encryption (ZipCrypto/PKZIP)
- **Algorithm**: Modified RC4 variant
- **Key Size**: 96-bit effective
- **Compatibility**: Universal - all ZIP tools support it
- **Security**: Weak - vulnerable to known-plaintext attacks
- **Performance**: Very fast (40,000+ passwords/sec)

### 2. AES Encryption (WinZip AES)
- **Algorithm**: AES-128/192/256
- **Compatibility**: Requires WinZip 9.0+ or compatible tools
- **Security**: Strong
- **Performance**: Moderate (10,000+ passwords/sec)

## Password Verification Process

### Traditional Encryption Validation

1. **12-byte Header Check** (Fast)
   ```
   - Decrypt 12-byte header
   - Verify last byte against CRC32 high byte or timestamp
   - Quick rejection of wrong passwords
   ```

2. **CRC32 Data Validation** (Accurate)
   ```
   - Decrypt file data
   - Calculate CRC32
   - Compare with stored CRC32
   - Ensures password correctness
   ```

### Implementation Fix

**Problem**: Initial implementation only read 16 bytes, insufficient for CRC validation. This caused wrong passwords to be accepted.

**Solution**: Use `read_to_end()` to ensure complete CRC validation:

```rust
// Read enough data to trigger CRC validation
match file.read_to_end(&mut buffer) {
    Ok(_) => true,  // CRC validated
    Err(e) if e.to_string().contains("Invalid checksum") => false,
    // ...
}
```

## Cross-Platform Compatibility

### Creating Encrypted ZIPs

**macOS/Linux**:
```bash
# Traditional encryption
zip -e archive.zip files...
zip -P password archive.zip files...

# No native AES support
```

**Windows**:
```bash
# Using 7-Zip for AES
7z a -p"password" -mem=AES256 archive.zip files...
```

### Reading Encrypted ZIPs

Our implementation correctly handles:
- ZIP files created on any platform
- Both traditional and AES encryption
- Proper byte order and CRC calculation
- Unicode filenames

## Technical Implementation

### Key Components

1. **Encryption Detection**
   ```rust
   // Check general purpose bit flag
   if flags & 0x0001 != 0 {
       // File is encrypted
   }
   ```

2. **CRC32 Calculation**
   - Uses standard ZIP polynomial
   - Validates decrypted data integrity

3. **Performance Optimization**
   - Header-only validation for quick rejection
   - Streaming decryption for large files
   - Batch processing for parallel testing

## Testing

Test files can be created with different encryption:

```bash
# Traditional (cross-platform)
echo "test data" > test.txt
zip -P testpass traditional.zip test.txt

# AES-256 (using 7z)
7z a -tzip -p"testpass" -mem=AES256 aes.zip test.txt
```

## Known Limitations

1. **ZIP64**: Large file support not fully tested
2. **Split Archives**: Multi-volume ZIPs not supported
3. **Compression**: Only Deflate method fully tested

## Security Considerations

1. Traditional ZIP encryption is **not secure** for sensitive data
2. Use AES encryption for actual security needs
3. Password complexity is critical - even AES can be brute-forced with weak passwords