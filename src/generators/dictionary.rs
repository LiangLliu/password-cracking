use super::PasswordSource;
use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::fs::{File, read_dir};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub struct DictionarySource {
    passwords: Vec<Box<[u8]>>,
    cursor: usize,
}

impl DictionarySource {
    pub fn new(path: &Path) -> Result<Self> {
        let mut passwords = Vec::new();
        if path.is_file() {
            load_file(path, &mut passwords)?;
        } else if path.is_dir() {
            load_dir(path, &mut passwords)?;
        } else {
            bail!("Not a file or directory: {}", path.display());
        }
        if passwords.is_empty() {
            bail!("No passwords found in {}", path.display());
        }
        // Dedup preserving order
        let mut seen = HashSet::new();
        passwords.retain(|p| seen.insert(p.clone()));
        Ok(Self {
            passwords,
            cursor: 0,
        })
    }
}

fn load_file(path: &Path, out: &mut Vec<Box<[u8]>>) -> Result<()> {
    let file = File::open(path).with_context(|| format!("Cannot read {}", path.display()))?;
    // Read raw bytes, not UTF-8 lines: wordlists are often encoded in legacy
    // code pages (e.g. GBK). `BufRead::lines()` would stop at the first
    // invalid UTF-8 line and silently truncate the rest of the file.
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        let line = trim_ascii(&buf);
        if !line.is_empty() && !line.starts_with(b"#") {
            out.push(line.to_vec().into_boxed_slice());
        }
    }
    Ok(())
}

/// Trim ASCII whitespace (including `\r`) from both ends of a byte slice.
fn trim_ascii(mut s: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = s {
        if first.is_ascii_whitespace() {
            s = rest;
        } else {
            break;
        }
    }
    while let [rest @ .., last] = s {
        if last.is_ascii_whitespace() {
            s = rest;
        } else {
            break;
        }
    }
    s
}

fn load_dir(path: &Path, out: &mut Vec<Box<[u8]>>) -> Result<()> {
    let mut stack: Vec<PathBuf> = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in read_dir(&dir)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "txt") {
                load_file(&p, out)?;
            }
        }
    }
    Ok(())
}

impl PasswordSource for DictionarySource {
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>) -> bool {
        batch.clear();
        let end = (self.cursor + super::BATCH_SIZE).min(self.passwords.len());
        if self.cursor >= self.passwords.len() {
            return false;
        }
        batch.extend(self.passwords[self.cursor..end].iter().cloned());
        self.cursor = end;
        !batch.is_empty()
    }

    fn estimated_total(&self) -> Option<u64> {
        Some(self.passwords.len() as u64)
    }

    fn checkpoint(&self) -> Option<String> {
        Some(self.cursor.to_string())
    }

    fn restore(&mut self, checkpoint: &str) -> Result<()> {
        self.cursor = checkpoint.parse()?;
        Ok(())
    }

    fn name(&self) -> &str {
        "dictionary"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_non_utf8_lines_without_truncation() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"first\n").unwrap();
        // GBK-encoded "密码" — invalid UTF-8, must not truncate the wordlist.
        f.write_all(&[0xC3, 0xDC, 0xC2, 0xEB, b'\n']).unwrap();
        f.write_all(b"third\n").unwrap();
        f.flush().unwrap();

        let mut out = Vec::new();
        load_file(f.path(), &mut out).unwrap();

        assert_eq!(out.len(), 3);
        assert_eq!(&*out[0], b"first");
        assert_eq!(&*out[1], &[0xC3, 0xDC, 0xC2, 0xEB]);
        assert_eq!(&*out[2], b"third");
    }

    #[test]
    fn skips_comments_blank_lines_and_crlf() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"# comment\n\n  spaced  \r\nlast\r\n").unwrap();
        f.flush().unwrap();

        let mut out = Vec::new();
        load_file(f.path(), &mut out).unwrap();

        assert_eq!(out.len(), 2);
        assert_eq!(&*out[0], b"spaced");
        assert_eq!(&*out[1], b"last");
    }
}
