use crate::generators::CandidateBatch;
use anyhow::Result;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Outcome of checking a batch or prefix group of candidates.
#[derive(Debug, Clone, Copy, Default)]
pub struct BatchCheckResult {
    /// Index of the matching candidate in the batch/group, if found.
    pub matched: Option<usize>,
    /// Number of candidates checked before stopping or finishing.
    pub attempts: u64,
}

/// Reusable buffer for `prefix || last_byte` candidates that stays on the stack
/// for passwords up to 128 bytes and spills to the heap only for longer prefixes.
pub(crate) struct PrefixBuf {
    stack: [u8; 128],
    heap: Vec<u8>,
    prefix_len: usize,
}

impl PrefixBuf {
    #[inline]
    pub(crate) fn new(prefix: &[u8]) -> Self {
        let mut stack = [0u8; 128];
        let mut heap = Vec::new();
        if prefix.len() < stack.len() {
            stack[..prefix.len()].copy_from_slice(prefix);
        } else {
            heap.resize(prefix.len() + 1, 0);
            heap[..prefix.len()].copy_from_slice(prefix);
        }
        Self {
            stack,
            heap,
            prefix_len: prefix.len(),
        }
    }

    #[inline]
    pub(crate) fn with_last(&mut self, last_byte: u8) -> &[u8] {
        if self.prefix_len < self.stack.len() {
            self.stack[self.prefix_len] = last_byte;
            &self.stack[..self.prefix_len + 1]
        } else {
            self.heap[self.prefix_len] = last_byte;
            &self.heap
        }
    }
}

/// Verifies passwords against an encrypted document.
///
/// The engine drives verification through [`verify_batch`] (for dictionary and
/// hybrid batches) and [`verify_prefix_group`] (for brute-force and mask shards).
/// Format implementations can override those methods to interleave multiple
/// candidates or reuse prefix state, or rely on the default implementations
/// which call [`quick_check`] followed by [`verify`].
pub trait PasswordVerifier: Send + Sync {
    /// Optional fast pre-filter. Returns `true` if the password *might* be
    /// correct. Defaults to `true` for formats where full verification is
    /// already a single step.
    fn quick_check(&self, _password: &[u8]) -> bool {
        true
    }

    /// Full verification. Returns `true` only if the password is correct.
    fn verify(&self, password: &[u8]) -> bool;

    /// Checks a batch of candidates, allowing format implementations to use
    /// multi-candidate interleaving or SIMD. Stops early when `stop` becomes
    /// `true`.
    fn verify_batch(&self, batch: &CandidateBatch, stop: &AtomicBool) -> BatchCheckResult {
        let mut attempts = 0u64;
        for (idx, pw) in batch.iter().enumerate() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            attempts += 1;
            if self.quick_check(pw) && self.verify(pw) {
                return BatchCheckResult {
                    matched: Some(idx),
                    attempts,
                };
            }
        }
        BatchCheckResult {
            matched: None,
            attempts,
        }
    }

    /// Checks all candidates formed by appending each byte of `last_bytes` to
    /// `prefix`. Allows verifiers to cache prefix state and/or interleave lanes.
    fn verify_prefix_group(
        &self,
        prefix: &[u8],
        last_bytes: &[u8],
        stop: &AtomicBool,
    ) -> BatchCheckResult {
        let mut pbuf = PrefixBuf::new(prefix);
        let mut attempts = 0u64;
        for (idx, &b) in last_bytes.iter().enumerate() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let cand = pbuf.with_last(b);
            attempts += 1;
            if self.quick_check(cand) && self.verify(cand) {
                return BatchCheckResult {
                    matched: Some(idx),
                    attempts,
                };
            }
        }
        BatchCheckResult {
            matched: None,
            attempts,
        }
    }

    /// Human-readable format name, e.g. `"ZIP"`, `"PDF"`.
    fn format_name(&self) -> &'static str;

    /// Encryption details, e.g. `"ZipCrypto"` or `"AES-256"`.
    fn encryption_info(&self) -> &str;
}

/// Options that influence password verification.
#[derive(Debug, Clone, Copy, Default)]
pub struct VerifyOptions {
    /// Also try the PDF owner password. Off by default because it roughly
    /// triples verification cost for every candidate.
    pub try_pdf_owner: bool,
}

/// Creates the appropriate verifier by sniffing the file's magic bytes.
///
/// Falls back to extension matching when magic bytes are inconclusive.
pub fn create_verifier(path: &Path) -> Result<Box<dyn PasswordVerifier>> {
    create_verifier_with_options(path, VerifyOptions::default())
}

/// Like [`create_verifier`], with explicit verification options.
pub fn create_verifier_with_options(
    path: &Path,
    options: VerifyOptions,
) -> Result<Box<dyn PasswordVerifier>> {
    let (magic, n) = read_magic(path)?;
    match detect_format(&magic[..n], path) {
        FormatKind::Zip => zip::ZipVerifier::new(path).map(|v| Box::new(v) as _),
        FormatKind::Pdf => {
            pdf::PdfVerifier::new(path, options.try_pdf_owner).map(|v| Box::new(v) as _)
        }
        FormatKind::Office => office::OfficeVerifier::new(path).map(|v| Box::new(v) as _),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormatKind {
    Zip,
    Pdf,
    Office,
}

fn read_magic(path: &Path) -> Result<([u8; 8], usize)> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut buf = [0u8; 8];
    let n = file.read(&mut buf)?;
    Ok((buf, n))
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
