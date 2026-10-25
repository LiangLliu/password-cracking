use anyhow::Result;

pub mod brute_force;
pub mod dictionary;
pub mod rules;

/// Streaming source of password candidates.
///
/// Implementations must never hold the entire search space in memory.
/// Use [`fill_batch`] to pull a chunk of candidates at a time.
pub trait PasswordSource: Send {
    /// Fills up to `max` candidates into `batch`. Returns `false` when the
    /// source is exhausted (the batch may still contain a final partial set).
    /// The batch is cleared before filling.
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>, max: usize) -> bool;

    /// Estimated total candidates for progress display. `None` if unknown.
    fn estimated_total(&self) -> Option<u64>;

    /// Serializes current position for resume. `None` if unsupported.
    fn checkpoint(&self) -> Option<String>;

    /// Restores from a checkpoint string produced by [`checkpoint`].
    fn restore(&mut self, checkpoint: &str) -> anyhow::Result<()>;

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
    }
}
