use super::PasswordVerifier;
use anyhow::{Context, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

pub struct ZipVerifier {
    file_path: std::path::PathBuf,
    first_encrypted_index: usize,
    aes: bool,
}

impl ZipVerifier {
    pub fn new(path: &Path) -> Result<Self> {
        let file = File::open(path).context("Failed to open ZIP")?;
        let mut archive = ZipArchive::new(file).context("Invalid ZIP file")?;

        // Access metadata via the central directory without reading file data.
        let mut first_encrypted = None;
        let mut aes = false;

        for i in 0..archive.len() {
            // by_index_raw reads the local file header without attempting decryption.
            let entry = archive.by_index_raw(i)?;
            if entry.encrypted() {
                first_encrypted = Some(i);
                if entry.compression() == zip::CompressionMethod::Aes {
                    aes = true;
                }
                break;
            }
        }

        let first_encrypted_index = first_encrypted.context("ZIP is not encrypted")?;

        Ok(Self {
            file_path: path.to_path_buf(),
            first_encrypted_index,
            aes,
        })
    }
}

impl PasswordVerifier for ZipVerifier {
    fn quick_check(&self, password: &[u8]) -> bool {
        // Phase 3: add ZipCrypto 12-byte header fast-path.
        // For now, fall through to full verification.
        self.verify(password)
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
        if self.aes {
            "AES"
        } else {
            "ZipCrypto"
        }
    }
}
