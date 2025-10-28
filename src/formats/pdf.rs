use super::DocumentCracker;
use anyhow::{Result, Context};
use std::path::Path;
use std::fs;

pub struct PdfCracker {
    file_path: String,
    file_data: Vec<u8>,
}

impl PdfCracker {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file_path = path.as_ref().to_string_lossy().to_string();
        let file_data = fs::read(&path)
            .with_context(|| format!("Failed to read PDF file: {}", file_path))?;

        // 验证是否是加密的PDF
        if !Self::is_encrypted(&file_data)? {
            anyhow::bail!("PDF file is not encrypted");
        }

        Ok(Self {
            file_path,
            file_data,
        })
    }

    fn is_encrypted(data: &[u8]) -> Result<bool> {
        // 简单检查PDF是否包含加密标记
        let content = String::from_utf8_lossy(data);
        Ok(content.contains("/Encrypt"))
    }

    fn try_password(&self, _password: &str) -> Result<bool> {
        // 注意：lopdf库的加密支持有限
        // 实际项目中可能需要使用更专业的PDF库

        // 这里只是一个示例实现
        // 实际需要实现PDF密码验证逻辑
        Ok(false)
    }
}

impl DocumentCracker for PdfCracker {
    fn verify_password(&self, password: &str) -> Result<bool> {
        self.try_password(password)
    }

    fn get_info(&self) -> String {
        format!("PDF File: {}", self.file_path)
    }

    fn get_type(&self) -> &'static str {
        "PDF"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_encrypted() {
        let encrypted_pdf = b"%PDF-1.4\n/Encrypt 123 0 R\n";
        assert!(PdfCracker::is_encrypted(encrypted_pdf).unwrap());

        let normal_pdf = b"%PDF-1.4\n/Type /Catalog\n";
        assert!(!PdfCracker::is_encrypted(normal_pdf).unwrap());
    }
}