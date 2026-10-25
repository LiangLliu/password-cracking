use super::PasswordSource;
use crate::utils::charsets;
use anyhow::{Result, bail};

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
    positions: Vec<Vec<u8>>,
    indices: Vec<usize>,
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

        let indices = vec![0usize; positions.len()];
        Ok(Self {
            positions,
            indices,
            exhausted: false,
        })
    }

    fn current(&self) -> Box<[u8]> {
        self.indices
            .iter()
            .zip(&self.positions)
            .map(|(&idx, charset)| charset[idx])
            .collect::<Vec<u8>>()
            .into_boxed_slice()
    }

    fn increment(&mut self) -> bool {
        for i in (0..self.indices.len()).rev() {
            if self.indices[i] + 1 < self.positions[i].len() {
                self.indices[i] += 1;
                return true;
            }
            self.indices[i] = 0;
        }
        false
    }
}

impl PasswordSource for MaskSource {
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>, max: usize) -> bool {
        batch.clear();
        while batch.len() < max && !self.exhausted {
            batch.push(self.current());
            if !self.increment() {
                self.exhausted = true;
            }
        }
        !batch.is_empty()
    }

    fn estimated_total(&self) -> Option<u64> {
        let mut total = 1u64;
        for charset in &self.positions {
            total = total.checked_mul(charset.len() as u64)?;
        }
        Some(total)
    }

    fn checkpoint(&self) -> Option<String> {
        Some(
            self.indices
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(","),
        )
    }

    fn restore(&mut self, checkpoint: &str) -> Result<()> {
        let parts: Vec<&str> = checkpoint.split(',').collect();
        if parts.len() != self.indices.len() {
            bail!("Mask checkpoint has {} positions, expected {}", parts.len(), self.indices.len());
        }
        for (i, part) in parts.iter().enumerate() {
            let idx: usize = part.parse()?;
            if idx >= self.positions[i].len() {
                bail!("Mask checkpoint index out of range at position {i}");
            }
            self.indices[i] = idx;
        }
        self.exhausted = false;
        Ok(())
    }

    fn name(&self) -> &str {
        "mask"
    }
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
        let mut batch = Vec::new();
        src.fill_batch(&mut batch, 3);
        assert_eq!(&*batch[0], b"0a");
        assert_eq!(&*batch[1], b"0b");
        assert_eq!(&*batch[2], b"0c");

        let ck = src.checkpoint().unwrap();
        assert_eq!(ck, "0,3");

        let mut restored = MaskSource::new("?d?l").unwrap();
        restored.restore(&ck).unwrap();
        let mut batch = Vec::new();
        restored.fill_batch(&mut batch, 1);
        assert_eq!(&*batch[0], b"0d");
    }

    #[test]
    fn rejects_invalid_masks() {
        assert!(MaskSource::new("").is_err());
        assert!(MaskSource::new("?x").is_err());
        assert!(MaskSource::new("abc?").is_err());
    }

    #[test]
    fn preserves_multibyte_literals() {
        let src = MaskSource::new("密?d").unwrap();
        // '密' is 3 UTF-8 bytes -> 3 fixed positions + 1 digit position.
        assert_eq!(src.positions.len(), 4);
        let mut batch = Vec::new();
        let mut src = src;
        src.fill_batch(&mut batch, 1);
        let expected = "密0".as_bytes();
        assert_eq!(&*batch[0], expected);
    }
}
