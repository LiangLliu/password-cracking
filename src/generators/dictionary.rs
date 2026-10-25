use super::PasswordSource;
use anyhow::{Context, Result, bail};
use memmap2::Mmap;
use std::collections::HashSet;
use std::fs::{File, read_dir};
use std::path::{Path, PathBuf};

/// Streaming dictionary source backed by memory-mapped files.
///
/// Unlike a `Vec<Box<[u8]>>` of every line, this keeps only the file mappings
/// (which the OS pages in on demand) and a cursor. Memory usage stays flat for
/// arbitrarily large wordlists.
///
/// Duplicate elimination is optional (`dedup`): the default is off, because
/// exact dedup requires remembering every candidate and would defeat the
/// streaming goal. With `dedup` enabled only hashes are retained.
pub struct DictionarySource {
    files: Vec<MappedFile>,
    file_index: usize,
    /// Byte offset of the next unread line in `files[file_index]`.
    offset: usize,
    /// Estimated number of candidates (line count at load time).
    total: u64,
    dedup: bool,
    seen: HashSet<u64>,
}

struct MappedFile {
    _path: PathBuf,
    mmap: Mmap,
}

impl DictionarySource {
    pub fn new(path: &Path) -> Result<Self> {
        Self::with_dedup(path, false)
    }

    pub fn with_dedup(path: &Path, dedup: bool) -> Result<Self> {
        let mut paths = Vec::new();
        if path.is_file() {
            paths.push(path.to_path_buf());
        } else if path.is_dir() {
            collect_txt_files(path, &mut paths)?;
            // Deterministic order keeps checkpoints stable across runs.
            paths.sort();
        } else {
            bail!("Not a file or directory: {}", path.display());
        }

        if paths.is_empty() {
            bail!("No .txt files found in {}", path.display());
        }

        let mut files = Vec::with_capacity(paths.len());
        let mut total = 0u64;
        for p in &paths {
            let file = File::open(p).with_context(|| format!("Cannot read {}", p.display()))?;
            // SAFETY: wordlists are treated as immutable while cracking. If a
            // file is modified concurrently, results are undefined (documented).
            let mmap = unsafe { Mmap::map(&file) }
                .with_context(|| format!("Cannot mmap {}", p.display()))?;
            total = total.saturating_add(count_lines(&mmap));
            files.push(MappedFile {
                _path: p.clone(),
                mmap,
            });
        }

        if total == 0 {
            bail!("No passwords found in {}", path.display());
        }

        Ok(Self {
            files,
            file_index: 0,
            offset: 0,
            total,
            dedup,
            seen: HashSet::new(),
        })
    }

    fn next_line(&mut self) -> Option<Box<[u8]>> {
        loop {
            if self.file_index >= self.files.len() {
                return None;
            }
            let data = &self.files[self.file_index].mmap;
            if self.offset >= data.len() {
                self.file_index += 1;
                self.offset = 0;
                continue;
            }

            let rest = &data[self.offset..];
            let line_end = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
            let line = trim_ascii(&rest[..line_end]);
            self.offset += line_end + usize::from(line_end < rest.len());

            if line.is_empty() || line.starts_with(b"#") {
                continue;
            }
            if self.dedup {
                let h = fnv1a(line);
                if !self.seen.insert(h) {
                    continue;
                }
            }
            return Some(line.to_vec().into_boxed_slice());
        }
    }
}

fn collect_txt_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in read_dir(&d)? {
            let p = entry?.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "txt") {
                out.push(p);
            }
        }
    }
    Ok(())
}

/// Count candidate lines, mirroring the filtering done by `next_line`
/// (skip blank lines and `#` comments).
fn count_lines(data: &[u8]) -> u64 {
    let mut count = 0u64;
    let mut start = 0usize;
    while start < data.len() {
        let rest = &data[start..];
        let end = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
        let line = trim_ascii(&rest[..end]);
        if !line.is_empty() && !line.starts_with(b"#") {
            count += 1;
        }
        start += end + usize::from(end < rest.len());
    }
    count
}

/// Trim ASCII whitespace (including `\r`) from both ends.
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

/// FNV-1a 64-bit hash, used only for optional duplicate suppression.
fn fnv1a(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &b in data {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

impl PasswordSource for DictionarySource {
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>, max: usize) -> bool {
        batch.clear();
        while batch.len() < max {
            match self.next_line() {
                Some(line) => batch.push(line),
                None => break,
            }
        }
        !batch.is_empty()
    }

    fn estimated_total(&self) -> Option<u64> {
        Some(self.total)
    }

    fn checkpoint(&self) -> Option<String> {
        // The offset always points at the start of an unread line.
        Some(format!("{}:{}", self.file_index, self.offset))
    }

    fn restore(&mut self, checkpoint: &str) -> Result<()> {
        let (idx, off) = checkpoint
            .split_once(':')
            .context("Invalid dictionary checkpoint")?;
        self.file_index = idx.parse()?;
        self.offset = off.parse()?;
        if self.file_index > self.files.len() {
            bail!("Checkpoint file index out of range");
        }
        if self.file_index < self.files.len()
            && self.offset > self.files[self.file_index].mmap.len()
        {
            bail!("Checkpoint offset out of range");
        }
        // Dedup state cannot be reconstructed; duplicates may be retried
        // after a resume when dedup is enabled. Harmless, only extra work.
        self.seen.clear();
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

    fn temp_wordlist(contents: &[u8]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(contents).unwrap();
        f.flush().unwrap();
        f
    }

    fn collect(src: &mut DictionarySource) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut batch = Vec::new();
        while src.fill_batch(&mut batch, 1024) {
            out.extend(batch.drain(..).map(|b| b.to_vec()));
        }
        out
    }

    #[test]
    fn streams_all_lines_including_non_utf8() {
        let f = temp_wordlist(b"first\r\n# comment\n\n\xc3\xdc\xc2\xeb\nlast");
        let mut src = DictionarySource::new(f.path()).unwrap();
        let lines = collect(&mut src);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], b"first");
        assert_eq!(lines[1], &[0xC3, 0xDC, 0xC2, 0xEB]);
        assert_eq!(lines[2], b"last");
        assert_eq!(src.estimated_total(), Some(3));
    }

    #[test]
    fn dedups_when_requested() {
        let f = temp_wordlist(b"a\nb\na\nc\nb\n");
        let mut src = DictionarySource::with_dedup(f.path(), true).unwrap();
        let lines = collect(&mut src);
        assert_eq!(lines, vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
    }

    #[test]
    fn checkpoint_roundtrip() {
        // "one\n" is 4 bytes; start of "two" is offset 4.
        let f = temp_wordlist(b"one\ntwo\nthree\nfour\n");

        let mut src = DictionarySource::new(f.path()).unwrap();
        src.restore("0:4").unwrap();
        let lines = collect(&mut src);
        assert_eq!(lines, vec![b"two".to_vec(), b"three".to_vec(), b"four".to_vec()]);

        // End-of-file checkpoint points past the only file.
        let mut src2 = DictionarySource::new(f.path()).unwrap();
        src2.restore("1:0").unwrap();
        assert!(collect(&mut src2).is_empty());
    }

    #[test]
    fn directory_mode_is_sorted_and_complete() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("b.txt"), b"b1\nb2\n").unwrap();
        std::fs::write(dir.path().join("a.txt"), b"a1\n").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/c.txt"), b"c1\n").unwrap();
        std::fs::write(dir.path().join("ignored.md"), b"nope\n").unwrap();

        let mut src = DictionarySource::new(dir.path()).unwrap();
        let lines = collect(&mut src);
        assert_eq!(
            lines,
            vec![
                b"a1".to_vec(),
                b"b1".to_vec(),
                b"b2".to_vec(),
                b"c1".to_vec()
            ]
        );
    }
}
