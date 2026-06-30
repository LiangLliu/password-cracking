use anyhow::Result;
use std::path::Path;

/// Verifies passwords against an encrypted document.
///
/// Implementations should make [`quick_check`] very fast (rejecting ~99% of
/// wrong passwords) and reserve [`verify`] for the expensive full integrity
/// check. The engine calls `quick_check` first and only invokes `verify` when
/// the quick check passes.
pub trait PasswordVerifier: Send + Sync {
    /// Fast pre-filter. Returns `true` if the password *might* be correct.
    fn quick_check(&self, password: &[u8]) -> bool;

    /// Full verification. Returns `true` only if the password is correct.
    fn verify(&self, password: &[u8]) -> bool;

    /// Human-readable format name, e.g. `"ZIP"`, `"PDF"`.
    fn format_name(&self) -> &'static str;

    /// Encryption details, e.g. `"ZipCrypto"` or `"AES-256"`.
    fn encryption_info(&self) -> &str;
}

/// Creates the appropriate verifier by sniffing the file's magic bytes.
///
/// Falls back to extension matching when magic bytes are inconclusive.
pub fn create_verifier(path: &Path) -> Result<Box<dyn PasswordVerifier>> {
    let magic = read_magic(path)?;
    match detect_format(&magic, path) {
        FormatKind::Zip => zip::ZipVerifier::new(path).map(|v| Box::new(v) as _),
        FormatKind::Pdf => pdf::PdfVerifier::new(path).map(|v| Box::new(v) as _),
        FormatKind::Office => office::OfficeVerifier::new(path).map(|v| Box::new(v) as _),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormatKind {
    Zip,
    Pdf,
    Office,
}

fn read_magic(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 8];
    let n = file.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

fn detect_format(magic: &[u8], path: &Path) -> FormatKind {
    // ZIP: PK\x03\x04  (also OOXML .docx/.xlsx/.pptx are ZIP)
    if magic.len() >= 4 && &magic[..4] == b"PK\x03\x04" {
        // OOXML files are ZIP containers with specific extensions
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext = ext.to_lowercase();
            if matches!(ext.as_str(), "docx" | "xlsx" | "pptx" | "docm" | "xlsm" | "pptm") {
                return FormatKind::Office;
            }
        }
        return FormatKind::Zip;
    }
    // PDF: %PDF-
    if magic.len() >= 5 && &magic[..5] == b"%PDF-" {
        return FormatKind::Pdf;
    }
    // OLE2 compound file (old Office .doc/.xls/.ppt)
    if magic.len() >= 8 && &magic[..8] == b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1" {
        return FormatKind::Office;
    }
    // Fallback to extension
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("zip") => FormatKind::Zip,
        Some("pdf") => FormatKind::Pdf,
        Some("docx" | "xlsx" | "pptx" | "doc" | "xls" | "ppt" | "docm" | "xlsm" | "pptm") => {
            FormatKind::Office
        }
        _ => FormatKind::Zip, // best guess
    }
}

pub mod office;
pub mod pdf;
pub mod zip;
