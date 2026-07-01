use anyhow::Result;

pub mod brute_force;
pub mod dictionary;
pub mod rules;

/// Streaming source of password candidates.
///
/// Implementations must never hold the entire search space in memory.
/// Use [`fill_batch`] to pull a chunk of candidates at a time.
pub trait PasswordSource: Send {
    /// Fills `batch` with candidates. Returns `false` when exhausted.
    /// The batch is cleared before filling.
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>) -> bool;

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
        GeneratorMode::Dictionary { path } => {
            Ok(Box::new(dictionary::DictionarySource::new(&path)?))
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
