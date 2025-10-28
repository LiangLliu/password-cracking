pub mod pdf;
pub mod office;
pub mod archive;
pub mod zip_crypto;

use anyhow::Result;
use std::path::Path;

/// 文档破解器的通用特征
pub trait DocumentCracker: Send + Sync {
    /// 验证给定密码是否正确
    fn verify_password(&self, password: &str) -> Result<bool>;

    /// 获取文档的基本信息
    fn get_info(&self) -> String;

    /// 获取文档类型
    fn get_type(&self) -> &'static str;
}

/// 根据文件扩展名创建相应的破解器
pub fn create_cracker(path: &Path) -> Result<Box<dyn DocumentCracker>> {
    let extension = path.extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| anyhow::anyhow!("Unable to determine file type"))?;

    match extension.to_lowercase().as_str() {
        "pdf" => Ok(Box::new(pdf::PdfCracker::new(path)?)),
        "docx" | "xlsx" | "pptx" => Ok(Box::new(office::OfficeCracker::new(path)?)),
        "zip" => Ok(Box::new(archive::ZipCracker::new(path)?)),
        _ => Err(anyhow::anyhow!("Unsupported file type: {}", extension))
    }
}