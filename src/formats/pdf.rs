use super::PasswordVerifier;
use anyhow::{Context, Result};
use std::path::Path;

pub struct PdfVerifier {
    _file_path: std::path::PathBuf,
    _data: Vec<u8>,
}

impl PdfVerifier {
    pub fn new(path: &Path) -> Result<Self> {
        let data = std::fs::read(path).context("Failed to read PDF")?;
        if !data.windows(8).any(|w| w == b"/Encrypt") {
            anyhow::bail!("PDF is not encrypted");
        }
        Ok(Self {
            _file_path: path.to_path_buf(),
            _data: data,
        })
    }
}

impl PasswordVerifier for PdfVerifier {
    fn quick_check(&self, _password: &[u8]) -> bool {
        false
    }

    fn verify(&self, _password: &[u8]) -> bool {
        // TODO: Phase 4
        false
    }

    fn format_name(&self) -> &'static str {
        "PDF"
    }

    fn encryption_info(&self) -> &str {
        "unknown"
    }
}
