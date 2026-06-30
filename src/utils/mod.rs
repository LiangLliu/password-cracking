use anyhow::{bail, Context, Result};
use std::path::Path;

/// Common character sets for brute-force attacks.
pub mod charsets {
    pub const DIGITS: &str = "0123456789";
    pub const LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
    pub const UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    pub const SPECIAL: &str = "!@#$%^&*()-_=+[]{}|;:'\",.<>/?`~";

    pub fn alphanumeric() -> String {
        format!("{DIGITS}{LOWERCASE}{UPPERCASE}")
    }

    pub fn all() -> String {
        format!("{DIGITS}{LOWERCASE}{UPPERCASE}{SPECIAL}")
    }
}

/// Validates that a path exists and is a readable file.
pub fn validate_file(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("File does not exist: {}", path.display());
    }
    if !path.is_file() {
        bail!("Not a file: {}", path.display());
    }
    std::fs::File::open(path).with_context(|| format!("Cannot read: {}", path.display()))?;
    Ok(())
}

/// Validates that a wordlist path exists (file or directory).
pub fn validate_wordlist(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("Path does not exist: {}", path.display());
    }
    if path.is_file() {
        std::fs::File::open(path).with_context(|| format!("Cannot read: {}", path.display()))?;
    } else if path.is_dir() {
        std::fs::read_dir(path).with_context(|| format!("Cannot read: {}", path.display()))?;
    } else {
        bail!("Not a file or directory: {}", path.display());
    }
    Ok(())
}

/// Formats a duration as `1h 2m 3s`.
pub fn format_duration(d: std::time::Duration) -> String {
    let s = d.as_secs();
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    match h {
        0 if m == 0 => format!("{s}s"),
        0 => format!("{m}m {s}s"),
        _ => format!("{h}h {m}m {s}s"),
    }
}

/// Formats an integer with thousands separators.
pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let len = chars.len();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formatting() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1_234_567_890), "1,234,567,890");
    }

    #[test]
    fn duration_formatting() {
        use std::time::Duration;
        assert_eq!(format_duration(Duration::from_secs(5)), "5s");
        assert_eq!(format_duration(Duration::from_secs(65)), "1m 5s");
        assert_eq!(format_duration(Duration::from_secs(3661)), "1h 1m 1s");
    }
}
