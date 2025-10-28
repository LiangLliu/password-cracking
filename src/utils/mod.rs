use std::fs;
use std::path::Path;
use anyhow::{Result, Context};

/// 常用密码字符集
pub mod charsets {
    pub const DIGITS: &str = "0123456789";
    pub const LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
    pub const UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    pub const SPECIAL: &str = "!@#$%^&*()-_=+[]{}|;:'\",.<>/?`~";

    pub fn alphanumeric() -> String {
        format!("{}{}{}", DIGITS, LOWERCASE, UPPERCASE)
    }

    pub fn all() -> String {
        format!("{}{}{}{}", DIGITS, LOWERCASE, UPPERCASE, SPECIAL)
    }
}

/// 验证文件是否存在且可读
pub fn validate_file(path: &Path) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("File does not exist: {:?}", path);
    }

    if !path.is_file() {
        anyhow::bail!("Path is not a file: {:?}", path);
    }

    // 尝试打开文件以验证权限
    fs::File::open(path)
        .with_context(|| format!("Cannot read file: {:?}", path))?;

    Ok(())
}

/// 验证字典路径（可以是文件或目录）
pub fn validate_wordlist(path: &Path) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("Path does not exist: {:?}", path);
    }

    if path.is_file() {
        // 如果是文件，验证是否可读
        fs::File::open(path)
            .with_context(|| format!("Cannot read file: {:?}", path))?;
    } else if path.is_dir() {
        // 如果是目录，验证是否可以列出内容
        fs::read_dir(path)
            .with_context(|| format!("Cannot read directory: {:?}", path))?;
    } else {
        anyhow::bail!("Path is neither a file nor a directory: {:?}", path);
    }

    Ok(())
}

/// 格式化时间显示
pub fn format_duration(duration: std::time::Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// 格式化大数字（添加千位分隔符）
pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;

    for ch in s.chars().rev() {
        if count == 3 {
            result.push(',');
            count = 0;
        }
        result.push(ch);
        count += 1;
    }

    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(1234567890), "1,234,567,890");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
    }

    #[test]
    fn test_format_duration() {
        use std::time::Duration;

        assert_eq!(format_duration(Duration::from_secs(45)), "45s");
        assert_eq!(format_duration(Duration::from_secs(125)), "2m 5s");
        assert_eq!(format_duration(Duration::from_secs(3665)), "1h 1m 5s");
    }
}