use super::PasswordSource;
use anyhow::Result;
use std::path::Path;

/// A mutation rule applied to dictionary words (hashcat-style).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    Capitalize,
    Upper,
    Lower,
    L33t,
    AppendDigits(u32),
    ToggleCase,
    Reverse,
    Duplicate,
}

impl Rule {
    /// Apply this rule to a password, producing a new password.
    pub fn apply(&self, password: &[u8]) -> Option<Vec<u8>> {
        match self {
            Self::Capitalize => {
                if password.is_empty() {
                    return None;
                }
                let mut result = password.to_vec();
                result[0] = result[0].to_ascii_uppercase();
                for b in &mut result[1..] {
                    *b = b.to_ascii_lowercase();
                }
                Some(result)
            }
            Self::Upper => {
                if password.iter().all(|b| b.is_ascii_uppercase()) {
                    return None;
                }
                Some(password.iter().map(|b| b.to_ascii_uppercase()).collect())
            }
            Self::Lower => {
                if password.iter().all(|b| b.is_ascii_lowercase()) {
                    return None;
                }
                Some(password.iter().map(|b| b.to_ascii_lowercase()).collect())
            }
            Self::L33t => {
                let mut changed = false;
                let result: Vec<u8> = password
                    .iter()
                    .map(|&b| match b {
                        b'a' | b'A' => {
                            changed = true;
                            b'@'
                        }
                        b'e' | b'E' => {
                            changed = true;
                            b'3'
                        }
                        b'i' | b'I' => {
                            changed = true;
                            b'1'
                        }
                        b'o' | b'O' => {
                            changed = true;
                            b'0'
                        }
                        b's' | b'S' => {
                            changed = true;
                            b'$'
                        }
                        other => other,
                    })
                    .collect();
                if changed { Some(result) } else { None }
            }
            Self::ToggleCase => {
                if password.iter().all(|b| !b.is_ascii_alphabetic()) {
                    return None;
                }
                Some(
                    password
                        .iter()
                        .map(|b| {
                            if b.is_ascii_uppercase() {
                                b.to_ascii_lowercase()
                            } else if b.is_ascii_lowercase() {
                                b.to_ascii_uppercase()
                            } else {
                                *b
                            }
                        })
                        .collect(),
                )
            }
            Self::Reverse => {
                if password.len() < 2 {
                    return None;
                }
                let mut result = password.to_vec();
                result.reverse();
                Some(result)
            }
            Self::Duplicate => {
                let mut result = Vec::with_capacity(password.len() * 2);
                result.extend_from_slice(password);
                result.extend_from_slice(password);
                Some(result)
            }
            Self::AppendDigits(_max) => {
                // For fill_batch, we can't return multiple variants here.
                // AppendDigits is handled specially in RuleSource::fill_batch.
                None
            }
        }
    }
}

/// Hybrid password source: dictionary words + mutation rules.
///
/// For each dictionary word, produces the original word plus all rule
/// variants. If AppendDigits is active, also generates word+0, word+1, ...
/// word+N for each word.
pub struct RuleSource {
    inner: super::dictionary::DictionarySource,
    rules: Vec<Rule>,
    /// Buffer of pending mutations for the current dictionary word.
    pending: Vec<Box<[u8]>>,
}

impl RuleSource {
    pub fn new(path: &Path, rules: Vec<Rule>) -> Result<Self> {
        let inner = super::dictionary::DictionarySource::new(path)?;
        Ok(Self {
            inner,
            rules,
            pending: Vec::new(),
        })
    }

    /// Generate all mutations for a single dictionary word.
    fn generate_mutations(&self, word: &[u8]) -> Vec<Box<[u8]>> {
        let mut results: Vec<Box<[u8]>> = Vec::new();

        // Always include the original word
        results.push(word.to_vec().into_boxed_slice());

        // Apply each rule
        for rule in &self.rules {
            match rule {
                Rule::AppendDigits(max) => {
                    for d in 0..=*max {
                        let mut variant = word.to_vec();
                        variant.extend_from_slice(d.to_string().as_bytes());
                        results.push(variant.into_boxed_slice());
                    }
                }
                other => {
                    if let Some(mutated) = other.apply(word) {
                        results.push(mutated.into_boxed_slice());
                    }
                }
            }
        }

        results
    }
}

impl PasswordSource for RuleSource {
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>) -> bool {
        const BATCH: usize = 1024;
        batch.clear();

        // First, drain any pending mutations from the previous word.
        while !self.pending.is_empty() && batch.len() < BATCH {
            batch.push(self.pending.remove(0));
        }

        // Then, keep pulling dictionary words and generating mutations
        // until the batch is full or the dictionary is exhausted.
        let mut dict_batch: Vec<Box<[u8]>> = Vec::new();
        while batch.len() < BATCH {
            dict_batch.clear();
            if !self.inner.fill_batch(&mut dict_batch) {
                break;
            }
            for word in dict_batch.drain(..) {
                let mutations = self.generate_mutations(&word);
                for m in mutations {
                    if batch.len() < BATCH {
                        batch.push(m);
                    } else {
                        self.pending.push(m);
                    }
                }
            }
        }

        !batch.is_empty()
    }

    fn estimated_total(&self) -> Option<u64> {
        let base = self.inner.estimated_total()?;
        // Estimate multiplier: 1 (original) + rules + append_digits
        let rule_count = self.rules.len() as u64;
        let append_digits_expansion: u64 = self
            .rules
            .iter()
            .filter_map(|r| {
                if let Rule::AppendDigits(max) = r {
                    Some(*max as u64 + 1)
                } else {
                    None
                }
            })
            .sum();
        let multiplier = 1 + rule_count + append_digits_expansion;
        Some(base.saturating_mul(multiplier))
    }

    fn checkpoint(&self) -> Option<String> {
        // Delegate to inner dictionary source.
        // Note: pending mutations are lost on restore (minor imprecision).
        self.inner.checkpoint()
    }

    fn restore(&mut self, checkpoint: &str) -> Result<()> {
        self.pending.clear();
        self.inner.restore(checkpoint)
    }

    fn name(&self) -> &str {
        "hybrid"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capitalize() {
        let result = Rule::Capitalize.apply(b"hello").unwrap();
        assert_eq!(result, b"Hello");
    }

    #[test]
    fn l33t() {
        let result = Rule::L33t.apply(b"password").unwrap();
        assert_eq!(result, b"p@$$w0rd");
    }

    #[test]
    fn append_digits_not_in_apply() {
        assert!(Rule::AppendDigits(4).apply(b"test").is_none());
    }
}
