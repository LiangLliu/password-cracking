use super::PasswordSource;
use anyhow::Result;
use std::path::Path;

/// A mutation rule applied to dictionary words (hashcat-style).
#[derive(Debug, Clone)]
pub enum Rule {
    Capitalize,
    Upper,
    Lower,
    L33t,
    AppendDigits(u32),
    ToggleCase,
}

pub struct RuleSource {
    inner: super::dictionary::DictionarySource,
    _rules: Vec<Rule>,
}

impl RuleSource {
    pub fn new(path: &Path, rules: Vec<Rule>) -> Result<Self> {
        let inner = super::dictionary::DictionarySource::new(path)?;
        Ok(Self {
            inner,
            _rules: rules,
        })
    }
}

impl PasswordSource for RuleSource {
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>) -> bool {
        // TODO: Phase 6 — apply rules to dictionary words
        self.inner.fill_batch(batch)
    }

    fn estimated_total(&self) -> Option<u64> {
        self.inner.estimated_total()
    }

    fn checkpoint(&self) -> Option<String> {
        self.inner.checkpoint()
    }

    fn restore(&mut self, checkpoint: &str) -> Result<()> {
        self.inner.restore(checkpoint)
    }

    fn name(&self) -> &str {
        "hybrid"
    }
}
