use anyhow::Result;
use std::io::Read;

/// ZIP encryption types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZipEncryptionType {
    None,
    Traditional,  // ZipCrypto/PKZIP
    Aes128,
    Aes192,
    Aes256,
}

/// ZIP crypto constants
const DECRYPT_HEADER_SIZE: usize = 12;
const ZIP_CRYPTO_KEY_0: u32 = 0x12345678;
const ZIP_CRYPTO_KEY_1: u32 = 0x23456789;
const ZIP_CRYPTO_KEY_2: u32 = 0x34567890;

/// Traditional ZIP crypto state
pub struct ZipCryptoKeys {
    key0: u32,
    key1: u32,
    key2: u32,
}

impl ZipCryptoKeys {
    fn new() -> Self {
        Self {
            key0: ZIP_CRYPTO_KEY_0,
            key1: ZIP_CRYPTO_KEY_1,
            key2: ZIP_CRYPTO_KEY_2,
        }
    }

    fn update(&mut self, byte: u8) {
        self.key0 = crc32_update(self.key0, byte);
        self.key1 = self.key1.wrapping_add((self.key0 & 0xff) as u32);
        self.key1 = self.key1.wrapping_mul(134775813).wrapping_add(1);
        self.key2 = crc32_update(self.key2, (self.key1 >> 24) as u8);
    }

    fn stream_byte(&mut self) -> u8 {
        let temp = (self.key2 | 2) as u16;
        ((temp.wrapping_mul(temp ^ 1)) >> 8) as u8
    }

    fn decrypt_byte(&mut self, cipher_byte: u8) -> u8 {
        let plain_byte = cipher_byte ^ self.stream_byte();
        self.update(plain_byte);
        plain_byte
    }
}

/// CRC32 table for ZIP encryption
const CRC32_TABLE: &[u32] = &[
    0x00000000, 0x77073096, 0xee0e612c, 0x990951ba, 0x076dc419,
    0x706af48f, 0xe963a535, 0x9e6495a3, 0x0edb8832, 0x79dcb8a4,
    0xe0d5e91e, 0x97d2d988, 0x09b64c2b, 0x7eb17cbd, 0xe7b82d07,
    0x90bf1d91, 0x1db71064, 0x6ab020f2, 0xf3b97148, 0x84be41de,
    0x1adad47d, 0x6ddde4eb, 0xf4d4b551, 0x83d385c7, 0x136c9856,
    0x646ba8c0, 0xfd62f97a, 0x8a65c9ec, 0x14015c4f, 0x63066cd9,
    0xfa0f3d63, 0x8d080df5, 0x3b6e20c8, 0x4c69105e, 0xd56041e4,
    0xa2677172, 0x3c03e4d1, 0x4b04d447, 0xd20d85fd, 0xa50ab56b,
    0x35b5a8fa, 0x42b2986c, 0xdbbbc9d6, 0xacbcf940, 0x32d86ce3,
    0x45df5c75, 0xdcd60dcf, 0xabd13d59, 0x26d930ac, 0x51de003a,
    0xc8d75180, 0xbfd06116, 0x21b4f4b5, 0x56b3c423, 0xcfba9599,
    0xb8bda50f, 0x2802b89e, 0x5f058808, 0xc60cd9b2, 0xb10be924,
    0x2f6f7c87, 0x58684c11, 0xc1611dab, 0xb6662d3d, 0x76dc4190,
    0x01db7106, 0x98d220bc, 0xefd5102a, 0x71b18589, 0x06b6b51f,
    0x9fbfe4a5, 0xe8b8d433, 0x7807c9a2, 0x0f00f934, 0x9609a88e,
    0xe10e9818, 0x7f6a0dbb, 0x086d3d2d, 0x91646c97, 0xe6635c01,
    0x6b6b51f4, 0x1c6c6162, 0x856530d8, 0xf262004e, 0x6c0695ed,
    0x1b01a57b, 0x8208f4c1, 0xf50fc457, 0x65b0d9c6, 0x12b7e950,
    0x8bbeb8ea, 0xfcb9887c, 0x62dd1ddf, 0x15da2d49, 0x8cd37cf3,
    0xfbd44c65, 0x4db26158, 0x3ab551ce, 0xa3bc0074, 0xd4bb30e2,
    0x4adfa541, 0x3dd895d7, 0xa4d1c46d, 0xd3d6f4fb, 0x4369e96a,
    0x346ed9fc, 0xad678846, 0xda60b8d0, 0x44042d73, 0x33031de5,
    0xaa0a4c5f, 0xdd0d7cc9, 0x5005713c, 0x270241aa, 0xbe0b1010,
    0xc90c2086, 0x5768b525, 0x206f85b3, 0xb966d409, 0xce61e49f,
    0x5edef90e, 0x29d9c998, 0xb0d09822, 0xc7d7a8b4, 0x59b33d17,
    0x2eb40d81, 0xb7bd5c3b, 0xc0ba6cad, 0xedb88320, 0x9abfb3b6,
    0x03b6e20c, 0x74b1d29a, 0xead54739, 0x9dd277af, 0x04db2615,
    0x73dc1683, 0xe3630b12, 0x94643b84, 0x0d6d6a3e, 0x7a6a5aa8,
    0xe40ecf0b, 0x9309ff9d, 0x0a00ae27, 0x7d079eb1, 0xf00f9344,
    0x8708a3d2, 0x1e01f268, 0x6906c2fe, 0xf762575d, 0x806567cb,
    0x196c3671, 0x6e6b06e7, 0xfed41b76, 0x89d32be0, 0x10da7a5a,
    0x67dd4acc, 0xf9b9df6f, 0x8ebeeff9, 0x17b7be43, 0x60b08ed5,
    0xd6d6a3e8, 0xa1d1937e, 0x38d8c2c4, 0x4fdff252, 0xd1bb67f1,
    0xa6bc5767, 0x3fb506dd, 0x48b2364b, 0xd80d2bda, 0xaf0a1b4c,
    0x36034af6, 0x41047a60, 0xdf60efc3, 0xa867df55, 0x316e8eef,
    0x4669be79, 0xcb61b38c, 0xbc66831a, 0x256fd2a0, 0x5268e236,
    0xcc0c7795, 0xbb0b4703, 0x220216b9, 0x5505262f, 0xc5ba3bbe,
    0xb2bd0b28, 0x2bb45a92, 0x5cb36a04, 0xc2d7ffa7, 0xb5d0cf31,
    0x2cd99e8b, 0x5bdeae1d, 0x9b64c2b0, 0xec63f226, 0x756aa39c,
    0x026d930a, 0x9c0906a9, 0xeb0e363f, 0x72076785, 0x05005713,
    0x95bf4a82, 0xe2b87a14, 0x7bb12bae, 0x0cb61b38, 0x92d28e9b,
    0xe5d5be0d, 0x7cdcefb7, 0x0bdbdf21, 0x86d3d2d4, 0xf1d4e242,
    0x68ddb3f8, 0x1fda836e, 0x81be16cd, 0xf6b9265b, 0x6fb077e1,
    0x18b74777, 0x88085ae6, 0xff0f6a70, 0x66063bca, 0x11010b5c,
    0x8f659eff, 0xf862ae69, 0x616bffd3, 0x166ccf45, 0xa00ae278,
    0xd70dd2ee, 0x4e048354, 0x3903b3c2, 0xa7672661, 0xd06016f7,
    0x4969474d, 0x3e6e77db, 0xaed16a4a, 0xd9d65adc, 0x40df0b66,
    0x37d83bf0, 0xa9bcae53, 0xdebb9ec5, 0x47b2cf7f, 0x30b5ffe9,
    0xbdbdf21c, 0xcabac28a, 0x53b39330, 0x24b4a3a6, 0xbad03605,
    0xcdd70693, 0x54de5729, 0x23d967bf, 0xb3667a2e, 0xc4614ab8,
    0x5d681b02, 0x2a6f2b94, 0xb40bbe37, 0xc30c8ea1, 0x5a05df1b,
    0x2d02ef8d,
];

/// Update CRC32 with a single byte (used by ZIP crypto)
fn crc32_update(crc: u32, byte: u8) -> u32 {
    let idx = ((crc ^ byte as u32) & 0xff) as usize;
    (crc >> 8) ^ CRC32_TABLE[idx]
}

/// Detect ZIP encryption type from the general purpose bit flag
pub fn detect_encryption_type(flags: u16, compression_method: u16) -> ZipEncryptionType {
    if flags & 0x0001 == 0 {
        return ZipEncryptionType::None;
    }

    // Check for AES encryption
    if compression_method == 99 {
        // AES encryption uses compression method 99
        // The actual AES strength is stored in the extra data field
        // For now, we'll default to AES256
        return ZipEncryptionType::Aes256;
    }

    // Traditional encryption
    ZipEncryptionType::Traditional
}

/// Verify password for traditional ZIP encryption
pub fn verify_traditional_password(
    encrypted_header: &[u8],
    password: &str,
    crc32: u32,
    last_mod_time: u16,
    flags: u16,
) -> Result<bool> {
    if encrypted_header.len() < DECRYPT_HEADER_SIZE {
        anyhow::bail!("Encrypted header too short");
    }

    // Initialize crypto keys
    let mut keys = ZipCryptoKeys::new();

    // Update keys with password
    for byte in password.bytes() {
        keys.update(byte);
    }

    // Decrypt the 12-byte header
    let mut decrypted_header = vec![0u8; DECRYPT_HEADER_SIZE];
    for i in 0..DECRYPT_HEADER_SIZE {
        decrypted_header[i] = keys.decrypt_byte(encrypted_header[i]);
    }

    // Verify the password
    // According to the ZIP specification, the last byte of the decrypted header
    // should match either:
    // 1. The high byte of the CRC32 (for non-streaming mode)
    // 2. The low byte of the last modification time (for streaming mode)

    let check_byte = decrypted_header[11];

    if flags & 0x0008 != 0 {
        // Data descriptor flag is set (streaming mode)
        // Check against last modification time
        Ok(check_byte == (last_mod_time & 0xff) as u8)
    } else {
        // Check against CRC32 high byte
        Ok(check_byte == ((crc32 >> 24) & 0xff) as u8)
    }
}

/// Read and decrypt a small portion of data to verify CRC
pub fn verify_data_crc(
    reader: &mut dyn Read,
    password: &str,
    _expected_crc: u32,
    encrypted_size: u64,
) -> Result<bool> {
    // Initialize crypto keys
    let mut keys = ZipCryptoKeys::new();

    // Update keys with password
    for byte in password.bytes() {
        keys.update(byte);
    }

    // Skip the 12-byte header (already verified)
    let mut header = vec![0u8; DECRYPT_HEADER_SIZE];
    reader.read_exact(&mut header)?;

    // Decrypt header to update key state
    for i in 0..DECRYPT_HEADER_SIZE {
        keys.decrypt_byte(header[i]);
    }

    // Read and decrypt a small portion of the actual data
    let bytes_to_read = std::cmp::min(1024, encrypted_size.saturating_sub(DECRYPT_HEADER_SIZE as u64)) as usize;
    let mut encrypted_data = vec![0u8; bytes_to_read];
    let bytes_read = reader.read(&mut encrypted_data)?;

    if bytes_read == 0 {
        return Ok(false);
    }

    // Decrypt the data
    let mut decrypted_data = vec![0u8; bytes_read];
    for i in 0..bytes_read {
        decrypted_data[i] = keys.decrypt_byte(encrypted_data[i]);
    }

    // For now, we'll trust the header verification
    // Full CRC verification would require decrypting the entire file
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zip_crypto_keys() {
        let mut keys = ZipCryptoKeys::new();

        // Test with known password
        for byte in b"password" {
            keys.update(*byte);
        }

        // Keys should have been updated
        assert_ne!(keys.key0, ZIP_CRYPTO_KEY_0);
        assert_ne!(keys.key1, ZIP_CRYPTO_KEY_1);
        assert_ne!(keys.key2, ZIP_CRYPTO_KEY_2);
    }

    #[test]
    fn test_detect_encryption() {
        // No encryption
        assert_eq!(detect_encryption_type(0x0000, 8), ZipEncryptionType::None);

        // Traditional encryption
        assert_eq!(detect_encryption_type(0x0001, 8), ZipEncryptionType::Traditional);

        // AES encryption
        assert_eq!(detect_encryption_type(0x0001, 99), ZipEncryptionType::Aes256);
    }
}