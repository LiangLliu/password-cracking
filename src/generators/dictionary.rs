use super::{CandidateBatch, PasswordSource, WorkShard};
use anyhow::{Context, Result, bail};
use memchr::memchr;
use memmap2::Mmap;
use std::collections::HashSet;
use std::fs::{File, read_dir};
use std::hash::{BuildHasherDefault, Hasher};
use std::io::{BufRead, BufReader, Stdin};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Files above this byte threshold sample the prefix for progress estimation
/// instead of scanning the entire wordlist synchronously at startup.
const EXACT_COUNT_LIMIT: u64 = 128 * 1024 * 1024;
const SAMPLE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 << 8) | u64::from(b);
        }
    }

    fn write_u64(&mut self, i: u64) {
        self.0 = i;
    }
}

type U64HashSet = HashSet<u64, BuildHasherDefault<IdentityHasher>>;

/// Streaming dictionary source backed by memory-mapped files (or stdin via `-`).
///
/// Unlike loading every line into memory, this keeps only file mappings (paged
/// in on demand by the OS) and a byte cursor. Memory usage stays flat for
/// arbitrarily large wordlists.
///
/// Duplicate elimination is optional (`dedup`): the default is off, because
/// exact dedup requires remembering every candidate and would defeat the
/// streaming goal. With `dedup` enabled only 64-bit hashes are retained.
pub struct DictionarySource {
    backend: DictBackend,
    dedup: bool,
    seen: U64HashSet,
    config_sig: String,
}

enum DictBackend {
    Mapped {
        files: Vec<MappedFile>,
        file_index: usize,
        /// Byte offset of the next unread line in `files[file_index]`.
        offset: usize,
        /// Estimated number of candidates (`None` when deferred on huge files).
        total: Option<u64>,
        total_bytes: u64,
    },
    Stdin {
        reader: BufReader<Stdin>,
        line_buf: Vec<u8>,
        exhausted: bool,
    },
}

struct MappedFile {
    _path: PathBuf,
    mmap: Arc<Mmap>,
}

impl DictionarySource {
    pub fn new(path: &Path) -> Result<Self> {
        Self::with_dedup(path, false)
    }

    pub fn with_dedup(path: &Path, dedup: bool) -> Result<Self> {
        if path == Path::new("-") {
            return Ok(Self {
                backend: DictBackend::Stdin {
                    reader: BufReader::new(std::io::stdin()),
                    line_buf: Vec::with_capacity(256),
                    exhausted: false,
                },
                dedup,
                seen: U64HashSet::default(),
                config_sig: "dict:stdin".to_string(),
            });
        }

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
        let mut total_bytes = 0u64;
        let mut sig = String::from("dict:");
        use std::fmt::Write;

        for p in &paths {
            let file = File::open(p).with_context(|| format!("Cannot read {}", p.display()))?;
            let meta_len = file.metadata().map(|m| m.len()).unwrap_or(0);
            total_bytes = total_bytes.saturating_add(meta_len);
            let canonical = p.canonicalize().unwrap_or_else(|_| p.clone());
            let _ = write!(sig, "{}#{};", canonical.display(), meta_len);

            // SAFETY: wordlists are treated as immutable while cracking. If a
            // file is modified concurrently, results are undefined (documented).
            let mmap = unsafe { Mmap::map(&file) }
                .with_context(|| format!("Cannot mmap {}", p.display()))?;
            files.push(MappedFile {
                _path: p.clone(),
                mmap: Arc::new(mmap),
            });
        }

        if total_bytes == 0 {
            bail!("No passwords found in {}", path.display());
        }

        let total = if total_bytes <= EXACT_COUNT_LIMIT {
            let mut exact = 0u64;
            for f in &files {
                exact = exact.saturating_add(count_lines(&f.mmap));
            }
            if exact == 0 {
                bail!("No passwords found in {}", path.display());
            }
            Some(exact)
        } else {
            estimate_lines_sampled(&files, total_bytes)
        };

        Ok(Self {
            backend: DictBackend::Mapped {
                files,
                file_index: 0,
                offset: 0,
                total,
                total_bytes,
            },
            dedup,
            seen: U64HashSet::default(),
            config_sig: sig,
        })
    }

    /// Reads the next filtered candidate line into `out`, returning `false` on EOF.
    pub(crate) fn next_word_into(&mut self, out: &mut Vec<u8>) -> bool {
        out.clear();
        match &mut self.backend {
            DictBackend::Mapped {
                files,
                file_index,
                offset,
                ..
            } => loop {
                if *file_index >= files.len() {
                    return false;
                }
                let data = &files[*file_index].mmap;
                if *offset >= data.len() {
                    *file_index += 1;
                    *offset = 0;
                    continue;
                }

                let rest = &data[*offset..];
                let line_end = memchr(b'\n', rest).unwrap_or(rest.len());
                let line = rest[..line_end].trim_ascii();
                *offset += line_end + usize::from(line_end < rest.len());

                if line.is_empty() || line.starts_with(b"#") {
                    continue;
                }
                if self.dedup && !self.seen.insert(fnv1a(line)) {
                    continue;
                }
                out.extend_from_slice(line);
                return true;
            },
            DictBackend::Stdin {
                reader,
                line_buf,
                exhausted,
            } => loop {
                if *exhausted {
                    return false;
                }
                line_buf.clear();
                match reader.read_until(b'\n', line_buf) {
                    Ok(0) | Err(_) => {
                        *exhausted = true;
                        return false;
                    }
                    Ok(_) => {
                        let line = line_buf.trim_ascii();
                        if line.is_empty() || line.starts_with(b"#") {
                            continue;
                        }
                        if self.dedup && !self.seen.insert(fnv1a(line)) {
                            continue;
                        }
                        out.extend_from_slice(line);
                        return true;
                    }
                }
            },
        }
    }

    fn next_mapped_line<'a>(
        files: &'a [MappedFile],
        file_index: &mut usize,
        offset: &mut usize,
        dedup: bool,
        seen: &mut U64HashSet,
    ) -> Option<&'a [u8]> {
        loop {
            if *file_index >= files.len() {
                return None;
            }
            let data = &files[*file_index].mmap;
            if *offset >= data.len() {
                *file_index += 1;
                *offset = 0;
                continue;
            }

            let rest = &data[*offset..];
            let line_end = memchr(b'\n', rest).unwrap_or(rest.len());
            let line = rest[..line_end].trim_ascii();
            *offset += line_end + usize::from(line_end < rest.len());

            if line.is_empty() || line.starts_with(b"#") {
                continue;
            }
            if dedup && !seen.insert(fnv1a(line)) {
                continue;
            }
            return Some(line);
        }
    }
}

fn collect_txt_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut visited_dirs = HashSet::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let canon = d.canonicalize().unwrap_or_else(|_| d.clone());
        if !visited_dirs.insert(canon) {
            continue;
        }
        for entry in read_dir(&d)? {
            let p = entry?.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("txt"))
            {
                out.push(p);
            }
        }
    }
    Ok(())
}

/// Count candidate lines, mirroring the filtering done by `next_word_into`
/// (skip blank lines and `#` comments).
fn count_lines(data: &[u8]) -> u64 {
    let mut count = 0u64;
    let mut start = 0usize;
    while start < data.len() {
        let rest = &data[start..];
        let end = memchr(b'\n', rest).unwrap_or(rest.len());
        let line = rest[..end].trim_ascii();
        if !line.is_empty() && !line.starts_with(b"#") {
            count += 1;
        }
        start += end + usize::from(end < rest.len());
    }
    count
}

fn estimate_lines_sampled(files: &[MappedFile], total_bytes: u64) -> Option<u64> {
    let mut sampled_bytes = 0usize;
    let mut sampled_lines = 0u64;
    for f in files {
        let data = &f.mmap;
        if data.is_empty() {
            continue;
        }
        let budget = SAMPLE_BYTES.saturating_sub(sampled_bytes);
        if budget == 0 {
            break;
        }
        let limit = data.len().min(budget);
        let slice_end = if limit < data.len() {
            memchr(b'\n', &data[limit..])
                .map(|p| limit + p + 1)
                .unwrap_or(data.len())
        } else {
            data.len()
        };
        sampled_lines = sampled_lines.saturating_add(count_lines(&data[..slice_end]));
        sampled_bytes = sampled_bytes.saturating_add(slice_end);
    }
    if sampled_bytes == 0 || sampled_lines == 0 {
        return None;
    }
    let est = (u128::from(total_bytes) * u128::from(sampled_lines)) / (sampled_bytes as u128);
    u64::try_from(est.max(1)).ok()
}

/// FNV-1a 64-bit hash, used only for optional duplicate suppression.
#[inline]
fn fnv1a(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &b in data {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

impl PasswordSource for DictionarySource {
    fn fill_batch(&mut self, batch: &mut CandidateBatch, max: usize) -> bool {
        batch.clear();
        if max == 0 {
            return false;
        }
        match &mut self.backend {
            DictBackend::Mapped {
                files,
                file_index,
                offset,
                ..
            } => {
                while batch.len() < max {
                    match Self::next_mapped_line(
                        files,
                        file_index,
                        offset,
                        self.dedup,
                        &mut self.seen,
                    ) {
                        Some(line) => {
                            if !batch.push(line) {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                !batch.is_empty()
            }
            DictBackend::Stdin { .. } => {
                let mut tmp = Vec::with_capacity(128);
                while batch.len() < max && self.next_word_into(&mut tmp) {
                    if !batch.push(&tmp) {
                        break;
                    }
                }
                !batch.is_empty()
            }
        }
    }

    fn take_shard(&mut self, max: usize) -> Option<WorkShard> {
        // Keep small batches exact for slow verifiers (Office/PDF-R6/AES);
        // switch to zero-copy byte-range shards once adaptive batch size grows.
        if self.dedup || max <= 256 {
            return None;
        }
        let DictBackend::Mapped {
            files,
            file_index,
            offset,
            ..
        } = &mut self.backend
        else {
            return None;
        };

        while *file_index < files.len() && *offset >= files[*file_index].mmap.len() {
            *file_index += 1;
            *offset = 0;
        }
        if *file_index >= files.len() {
            return None;
        }

        let mmap = Arc::clone(&files[*file_index].mmap);
        let start = *offset;
        let target_bytes = (max.saturating_mul(12)).min(4 * 1024 * 1024);
        let approx_end = start.saturating_add(target_bytes);
        let end = if approx_end >= mmap.len() {
            mmap.len()
        } else {
            memchr(b'\n', &mmap[approx_end..])
                .map(|p| approx_end + p + 1)
                .unwrap_or(mmap.len())
        };

        *offset = end;
        if *offset >= mmap.len() {
            *file_index += 1;
            *offset = 0;
        }

        Some(WorkShard::DictSlice { mmap, start, end })
    }

    fn estimated_total(&self) -> Option<u64> {
        match &self.backend {
            DictBackend::Mapped { total, .. } => *total,
            DictBackend::Stdin { .. } => None,
        }
    }

    fn remaining_estimate(&self) -> Option<u64> {
        match &self.backend {
            DictBackend::Mapped {
                files,
                file_index,
                offset,
                total,
                total_bytes,
            } => {
                let total = (*total)?;
                if *file_index == 0 && *offset == 0 {
                    return Some(total);
                }
                if *file_index >= files.len() {
                    return Some(0);
                }
                let mut rem_bytes = files[*file_index].mmap.len().saturating_sub(*offset) as u64;
                for f in &files[*file_index + 1..] {
                    rem_bytes = rem_bytes.saturating_add(f.mmap.len() as u64);
                }
                if *total_bytes == 0 {
                    return Some(0);
                }
                let est = (u128::from(total) * u128::from(rem_bytes)) / u128::from(*total_bytes);
                Some(u64::try_from(est).unwrap_or(total))
            }
            DictBackend::Stdin { .. } => None,
        }
    }

    fn checkpoint(&self) -> Option<String> {
        match &self.backend {
            DictBackend::Mapped {
                file_index, offset, ..
            } => Some(format!("{file_index}:{offset}")),
            DictBackend::Stdin { .. } => None,
        }
    }

    fn restore(&mut self, checkpoint: &str) -> Result<()> {
        let DictBackend::Mapped {
            files,
            file_index,
            offset,
            ..
        } = &mut self.backend
        else {
            bail!("Cannot restore checkpoint for stdin wordlist");
        };
        let (idx, off) = checkpoint
            .split_once(':')
            .context("Invalid dictionary checkpoint")?;
        let parsed_idx: usize = idx.parse()?;
        let parsed_off: usize = off.parse()?;
        if parsed_idx > files.len() {
            bail!("Checkpoint file index out of range");
        }
        if parsed_idx < files.len() && parsed_off > files[parsed_idx].mmap.len() {
            bail!("Checkpoint offset out of range");
        }
        *file_index = parsed_idx;
        *offset = parsed_off;
        // Dedup state cannot be reconstructed; duplicates may be retried
        // after a resume when dedup is enabled. Harmless, only extra work.
        self.seen.clear();
        Ok(())
    }

    fn config_signature(&self) -> String {
        self.config_sig.clone()
    }

    fn name(&self) -> &str {
        "dictionary"
    }
}

pub(super) fn execute_shard(
    mmap: &[u8],
    start: usize,
    end: usize,
    verifier: &dyn crate::formats::PasswordVerifier,
    stop: &std::sync::atomic::AtomicBool,
    global_attempts: &std::sync::atomic::AtomicU64,
    local_batch: &mut CandidateBatch,
) -> Option<Vec<u8>> {
    use super::add_and_maybe_flush;
    use std::sync::atomic::Ordering;

    let mut pos = start;
    let data = &mmap[..end.min(mmap.len())];
    let mut pending_attempts = 0u64;
    local_batch.clear();

    while pos < data.len() && !stop.load(Ordering::Relaxed) {
        let rest = &data[pos..];
        let line_end = memchr(b'\n', rest).unwrap_or(rest.len());
        let line = rest[..line_end].trim_ascii();
        pos += line_end + usize::from(line_end < rest.len());

        if line.is_empty() || line.starts_with(b"#") {
            continue;
        }

        if !local_batch.push(line) {
            if let Some(hit) = verify_and_clear_batch(
                local_batch,
                verifier,
                stop,
                &mut pending_attempts,
                global_attempts,
            ) {
                return Some(hit);
            }
            let _ = local_batch.push(line);
        }

        if local_batch.len() >= 4096
            && let Some(hit) = verify_and_clear_batch(
                local_batch,
                verifier,
                stop,
                &mut pending_attempts,
                global_attempts,
            )
        {
            return Some(hit);
        }
    }

    if !stop.load(Ordering::Relaxed)
        && let Some(hit) = verify_and_clear_batch(
            local_batch,
            verifier,
            stop,
            &mut pending_attempts,
            global_attempts,
        )
    {
        return Some(hit);
    }
    local_batch.clear();

    add_and_maybe_flush(&mut pending_attempts, 0, global_attempts, true);
    None
}

fn verify_and_clear_batch(
    local_batch: &mut CandidateBatch,
    verifier: &dyn crate::formats::PasswordVerifier,
    stop: &std::sync::atomic::AtomicBool,
    pending_attempts: &mut u64,
    global_attempts: &std::sync::atomic::AtomicU64,
) -> Option<Vec<u8>> {
    use super::add_and_maybe_flush;

    if local_batch.is_empty() {
        return None;
    }
    let res = verifier.verify_batch(local_batch, stop);
    add_and_maybe_flush(pending_attempts, res.attempts, global_attempts, false);
    let matched = res
        .matched
        .map(|idx| local_batch.get(idx).unwrap().to_vec());
    local_batch.clear();
    if matched.is_some() {
        add_and_maybe_flush(pending_attempts, 0, global_attempts, true);
    }
    matched
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
        let mut batch = CandidateBatch::new();
        while src.fill_batch(&mut batch, 1024) {
            out.extend(batch.iter().map(|b| b.to_vec()));
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

    #[test]
    fn shard_splits_on_line_boundaries() {
        let mut data = Vec::new();
        for i in 0..1000 {
            use std::io::Write;
            writeln!(data, "word{i:04}").unwrap();
        }
        let f = temp_wordlist(&data);
        let mut src = DictionarySource::new(f.path()).unwrap();
        let mut collected = Vec::new();
        while let Some(shard) = src.take_shard(512) {
            if let WorkShard::DictSlice { mmap, start, end } = shard {
                let mut pos = start;
                while pos < end {
                    let rest = &mmap[pos..end];
                    let line_end = memchr(b'\n', rest).unwrap_or(rest.len());
                    let line = rest[..line_end].trim_ascii();
                    if !line.is_empty() && !line.starts_with(b"#") {
                        collected.push(line.to_vec());
                    }
                    pos += line_end + usize::from(line_end < rest.len());
                }
            }
        }
        assert_eq!(collected.len(), 1000);
        assert_eq!(collected[0], b"word0000");
        assert_eq!(collected[999], b"word0999");
    }
}
