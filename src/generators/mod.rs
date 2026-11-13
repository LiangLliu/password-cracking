use anyhow::Result;
use memmap2::Mmap;
use std::sync::Arc;

pub mod brute_force;
pub mod dictionary;
pub mod mask;
pub mod rules;

/// Reusable flat buffer holding a batch of candidate passwords without
/// per-candidate heap allocations.
#[derive(Debug, Default, Clone)]
pub struct CandidateBatch {
    data: Vec<u8>,
    /// Cumulative end byte offset in `data` for each candidate.
    ends: Vec<u32>,
}

impl CandidateBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(candidates: usize, total_bytes: usize) -> Self {
        Self {
            data: Vec::with_capacity(total_bytes),
            ends: Vec::with_capacity(candidates),
        }
    }

    pub fn clear(&mut self) {
        self.data.clear();
        self.ends.clear();
    }

    pub fn len(&self) -> usize {
        self.ends.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    /// Appends a candidate slice. Returns `false` if the batch buffer would
    /// exceed 4 GiB (`u32::MAX`).
    #[inline]
    pub fn push(&mut self, candidate: &[u8]) -> bool {
        let Some(new_end) = self
            .data
            .len()
            .checked_add(candidate.len())
            .and_then(|n| u32::try_from(n).ok())
        else {
            return false;
        };
        self.data.extend_from_slice(candidate);
        self.ends.push(new_end);
        true
    }

    /// Appends a candidate built by `prefix` followed by `suffix`.
    #[inline]
    pub fn push_concat(&mut self, prefix: &[u8], suffix: &[u8]) -> bool {
        let Some(new_end) = self
            .data
            .len()
            .checked_add(prefix.len())
            .and_then(|n| n.checked_add(suffix.len()))
            .and_then(|n| u32::try_from(n).ok())
        else {
            return false;
        };
        self.data.extend_from_slice(prefix);
        self.data.extend_from_slice(suffix);
        self.ends.push(new_end);
        true
    }

    /// Appends a candidate written directly into the backing buffer by `writer`.
    #[inline]
    pub fn push_with(&mut self, writer: impl FnOnce(&mut Vec<u8>)) -> bool {
        let prev_len = self.data.len();
        writer(&mut self.data);
        let Ok(new_end) = u32::try_from(self.data.len()) else {
            self.data.truncate(prev_len);
            return false;
        };
        self.ends.push(new_end);
        true
    }

    #[inline]
    pub fn get(&self, index: usize) -> Option<&[u8]> {
        let end = *self.ends.get(index)? as usize;
        let start = if index == 0 {
            0
        } else {
            self.ends[index - 1] as usize
        };
        Some(&self.data[start..end])
    }

    #[inline]
    pub fn iter(&self) -> CandidateIter<'_> {
        CandidateIter {
            batch: self,
            idx: 0,
            start: 0,
        }
    }
}

impl<'a> IntoIterator for &'a CandidateBatch {
    type Item = &'a [u8];
    type IntoIter = CandidateIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct CandidateIter<'a> {
    batch: &'a CandidateBatch,
    idx: usize,
    start: usize,
}

impl<'a> Iterator for CandidateIter<'a> {
    type Item = &'a [u8];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let &end = self.batch.ends.get(self.idx)?;
        let end = end as usize;
        let slice = &self.batch.data[self.start..end];
        self.start = end;
        self.idx += 1;
        Some(slice)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let rem = self.batch.ends.len() - self.idx;
        (rem, Some(rem))
    }
}

impl ExactSizeIterator for CandidateIter<'_> {}

/// Internal zero-copy work slice handed to workers so they can generate or
/// scan candidates in parallel without central materialization bottlenecks.
#[doc(hidden)]
pub enum WorkShard {
    BruteForce {
        charset: Arc<[u8]>,
        len: usize,
        indices: [usize; 20],
        count: u64,
    },
    Mask {
        positions: Arc<[Vec<u8>]>,
        indices: [u16; 128],
        pos_count: usize,
        count: u64,
    },
    DictSlice {
        mmap: Arc<Mmap>,
        start: usize,
        end: usize,
    },
}

impl WorkShard {
    /// Executes this work shard directly inside a worker thread, returning the
    /// matching password bytes if found.
    pub fn execute(
        self,
        verifier: &dyn crate::formats::PasswordVerifier,
        stop: &std::sync::atomic::AtomicBool,
        global_attempts: &std::sync::atomic::AtomicU64,
        local_batch: &mut CandidateBatch,
    ) -> Option<Vec<u8>> {
        match self {
            Self::BruteForce {
                charset,
                len,
                indices,
                count,
            } => brute_force::execute_shard(
                &charset,
                len,
                indices,
                count,
                verifier,
                stop,
                global_attempts,
            ),
            Self::Mask {
                positions,
                indices,
                pos_count,
                count,
            } => mask::execute_shard(
                &positions,
                indices,
                pos_count,
                count,
                verifier,
                stop,
                global_attempts,
            ),
            Self::DictSlice { mmap, start, end } => dictionary::execute_shard(
                &mmap,
                start,
                end,
                verifier,
                stop,
                global_attempts,
                local_batch,
            ),
        }
    }
}

#[inline]
pub(crate) fn add_and_maybe_flush(
    pending: &mut u64,
    delta: u64,
    global: &std::sync::atomic::AtomicU64,
    force: bool,
) {
    *pending += delta;
    if (force && *pending > 0) || *pending >= 4096 {
        global.fetch_add(*pending, std::sync::atomic::Ordering::Relaxed);
        *pending = 0;
    }
}

/// Streaming source of password candidates.
///
/// Implementations must never hold the entire search space in memory.
/// Use [`fill_batch`] to pull a chunk of candidates at a time.
pub trait PasswordSource: Send {
    /// Fills up to `max` candidates into `batch`. Returns `false` when the
    /// source is exhausted (the batch may still contain a final partial set).
    /// Implementations should call `batch.clear()` before filling.
    fn fill_batch(&mut self, batch: &mut CandidateBatch, max: usize) -> bool;

    /// Optionally yields a compact work shard so workers can generate or scan
    /// candidates in parallel. Returning `None` falls back to `fill_batch`.
    #[doc(hidden)]
    fn take_shard(&mut self, _max: usize) -> Option<WorkShard> {
        None
    }

    /// Estimated total candidates for progress display. `None` if unknown.
    fn estimated_total(&self) -> Option<u64>;

    /// Estimated remaining candidates from the current position (used for
    /// progress bars after resuming from a checkpoint).
    fn remaining_estimate(&self) -> Option<u64> {
        self.estimated_total()
    }

    /// Serializes current position for resume. `None` if unsupported.
    fn checkpoint(&self) -> Option<String>;

    /// Restores from a checkpoint string produced by [`checkpoint`].
    fn restore(&mut self, checkpoint: &str) -> anyhow::Result<()>;

    /// Configuration fingerprint checked on `--resume` so mismatched attack
    /// parameters are rejected instead of resuming with invalid state.
    fn config_signature(&self) -> String {
        String::new()
    }

    /// Human-readable name for logging.
    fn name(&self) -> &str;
}

/// Configuration for the password generator.
#[derive(Debug, Clone)]
pub enum GeneratorMode {
    Dictionary {
        path: std::path::PathBuf,
        /// Deduplicate candidates. Requires remembering hashes (memory grows
        /// with unique passwords), so it is off by default.
        dedup: bool,
    },
    BruteForce {
        charset: String,
        min_length: usize,
        max_length: usize,
    },
    Hybrid {
        dictionary_path: std::path::PathBuf,
        rules: Vec<rules::Rule>,
    },
    Mask {
        mask: String,
    },
}

/// Creates a password source from the given mode.
pub fn create_source(mode: GeneratorMode) -> Result<Box<dyn PasswordSource>> {
    match mode {
        GeneratorMode::Dictionary { path, dedup } => {
            Ok(Box::new(dictionary::DictionarySource::with_dedup(&path, dedup)?))
        }
        GeneratorMode::BruteForce {
            charset,
            min_length,
            max_length,
        } => Ok(Box::new(brute_force::BruteForceSource::new(charset, min_length, max_length)?)),
        GeneratorMode::Hybrid {
            dictionary_path,
            rules,
        } => Ok(Box::new(rules::RuleSource::new(&dictionary_path, rules)?)),
        GeneratorMode::Mask { mask } => Ok(Box::new(mask::MaskSource::new(&mask)?)),
    }
}
