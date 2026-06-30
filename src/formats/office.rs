use super::PasswordVerifier;
use anyhow::{Context, Result};
use std::path::Path;

pub struct OfficeVerifier {
    _file_path: std::path::PathBuf,
}

impl OfficeVerifier {
    pub fn new(path: &Path) -> Result<Self> {
        let _ = std::fs::File::open(path).context("Failed to open Office file")?;
        Ok(Self {
            _file_path: path.to_path_buf(),
        })
    }
}

impl PasswordVerifier for OfficeVerifier {
    fn quick_check(&self, _password: &[u8]) -> bool {
        false
    }

    fn verify(&self, _password: &[u8]) -> bool {
        // TODO: Phase 5
        false
    }

    fn format_name(&self) -> &'static str {
        "Office"
    }

    fn encryption_info(&self) -> &str {
        "unknown"
    }
}
