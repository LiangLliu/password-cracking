use super::{CandidateBatch, PasswordSource, WorkShard};
use crate::utils::charsets;
use anyhow::{Result, bail};
use std::sync::Arc;

/// Hashcat-style mask attack source.
///
/// Each position has its own character set, so a mask like `?u?l?l?l?d?d`
/// tries exactly the intended shape instead of the full cartesian product of
/// one global charset.
///
/// Supported placeholders:
/// - `?d` digits, `?l` lowercase, `?u` uppercase, `?s` special, `?a` all
/// - `??` literal `?`
/// - any other character is a literal (multi-byte UTF-8 is preserved)
pub struct MaskSource {
    mask_pattern: String,
    positions: Arc<[Vec<u8>]>,
    indices: [u16; 128],
    pos_count: usize,
    exhausted: bool,
}

impl MaskSource {
    pub fn new(mask: &str) -> Result<Self> {
        if mask.is_empty() {
            bail!("Mask cannot be empty");
        }

        let mut positions: Vec<Vec<u8>> = Vec::new();
        let mut chars = mask.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '?' {
                match chars.next() {
                    Some('d') => positions.push(charsets::DIGITS.as_bytes().to_vec()),
                    Some('l') => positions.push(charsets::LOWERCASE.as_bytes().to_vec()),
                    Some('u') => positions.push(charsets::UPPERCASE.as_bytes().to_vec()),
                    Some('s') => positions.push(charsets::SPECIAL.as_bytes().to_vec()),
                    Some('a') => positions.push(charsets::all().into_bytes()),
                    Some('?') => positions.push(vec![b'?']),
                    Some(other) => bail!("Unknown mask placeholder: ?{other}"),
                    None => bail!("Mask ends with a dangling '?' (use ?? for a literal ?)"),
                }
            } else {
                // Keep multi-byte UTF-8 intact as a fixed byte sequence.
                let mut buf = [0u8; 4];
                for &b in c.encode_utf8(&mut buf).as_bytes() {
                    positions.push(vec![b]);
                }
            }
        }

        if positions.is_empty() {
            bail!("Mask produced no positions");
        }
        // A 128-position mask is already astronomically large; refuse absurd
        // masks that would make the index vector unwieldy.
        if positions.len() > 128 {
            bail!("Mask too long (max 128 positions)");
        }

        let pos_count = positions.len();
        Ok(Self {
            mask_pattern: mask.to_string(),
            positions: positions.into(),
            indices: [0u16; 128],
            pos_count,
            exhausted: false,
        })
    }

    fn increment(&mut self) -> bool {
        for i in (0..self.pos_count).rev() {
            if usize::from(self.indices[i]) + 1 < self.positions[i].len() {
                self.indices[i] += 1;
                return true;
            }
            self.indices[i] = 0;
        }
        false
    }

    fn remaining_count(&self) -> u64 {
        if self.exhausted {
            return 0;
        }
        let mut rem = 0u64;
        let mut weight = 1u64;
        for i in (0..self.pos_count).rev() {
            let base = self.positions[i].len() as u64;
            let left = (self.positions[i].len() - 1 - usize::from(self.indices[i])) as u64;
            rem = rem.saturating_add(left.saturating_mul(weight));
            weight = weight.saturating_mul(base);
        }
        rem.saturating_add(1)
    }

    fn advance_by(&mut self, steps: u64) {
        let mut carry = steps;
        for i in (0..self.pos_count).rev() {
            let base = self.positions[i].len() as u64;
            let sum = u64::from(self.indices[i]) + carry;
            self.indices[i] = (sum % base) as u16;
            carry = sum / base;
            if carry == 0 {
                break;
            }
        }
    }
}

impl PasswordSource for MaskSource {
    fn fill_batch(&mut self, batch: &mut CandidateBatch, max: usize) -> bool {
        batch.clear();
        if self.exhausted || max == 0 {
            return false;
        }
        let mut buf = [0u8; 128];
        while batch.len() < max && !self.exhausted {
            for ((dst, charset), &idx) in buf[..self.pos_count]
                .iter_mut()
                .zip(self.positions.iter())
                .zip(&self.indices[..self.pos_count])
            {
                *dst = charset[usize::from(idx)];
            }
            if !batch.push(&buf[..self.pos_count]) {
                break;
            }
            if !self.increment() {
                self.exhausted = true;
            }
        }
        !batch.is_empty()
    }

    fn take_shard(&mut self, max: usize) -> Option<WorkShard> {
        if self.exhausted || max == 0 {
            return None;
        }
        let start_indices = self.indices;
        let rem = self.remaining_count();
        let taken = rem.min(max as u64);
        if taken == rem {
            self.exhausted = true;
        } else {
            self.advance_by(taken);
        }

        Some(WorkShard::Mask {
            positions: Arc::clone(&self.positions),
            indices: start_indices,
            pos_count: self.pos_count,
            count: taken,
        })
    }

    fn estimated_total(&self) -> Option<u64> {
        let mut total = 1u64;
        for charset in self.positions.iter() {
            total = total.checked_mul(charset.len() as u64)?;
        }
        Some(total)
    }

    fn remaining_estimate(&self) -> Option<u64> {
        if self.exhausted {
            return Some(0);
        }
        // Only compute exact remaining when total fits or saturating count didn't saturate.
        let rem = self.remaining_count();
        if rem == u64::MAX { None } else { Some(rem) }
    }

    fn checkpoint(&self) -> Option<String> {
        let coords = self.indices[..self.pos_count]
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        Some(if self.exhausted {
            format!("{coords}|1")
        } else {
            coords
        })
    }

    fn restore(&mut self, checkpoint: &str) -> Result<()> {
        let (coords, exhausted) = match checkpoint.split_once('|') {
            Some((c, flag)) => (c, flag == "1"),
            None => (checkpoint, false),
        };
        if exhausted {
            self.exhausted = true;
            return Ok(());
        }
        let parts: Vec<&str> = coords.split(',').collect();
        if parts.len() != self.pos_count {
            bail!("Mask checkpoint has {} positions, expected {}", parts.len(), self.pos_count);
        }
        for (i, part) in parts.iter().enumerate() {
            let idx: u16 = part.parse()?;
            if usize::from(idx) >= self.positions[i].len() {
                bail!("Mask checkpoint index out of range at position {i}");
            }
            self.indices[i] = idx;
        }
        self.exhausted = false;
        Ok(())
    }

    fn config_signature(&self) -> String {
        format!("mask:{}", self.mask_pattern)
    }

    fn name(&self) -> &str {
        "mask"
    }
}

pub(super) fn execute_shard(
    positions: &[Vec<u8>],
    mut indices: [u16; 128],
    pos_count: usize,
    mut count: u64,
    verifier: &dyn crate::formats::PasswordVerifier,
    stop: &std::sync::atomic::AtomicBool,
    global_attempts: &std::sync::atomic::AtomicU64,
) -> Option<Vec<u8>> {
    use super::add_and_maybe_flush;
    use std::sync::atomic::Ordering;

    if pos_count == 0 || count == 0 {
        return None;
    }
    let mut buf = [0u8; 128];
    for ((dst, charset), &idx) in buf[..pos_count - 1]
        .iter_mut()
        .zip(&positions[..pos_count - 1])
        .zip(&indices[..pos_count - 1])
    {
        *dst = charset[usize::from(idx)];
    }
    let last_charset = &positions[pos_count - 1];

    let mut pending_attempts = 0u64;
    while count > 0 && !stop.load(Ordering::Relaxed) {
        let start_last = usize::from(indices[pos_count - 1]);
        let avail = (last_charset.len() - start_last) as u64;
        let group_len = count.min(avail) as usize;
        let last_slice = &last_charset[start_last..start_last + group_len];

        let res = verifier.verify_prefix_group(&buf[..pos_count - 1], last_slice, stop);
        add_and_maybe_flush(&mut pending_attempts, res.attempts, global_attempts, false);
        if let Some(matched_idx) = res.matched {
            add_and_maybe_flush(&mut pending_attempts, 0, global_attempts, true);
            buf[pos_count - 1] = last_slice[matched_idx];
            return Some(buf[..pos_count].to_vec());
        }

        count -= group_len as u64;
        if count == 0 {
            break;
        }
        indices[pos_count - 1] = 0;
        for i in (0..pos_count - 1).rev() {
            let next = usize::from(indices[i]) + 1;
            if next < positions[i].len() {
                indices[i] = next as u16;
                buf[i] = positions[i][next];
                break;
            }
            indices[i] = 0;
            buf[i] = positions[i][0];
        }
    }
    add_and_maybe_flush(&mut pending_attempts, 0, global_attempts, true);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builtin_sets_and_literals() {
        let src = MaskSource::new("?d?l?u?s?a??x").unwrap();
        assert_eq!(src.positions.len(), 7);
        assert_eq!(src.positions[0].len(), 10);
        assert_eq!(src.positions[1].len(), 26);
        assert_eq!(src.positions[2].len(), 26);
        assert_eq!(src.positions[3].len(), charsets::SPECIAL.len());
        assert_eq!(src.positions[4].len(), charsets::all().len());
        assert_eq!(src.positions[5], vec![b'?']);
        assert_eq!(src.positions[6], vec![b'x']);
    }

    #[test]
    fn total_is_product() {
        let src = MaskSource::new("?d?d?l?l").unwrap();
        assert_eq!(src.estimated_total(), Some(10 * 10 * 26 * 26));
    }

    #[test]
    fn iteration_order_and_checkpoint() {
        let mut src = MaskSource::new("?d?l").unwrap();
        let mut batch = CandidateBatch::new();
        src.fill_batch(&mut batch, 3);
        assert_eq!(batch.get(0), Some(&b"0a"[..]));
        assert_eq!(batch.get(1), Some(&b"0b"[..]));
        assert_eq!(batch.get(2), Some(&b"0c"[..]));

        let ck = src.checkpoint().unwrap();
        assert_eq!(ck, "0,3");

        let mut restored = MaskSource::new("?d?l").unwrap();
        restored.restore(&ck).unwrap();
        let mut batch = CandidateBatch::new();
        restored.fill_batch(&mut batch, 1);
        assert_eq!(batch.get(0), Some(&b"0d"[..]));
    }

    #[test]
    fn rejects_invalid_masks() {
        assert!(MaskSource::new("").is_err());
        assert!(MaskSource::new("?x").is_err());
        assert!(MaskSource::new("abc?").is_err());
    }

    #[test]
    fn preserves_multibyte_literals() {
        let mut src = MaskSource::new("密?d").unwrap();
        // '密' is 3 UTF-8 bytes -> 3 fixed positions + 1 digit position.
        assert_eq!(src.positions.len(), 4);
        let mut batch = CandidateBatch::new();
        src.fill_batch(&mut batch, 1);
        let expected = "密0".as_bytes();
        assert_eq!(batch.get(0), Some(expected));
    }

    #[test]
    fn shard_advances_accurately() {
        let mut src = MaskSource::new("?d?d").unwrap();
        let shard = src.take_shard(37).unwrap();
        match shard {
            WorkShard::Mask { count, .. } => assert_eq!(count, 37),
            _ => unreachable!(),
        }
        assert_eq!(src.remaining_estimate(), Some(63));
        let mut batch = CandidateBatch::new();
        src.fill_batch(&mut batch, 1);
        assert_eq!(batch.get(0), Some(&b"37"[..]));
    }
}
