use super::{CandidateBatch, PasswordSource, WorkShard};
use anyhow::{Result, bail};
use std::sync::Arc;

pub struct BruteForceSource {
    charset: Arc<[u8]>,
    indices: [usize; 20],
    min_length: usize,
    max_length: usize,
    current_length: usize,
    exhausted: bool,
    config_sig: String,
}

impl BruteForceSource {
    pub fn new(charset: String, min_length: usize, max_length: usize) -> Result<Self> {
        if charset.is_empty() {
            bail!("Charset cannot be empty");
        }
        if min_length == 0 {
            bail!("Minimum length must be >= 1");
        }
        if min_length > max_length {
            bail!("Min length > max length");
        }
        if max_length > 20 {
            bail!("Max length too large (limit: 20)");
        }

        // Deduplicate repeated characters in charset while preserving order so
        // custom sets like "aab" do not repeat work.
        let mut seen = [false; 256];
        let mut deduped = Vec::with_capacity(charset.len());
        for b in charset.into_bytes() {
            if !seen[b as usize] {
                seen[b as usize] = true;
                deduped.push(b);
            }
        }
        let charset: Arc<[u8]> = deduped.into();

        let mut sig = String::with_capacity(32 + charset.len() * 2);
        use std::fmt::Write;
        let _ = write!(sig, "bf:{min_length}:{max_length}:");
        for &b in charset.iter() {
            let _ = write!(sig, "{b:02x}");
        }

        Ok(Self {
            charset,
            indices: [0; 20],
            min_length,
            max_length,
            current_length: min_length,
            exhausted: false,
            config_sig: sig,
        })
    }

    fn increment(&mut self) -> bool {
        let len = self.charset.len();
        for i in (0..self.current_length).rev() {
            if self.indices[i] < len - 1 {
                self.indices[i] += 1;
                return true;
            }
            self.indices[i] = 0;
        }
        false
    }

    fn remaining_in_current_length(&self) -> u64 {
        let base = self.charset.len() as u64;
        let mut rem = 0u64;
        let mut weight = 1u64;
        for i in (0..self.current_length).rev() {
            let digits_left = (self.charset.len() - 1 - self.indices[i]) as u64;
            rem = rem.saturating_add(digits_left.saturating_mul(weight));
            weight = weight.saturating_mul(base);
        }
        rem.saturating_add(1)
    }

    fn advance_within_current_length(&mut self, steps: u64) {
        let base = self.charset.len() as u64;
        let mut carry = steps;
        for i in (0..self.current_length).rev() {
            let sum = self.indices[i] as u64 + carry;
            self.indices[i] = (sum % base) as usize;
            carry = sum / base;
            if carry == 0 {
                break;
            }
        }
    }
}

impl PasswordSource for BruteForceSource {
    fn fill_batch(&mut self, batch: &mut CandidateBatch, max: usize) -> bool {
        batch.clear();
        if self.exhausted || max == 0 {
            return false;
        }
        let mut buf = [0u8; 20];
        while batch.len() < max && !self.exhausted {
            let len = self.current_length;
            for (dst, &idx) in buf[..len].iter_mut().zip(&self.indices[..len]) {
                *dst = self.charset[idx];
            }
            if !batch.push(&buf[..len]) {
                break;
            }
            if !self.increment() {
                self.current_length += 1;
                if self.current_length > self.max_length {
                    self.exhausted = true;
                } else {
                    self.indices[..self.current_length].fill(0);
                }
            }
        }
        !batch.is_empty()
    }

    fn take_shard(&mut self, max: usize) -> Option<WorkShard> {
        if self.exhausted || max == 0 {
            return None;
        }
        let start_indices = self.indices;
        let len = self.current_length;
        let rem = self.remaining_in_current_length();
        let req = max as u64;
        let taken = rem.min(req);
        if taken == rem {
            self.current_length += 1;
            self.indices = [0; 20];
            if self.current_length > self.max_length {
                self.exhausted = true;
            }
        } else {
            self.advance_within_current_length(taken);
        }

        Some(WorkShard::BruteForce {
            charset: Arc::clone(&self.charset),
            len,
            indices: start_indices,
            count: taken,
        })
    }

    fn estimated_total(&self) -> Option<u64> {
        let base = self.charset.len() as u64;
        let mut total = 0u64;
        for len in self.min_length..=self.max_length {
            total = total.checked_add(base.checked_pow(len as u32)?)?;
        }
        Some(total)
    }

    fn remaining_estimate(&self) -> Option<u64> {
        if self.exhausted {
            return Some(0);
        }
        let base = self.charset.len() as u64;
        let mut total = self.remaining_in_current_length();
        if total == u64::MAX {
            return None;
        }
        for len in (self.current_length + 1)..=self.max_length {
            total = total.checked_add(base.checked_pow(len as u32)?)?;
        }
        Some(total)
    }

    fn checkpoint(&self) -> Option<String> {
        let indices_str = if self.exhausted {
            String::new()
        } else {
            self.indices[..self.current_length]
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",")
        };
        Some(format!("{}:{}:{}", self.current_length, indices_str, self.exhausted as u8))
    }

    fn restore(&mut self, checkpoint: &str) -> Result<()> {
        let parts: Vec<&str> = checkpoint.splitn(3, ':').collect();
        if parts.len() != 3 {
            bail!("Invalid brute-force checkpoint");
        }
        let len: usize = parts[0].parse()?;
        let exhausted = match parts[2] {
            "0" => false,
            "1" => true,
            other => bail!("Invalid brute-force checkpoint exhausted flag: {other}"),
        };

        self.indices = [0; 20];
        if exhausted {
            if len < self.min_length || len > self.max_length + 1 {
                bail!(
                    "Brute-force checkpoint length {len} out of range {}..={}",
                    self.min_length,
                    self.max_length
                );
            }
            self.current_length = len;
            self.exhausted = true;
            return Ok(());
        }

        if len < self.min_length || len > self.max_length {
            bail!(
                "Brute-force checkpoint length {len} out of range {}..={}",
                self.min_length,
                self.max_length
            );
        }
        let idx_parts: Vec<&str> = if parts[1].is_empty() {
            Vec::new()
        } else {
            parts[1].split(',').collect()
        };
        if idx_parts.len() != len {
            bail!("Brute-force checkpoint has {} indices, expected {len}", idx_parts.len());
        }
        for (i, s) in idx_parts.iter().enumerate() {
            let idx: usize = s.parse()?;
            if idx >= self.charset.len() {
                bail!("Brute-force checkpoint index {idx} out of charset bounds");
            }
            self.indices[i] = idx;
        }
        self.current_length = len;
        self.exhausted = false;
        Ok(())
    }

    fn config_signature(&self) -> String {
        self.config_sig.clone()
    }

    fn name(&self) -> &str {
        "brute-force"
    }
}

pub(super) fn execute_shard(
    charset: &[u8],
    len: usize,
    mut indices: [usize; 20],
    mut count: u64,
    verifier: &dyn crate::formats::PasswordVerifier,
    stop: &std::sync::atomic::AtomicBool,
    global_attempts: &std::sync::atomic::AtomicU64,
) -> Option<Vec<u8>> {
    use super::add_and_maybe_flush;
    use std::sync::atomic::Ordering;

    if len == 0 || count == 0 || charset.is_empty() {
        return None;
    }
    let base = charset.len();
    let mut buf = [0u8; 20];
    for (dst, &idx) in buf[..len - 1].iter_mut().zip(&indices[..len - 1]) {
        *dst = charset[idx];
    }

    let mut pending_attempts = 0u64;
    while count > 0 && !stop.load(Ordering::Relaxed) {
        let start_last = indices[len - 1];
        let avail = (base - start_last) as u64;
        let group_len = count.min(avail) as usize;
        let last_slice = &charset[start_last..start_last + group_len];

        let res = verifier.verify_prefix_group(&buf[..len - 1], last_slice, stop);
        add_and_maybe_flush(&mut pending_attempts, res.attempts, global_attempts, false);
        if let Some(matched_idx) = res.matched {
            add_and_maybe_flush(&mut pending_attempts, 0, global_attempts, true);
            buf[len - 1] = last_slice[matched_idx];
            return Some(buf[..len].to_vec());
        }

        count -= group_len as u64;
        if count == 0 {
            break;
        }
        indices[len - 1] = 0;
        for i in (0..len - 1).rev() {
            if indices[i] + 1 < base {
                indices[i] += 1;
                buf[i] = charset[indices[i]];
                break;
            }
            indices[i] = 0;
            buf[i] = charset[0];
        }
    }
    add_and_maybe_flush(&mut pending_attempts, 0, global_attempts, true);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence() {
        let mut src = BruteForceSource::new("ab".into(), 1, 2).unwrap();
        let mut batch = CandidateBatch::new();
        src.fill_batch(&mut batch, 1024);
        assert_eq!(batch.get(0), Some(&b"a"[..]));
        assert_eq!(batch.get(1), Some(&b"b"[..]));
        assert_eq!(batch.get(2), Some(&b"aa"[..]));
        assert_eq!(batch.len(), 6);
    }

    #[test]
    fn total() {
        let src = BruteForceSource::new("ab".into(), 1, 3).unwrap();
        assert_eq!(src.estimated_total(), Some(2 + 4 + 8));
    }

    #[test]
    fn deduplicates_charset() {
        let src = BruteForceSource::new("aabb".into(), 1, 2).unwrap();
        assert_eq!(src.estimated_total(), Some(2 + 4));
    }

    #[test]
    fn restore_rejects_out_of_bounds_checkpoint() {
        let mut src = BruteForceSource::new("01".into(), 1, 3).unwrap();
        assert!(src.restore("2:0,5:0").is_err());
        assert!(src.restore("4:0,0,0,0:0").is_err());
        assert!(src.restore("2:0:0").is_err());
    }

    #[test]
    fn shard_checkpoint_roundtrip() {
        let mut src = BruteForceSource::new("ab".into(), 1, 2).unwrap();
        let shard = src.take_shard(3).unwrap();
        match shard {
            WorkShard::BruteForce { len, count, .. } => {
                assert_eq!(len, 1);
                assert_eq!(count, 2);
            }
            _ => unreachable!(),
        }
        assert_eq!(src.remaining_estimate(), Some(4));
        let ck = src.checkpoint().unwrap();
        let mut restored = BruteForceSource::new("ab".into(), 1, 2).unwrap();
        restored.restore(&ck).unwrap();
        let mut batch = CandidateBatch::new();
        restored.fill_batch(&mut batch, 10);
        assert_eq!(batch.get(0), Some(&b"aa"[..]));
        assert_eq!(batch.len(), 4);
    }
}
