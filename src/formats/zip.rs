use super::PasswordVerifier;
use anyhow::{Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::OnceLock;
use zip::ZipArchive;

/// ZIP password verifier supporting both ZipCrypto and AES encryption.
///
/// For ZipCrypto, `quick_check` uses a custom 12-byte header check that
/// rejects ~99.6% of wrong passwords without any file data decryption.
pub struct ZipVerifier {
    file_path: std::path::PathBuf,
    first_encrypted_index: usize,
    encryption: Encryption,
    /// Pre-read 12-byte encryption header for ZipCrypto quick check.
    crypto_header: [u8; 12],
    /// CRC32 of the encrypted entry (for header verification byte).
    crc32: u32,
    /// Last modification time (DOS format) for data-descriptor mode check.
    last_mod_time: u16,
    /// General purpose bit flag (bit 3 = data descriptor / streaming mode).
    flags: u16,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Encryption {
    ZipCrypto,
    Aes,
}

/// CRC32 lookup table (polynomial 0xEDB88320), built once.
fn crc32_table() -> &'static [u32; 256] {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut tbl = [0u32; 256];
        for i in 0..256u32 {
            let mut crc = i;
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB88320
                } else {
                    crc >> 1
                };
            }
            tbl[i as usize] = crc;
        }
        tbl
    })
}

/// ZipCrypto stream cipher key state.
struct ZipCryptoKeys {
    k0: u32,
    k1: u32,
    k2: u32,
}

impl ZipCryptoKeys {
    const INIT_K0: u32 = 0x12345678;
    const INIT_K1: u32 = 0x23456789;
    const INIT_K2: u32 = 0x34567890;

    fn from_password(password: &[u8], table: &[u32; 256]) -> Self {
        let mut keys = Self {
            k0: Self::INIT_K0,
            k1: Self::INIT_K1,
            k2: Self::INIT_K2,
        };
        for &byte in password {
            keys.update(byte, table);
        }
        keys
    }

    #[inline]
    fn update(&mut self, byte: u8, table: &[u32; 256]) {
        self.k0 = crc32_byte(self.k0, byte, table);
        self.k1 = self
            .k1
            .wrapping_add(self.k0 & 0xff)
            .wrapping_mul(134_775_813)
            .wrapping_add(1);
        self.k2 = crc32_byte(self.k2, (self.k1 >> 24) as u8, table);
    }

    #[inline]
    fn stream_byte(&self) -> u8 {
        // The multiplication must be done in u32 to match the reference
        // implementation (C uses `unsigned` = 32-bit for the product).
        let temp = (self.k2 | 2) & 0xffff;
        ((temp.wrapping_mul(temp ^ 1)) >> 8) as u8
    }

    #[inline]
    fn decrypt_byte(&mut self, cipher: u8, table: &[u32; 256]) -> u8 {
        let plain = cipher ^ self.stream_byte();
        self.update(plain, table);
        plain
    }
}

#[inline]
fn crc32_byte(crc: u32, byte: u8, table: &[u32; 256]) -> u32 {
    let idx = ((crc ^ byte as u32) & 0xff) as usize;
    (crc >> 8) ^ table[idx]
}

impl ZipVerifier {
    pub fn new(path: &Path) -> Result<Self> {
        let file = File::open(path).context("Failed to open ZIP")?;
        let mut archive = ZipArchive::new(file).context("Invalid ZIP file")?;

        let mut first_encrypted = None;
        let mut encryption = Encryption::ZipCrypto;

        for i in 0..archive.len() {
            let entry = archive.by_index_raw(i)?;
            if entry.encrypted() {
                first_encrypted = Some(i);
                if entry.compression() == zip::CompressionMethod::Aes {
                    encryption = Encryption::Aes;
                }
                break;
            }
        }

        let first_encrypted_index = first_encrypted.context("ZIP is not encrypted")?;

        // For ZipCrypto, extract the 12-byte header + metadata for fast checking.
        let (crypto_header, crc32, last_mod_time, flags) = if encryption == Encryption::ZipCrypto {
            extract_zipcrypto_params(path, first_encrypted_index)?
        } else {
            ([0u8; 12], 0, 0, 0)
        };

        Ok(Self {
            file_path: path.to_path_buf(),
            first_encrypted_index,
            encryption,
            crypto_header,
            crc32,
            last_mod_time,
            flags,
        })
    }
}

/// Extract ZipCrypto parameters by reading the local file header directly.
///
/// ZIP local file header layout:
/// ```text
/// offset  size  field
/// 0       4     signature (PK\x03\x04)
/// 4       2     version needed
/// 6       2     general purpose bit flag   ← we need this
/// 8       2     compression method
/// 10      2     last mod time              ← we need this (for data descriptor mode)
/// 12      2     last mod date
/// 14      4     crc32                      ← we need this
/// 18      4     compressed size
/// 22      4     uncompressed size
/// 26      2     file name length
/// 28      2     extra field length
/// 30      var   file name
/// 30+n    var   extra field
/// 30+n+m  12    encryption header          ← we need this
/// ```
fn extract_zipcrypto_params(path: &Path, index: usize) -> Result<([u8; 12], u32, u16, u16)> {
    // Use the zip crate to get CRC32 and header start, then read raw bytes.
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let entry = archive.by_index_raw(index)?;

    let crc32 = entry.crc32();
    let header_start = entry.header_start();

    // Read the local file header manually to get flags, mod time, and compute
    // the data start (which is where the 12-byte encryption header begins).
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(header_start))?;

    // Local file header: sig(4) + version(2) + flags(2) + compression(2)
    // + mod_time(2) + mod_date(2) + crc32(4) + comp_size(4) + uncomp_size(4)
    // + name_len(2) + extra_len(2) = 30 bytes fixed
    let mut local_header = [0u8; 30];
    file.read_exact(&mut local_header)?;

    // Verify signature
    if &local_header[0..4] != b"PK\x03\x04" {
        anyhow::bail!("Invalid local file header signature");
    }

    let flags = u16::from_le_bytes([local_header[6], local_header[7]]);
    let last_mod_time = u16::from_le_bytes([local_header[10], local_header[11]]);
    let name_len = u16::from_le_bytes([local_header[26], local_header[27]]) as u64;
    let extra_len = u16::from_le_bytes([local_header[28], local_header[29]]) as u64;

    // Data starts right after the fixed header + filename + extra field.
    let data_start = header_start + 30 + name_len + extra_len;

    // Read the 12-byte encryption header.
    file.seek(SeekFrom::Start(data_start))?;
    let mut crypto_header = [0u8; 12];
    file.read_exact(&mut crypto_header)?;

    Ok((crypto_header, crc32, last_mod_time, flags))
}

impl PasswordVerifier for ZipVerifier {
    fn quick_check(&self, password: &[u8]) -> bool {
        match self.encryption {
            Encryption::ZipCrypto => {
                // Fast path: decrypt only the 12-byte header, check the
                // verification byte. This rejects ~99.6% of wrong passwords.
                let table = crc32_table();
                let mut keys = ZipCryptoKeys::from_password(password, table);
                let mut last_byte = 0u8;
                for &cipher in &self.crypto_header {
                    last_byte = keys.decrypt_byte(cipher, table);
                }
                // The last byte must match:
                // - CRC32 high byte (standard mode, bit 3 of flags = 0)
                // - Last modification time HIGH byte (data descriptor mode, bit 3 = 1)
                //   (Info-ZIP modification: uses high byte of 16-bit DOS time)
                if self.flags & 0x0008 != 0 {
                    last_byte == ((self.last_mod_time >> 8) & 0xff) as u8
                } else {
                    last_byte == ((self.crc32 >> 24) & 0xff) as u8
                }
            }
            Encryption::Aes => self.verify(password),
        }
    }

    fn verify(&self, password: &[u8]) -> bool {
        let file = match File::open(&self.file_path) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let mut archive = match ZipArchive::new(file) {
            Ok(a) => a,
            Err(_) => return false,
        };

        match archive.by_index_decrypt(self.first_encrypted_index, password) {
            Ok(mut reader) => {
                let mut buf = Vec::with_capacity(4096);
                reader.read_to_end(&mut buf).is_ok()
            }
            Err(_) => false,
        }
    }

    fn format_name(&self) -> &'static str {
        "ZIP"
    }

    fn encryption_info(&self) -> &str {
        match self.encryption {
            Encryption::ZipCrypto => "ZipCrypto",
            Encryption::Aes => "AES",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_table_is_standard() {
        let table = crc32_table();
        assert_eq!(table[0], 0x00000000);
        assert_eq!(table[1], 0x77073096);
        assert_eq!(table[255], 0x2D02EF8D);
    }

    #[test]
    fn zipcrypto_keys_change_with_password() {
        let table = crc32_table();
        let keys = ZipCryptoKeys::from_password(b"test", table);
        assert_ne!(keys.k0, ZipCryptoKeys::INIT_K0);
        assert_ne!(keys.k1, ZipCryptoKeys::INIT_K1);
    }

    #[test]
    fn zipcrypto_decrypt_header_known_password() {
        // Decrypt a known header with password "92eo" and verify the check byte.
        // Header bytes from the test.zip file.
        let header = [
            0xfc, 0x3f, 0x66, 0xcc, 0x86, 0x4c, 0xf2, 0x99, 0x7d, 0x76, 0xcc, 0x94,
        ];
        let table = crc32_table();
        let mut keys = ZipCryptoKeys::from_password(b"92eo", table);
        let mut decrypted = [0u8; 12];
        for (i, &cipher) in header.iter().enumerate() {
            decrypted[i] = keys.decrypt_byte(cipher, table);
        }
        // In data descriptor mode, last byte should be 0x74 (high byte of mod_time 0x741d)
        // In CRC mode, last byte should be 0x47 (high byte of CRC 0x475a3fa6)
        eprintln!("Decrypted header: {decrypted:02x?}");
        eprintln!("Last byte: {:#04x}", decrypted[11]);
        assert!(
            decrypted[11] == 0x74 || decrypted[11] == 0x47,
            "Neither check byte matched: got {:#04x}",
            decrypted[11]
        );
    }
}
