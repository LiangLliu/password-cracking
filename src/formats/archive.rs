use super::DocumentCracker;
use anyhow::{Result, Context};
use std::path::Path;
use std::fs::File;
use std::io::Read;
use zip::ZipArchive;

pub struct ZipCracker {
    file_path: String,
}

impl ZipCracker {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file_path = path.as_ref().to_string_lossy().to_string();

        // Try to open and check if the file is a valid ZIP
        let _file = File::open(&path)
            .with_context(|| format!("Failed to open ZIP file: {}", file_path))?;

        Ok(Self { file_path })
    }

    fn verify_password_internal(&self, password: &str) -> Result<bool> {
        self.verify_password_standard(password)
    }


    fn verify_password_standard(&self, password: &str) -> Result<bool> {
        let file = File::open(&self.file_path)?;
        let mut archive = ZipArchive::new(file)?;

        if archive.len() == 0 {
            return Ok(false);
        }

        // Try to decrypt and read the first file
        let decrypt_result = archive.by_index_decrypt(0, password.as_bytes());

        let success = match decrypt_result {
            Ok(mut file) => {
                // Use read_to_end to force reading all data and trigger CRC validation
                let mut buffer = Vec::new();

                // For large files, limit how much we read to avoid memory issues
                let file_size = file.size();
                if file_size > 1024 * 1024 {
                    // For files larger than 1MB, read up to 1MB
                    buffer.reserve(1024 * 1024);
                    let mut limited = file.take(1024 * 1024);
                    match limited.read_to_end(&mut buffer) {
                        Ok(_) => true,
                        Err(_) => {
                            // Silently ignore all errors - password is wrong
                            false
                        }
                    }
                } else {
                    // For smaller files, read the entire file
                    match file.read_to_end(&mut buffer) {
                        Ok(_) => true,
                        Err(_) => {
                            // Password is wrong, silently return false
                            false
                        }
                    }
                }
            }
            Err(zip::result::ZipError::InvalidPassword) => false,
            Err(zip::result::ZipError::UnsupportedArchive(_)) => false,
            Err(_) => false,
        };

        Ok(success)
    }
}

impl DocumentCracker for ZipCracker {
    fn verify_password(&self, password: &str) -> Result<bool> {
        self.verify_password_internal(password)
    }

    fn get_info(&self) -> String {
        format!("ZIP Archive: {}", self.file_path)
    }

    fn get_type(&self) -> &'static str {
        "ZIP"
    }
}


// Keep the RAR implementation as before
pub struct RarCracker {
    file_path: String,
}

impl RarCracker {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let _file_path = path.as_ref().to_string_lossy().to_string();
        anyhow::bail!("RAR support not implemented yet. Consider using unrar crate.");
    }
}

impl DocumentCracker for RarCracker {
    fn verify_password(&self, _password: &str) -> Result<bool> {
        unimplemented!("RAR password verification not implemented")
    }

    fn get_info(&self) -> String {
        format!("RAR Archive: {}", self.file_path)
    }

    fn get_type(&self) -> &'static str {
        "RAR"
    }
}