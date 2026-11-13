use super::{CandidateBatch, PasswordSource};
use anyhow::{Context, Result, bail};
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
    /// Apply this rule to a password, writing into `out` and returning `true`
    /// if the transformation produced a distinct candidate.
    pub fn apply_into(&self, password: &[u8], out: &mut Vec<u8>) -> bool {
        out.clear();
        match self {
            Self::Capitalize => {
                if password.is_empty() {
                    return false;
                }
                let first = password[0].to_ascii_uppercase();
                let mut changed = first != password[0];
                out.reserve(password.len());
                out.push(first);
                for &b in &password[1..] {
                    let lower = b.to_ascii_lowercase();
                    changed |= lower != b;
                    out.push(lower);
                }
                changed
            }
            Self::Upper => {
                if !password.iter().any(|b| b.is_ascii_lowercase()) {
                    return false;
                }
                out.extend(password.iter().map(|b| b.to_ascii_uppercase()));
                true
            }
            Self::Lower => {
                if !password.iter().any(|b| b.is_ascii_uppercase()) {
                    return false;
                }
                out.extend(password.iter().map(|b| b.to_ascii_lowercase()));
                true
            }
            Self::L33t => {
                let mut changed = false;
                out.reserve(password.len());
                for &b in password {
                    let mapped = match b {
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
                    };
                    out.push(mapped);
                }
                changed
            }
            Self::ToggleCase => {
                if !password.iter().any(|b| b.is_ascii_alphabetic()) {
                    return false;
                }
                out.reserve(password.len());
                for &b in password {
                    let toggled = if b.is_ascii_uppercase() {
                        b.to_ascii_lowercase()
                    } else if b.is_ascii_lowercase() {
                        b.to_ascii_uppercase()
                    } else {
                        b
                    };
                    out.push(toggled);
                }
                true
            }
            Self::Reverse => {
                if password.len() < 2 {
                    return false;
                }
                out.extend(password.iter().rev().copied());
                out.as_slice() != password
            }
            Self::Duplicate => {
                out.reserve(password.len() * 2);
                out.extend_from_slice(password);
                out.extend_from_slice(password);
                !password.is_empty()
            }
            Self::AppendDigits(_max) => {
                // Handled directly inside RuleSource::fill_batch to stream without buffering.
                false
            }
        }
    }

    /// Apply this rule to a password, producing a new password.
    pub fn apply(&self, password: &[u8]) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        if self.apply_into(password, &mut out) {
            Some(out)
        } else {
            None
        }
    }
}

/// Hybrid password source: dictionary words + mutation rules.
///
/// Streams one word and one mutation at a time with O(1) memory and lossless
/// checkpoint/restore cursors, even for large `--append-digits` ranges.
pub struct RuleSource {
    inner: super::dictionary::DictionarySource,
    rules: Vec<Rule>,
    initialized: bool,
    has_current: bool,
    word_start_ckpt: String,
    current_word: Vec<u8>,
    /// 0 = emit original word; 1..=rules.len() = rules[rule_idx - 1].
    rule_idx: usize,
    /// Sub-index within `Rule::AppendDigits`.
    digit: u32,
    scratch: Vec<u8>,
}

impl RuleSource {
    pub fn new(path: &Path, rules: Vec<Rule>) -> Result<Self> {
        let inner = super::dictionary::DictionarySource::new(path)?;
        Ok(Self {
            inner,
            rules,
            initialized: false,
            has_current: false,
            word_start_ckpt: String::new(),
            current_word: Vec::with_capacity(64),
            rule_idx: 0,
            digit: 0,
            scratch: Vec::with_capacity(128),
        })
    }

    fn load_next_word(&mut self) -> bool {
        self.initialized = true;
        self.word_start_ckpt = self.inner.checkpoint().unwrap_or_default();
        self.has_current = self.inner.next_word_into(&mut self.current_word);
        self.rule_idx = 0;
        self.digit = 0;
        self.has_current
    }

    fn rules_multiplier(&self) -> u64 {
        let mut multiplier = 1u64;
        for rule in &self.rules {
            multiplier = multiplier.saturating_add(match rule {
                Rule::AppendDigits(max) => u64::from(*max) + 1,
                _ => 1,
            });
        }
        multiplier
    }
}

/// Format `u32` ASCII digits into `buf` without heap allocation.
#[inline]
fn format_u32_ascii(mut n: u32, buf: &mut [u8; 10]) -> &[u8] {
    let mut pos = buf.len();
    loop {
        pos -= 1;
        buf[pos] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    &buf[pos..]
}

impl PasswordSource for RuleSource {
    fn fill_batch(&mut self, batch: &mut CandidateBatch, max: usize) -> bool {
        batch.clear();
        if max == 0 {
            return false;
        }
        if !self.initialized && !self.load_next_word() {
            return false;
        }

        let mut num_buf = [0u8; 10];
        while batch.len() < max && self.has_current {
            if self.rule_idx == 0 {
                if !batch.push(&self.current_word) {
                    break;
                }
                self.rule_idx = 1;
                self.digit = 0;
            } else if self.rule_idx <= self.rules.len() {
                match self.rules[self.rule_idx - 1] {
                    Rule::AppendDigits(max_d) => {
                        let d_bytes = format_u32_ascii(self.digit, &mut num_buf);
                        if !batch.push_concat(&self.current_word, d_bytes) {
                            break;
                        }
                        if self.digit == max_d {
                            self.rule_idx += 1;
                            self.digit = 0;
                        } else {
                            self.digit += 1;
                        }
                    }
                    other => {
                        if other.apply_into(&self.current_word, &mut self.scratch)
                            && !batch.push(&self.scratch)
                        {
                            break;
                        }
                        self.rule_idx += 1;
                    }
                }
            }

            if self.rule_idx > self.rules.len() {
                self.load_next_word();
            }
        }

        !batch.is_empty()
    }

    fn estimated_total(&self) -> Option<u64> {
        let base = self.inner.estimated_total()?;
        Some(base.saturating_mul(self.rules_multiplier()))
    }

    fn remaining_estimate(&self) -> Option<u64> {
        let base = self.inner.remaining_estimate()?;
        let mut rem = base.saturating_mul(self.rules_multiplier());
        if self.initialized && self.has_current && self.rule_idx <= self.rules.len() {
            // Add remaining variants from current word (whose dictionary cursor already moved past).
            let mut cur_rem = u64::from(self.rule_idx == 0);
            let start_idx = self.rule_idx.saturating_sub(1);
            for (offset, rule) in self.rules[start_idx..].iter().enumerate() {
                cur_rem = cur_rem.saturating_add(match rule {
                    Rule::AppendDigits(max) if offset == 0 && self.rule_idx > 0 => {
                        u64::from(*max).saturating_sub(u64::from(self.digit)) + 1
                    }
                    Rule::AppendDigits(max) => u64::from(*max) + 1,
                    _ => 1,
                });
            }
            rem = rem.saturating_add(cur_rem);
        }
        Some(rem)
    }

    fn checkpoint(&self) -> Option<String> {
        let inner_ckpt =
            if self.initialized && self.has_current && self.rule_idx <= self.rules.len() {
                &self.word_start_ckpt
            } else {
                return self.inner.checkpoint().map(|c| format!("{c}|0|0"));
            };
        Some(format!("{inner_ckpt}|{}|{}", self.rule_idx, self.digit))
    }

    fn restore(&mut self, checkpoint: &str) -> Result<()> {
        let mut parts = checkpoint.splitn(3, '|');
        let inner_ckpt = parts.next().context("Invalid hybrid checkpoint")?;
        let rule_idx: usize = parts
            .next()
            .context("Missing rule index in hybrid checkpoint")?
            .parse()?;
        let digit: u32 = parts
            .next()
            .context("Missing digit index in hybrid checkpoint")?
            .parse()?;

        if rule_idx > self.rules.len() {
            bail!("Hybrid checkpoint rule index {rule_idx} out of range ({})", self.rules.len());
        }
        if rule_idx > 0 {
            if let Rule::AppendDigits(max_d) = self.rules[rule_idx - 1] {
                if digit > max_d {
                    bail!("Hybrid checkpoint digit {digit} exceeds --append-digits {max_d}");
                }
            } else if digit != 0 {
                bail!("Hybrid checkpoint has non-zero digit for non-digit rule");
            }
        } else if digit != 0 {
            bail!("Hybrid checkpoint has non-zero digit at rule 0");
        }

        self.inner.restore(inner_ckpt)?;
        self.load_next_word();
        if !self.has_current && (rule_idx > 0 || digit > 0) {
            bail!("Hybrid checkpoint points past end of dictionary");
        }
        self.rule_idx = rule_idx;
        self.digit = digit;
        Ok(())
    }

    fn config_signature(&self) -> String {
        use std::fmt::Write;
        let mut sig = format!("hybrid:{};rules:", self.inner.config_signature());
        for r in &self.rules {
            match r {
                Rule::Capitalize => sig.push('C'),
                Rule::Upper => sig.push('U'),
                Rule::Lower => sig.push('L'),
                Rule::L33t => sig.push('3'),
                Rule::ToggleCase => sig.push('T'),
                Rule::Reverse => sig.push('R'),
                Rule::Duplicate => sig.push('D'),
                Rule::AppendDigits(n) => {
                    let _ = write!(sig, "A{n}");
                }
            }
            sig.push(',');
        }
        sig
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
        assert_eq!(Rule::Capitalize.apply(b"hello").unwrap(), b"Hello");
        assert!(Rule::Capitalize.apply(b"Hello").is_none());
    }

    #[test]
    fn upper_and_lower_skip_non_alpha() {
        assert!(Rule::Upper.apply(b"1234!").is_none());
        assert!(Rule::Lower.apply(b"1234!").is_none());
        assert_eq!(Rule::Upper.apply(b"abc1").unwrap(), b"ABC1");
        assert_eq!(Rule::Lower.apply(b"ABC1").unwrap(), b"abc1");
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

    #[test]
    fn checkpoint_across_mid_word_mutations_is_lossless() {
        use std::io::Write;

        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "alpha").unwrap();
        writeln!(f, "beta").unwrap();
        writeln!(f, "gamma").unwrap();
        f.flush().unwrap();

        let rules = vec![Rule::Capitalize, Rule::AppendDigits(4)];

        let mut full_src = RuleSource::new(f.path(), rules.clone()).unwrap();
        let mut expected = Vec::new();
        let mut batch = CandidateBatch::new();
        while full_src.fill_batch(&mut batch, 100) {
            expected.extend(batch.iter().map(|b| b.to_vec()));
        }

        // Pull in tiny batches of 3 (splits mid-word inside AppendDigits) and
        // checkpoint/restore after every single batch.
        let mut step_src = RuleSource::new(f.path(), rules.clone()).unwrap();
        let mut replayed = Vec::new();
        loop {
            let ck = step_src.checkpoint().unwrap();
            let mut resumed = RuleSource::new(f.path(), rules.clone()).unwrap();
            resumed.restore(&ck).unwrap();
            assert_eq!(resumed.checkpoint().unwrap(), ck);

            let mut b = CandidateBatch::new();
            if !resumed.fill_batch(&mut b, 3) {
                break;
            }
            replayed.extend(b.iter().map(|x| x.to_vec()));
            step_src = resumed;
        }
        assert_eq!(replayed, expected);
    }
}
