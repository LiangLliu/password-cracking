use super::PasswordSource;
use anyhow::{Result, bail};

pub struct BruteForceSource {
    charset: Vec<u8>,
    indices: Vec<usize>,
    min_length: usize,
    max_length: usize,
    current_length: usize,
    exhausted: bool,
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
        Ok(Self {
            charset: charset.into_bytes(),
            indices: vec![0; min_length],
            min_length,
            max_length,
            current_length: min_length,
            exhausted: false,
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

    fn current(&self) -> Box<[u8]> {
        let mut buf = Vec::with_capacity(self.current_length);
        for &idx in &self.indices {
            buf.push(self.charset[idx]);
        }
        buf.into_boxed_slice()
    }
}

impl PasswordSource for BruteForceSource {
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>) -> bool {
        if self.exhausted {
            return false;
        }
        const BATCH: usize = 1024;
        batch.clear();
        for _ in 0..BATCH {
            if self.exhausted {
                break;
            }
            batch.push(self.current());
            if !self.increment() {
                self.current_length += 1;
                if self.current_length > self.max_length {
                    self.exhausted = true;
                } else {
                    self.indices = vec![0; self.current_length];
                }
            }
        }
        !batch.is_empty()
    }

    fn estimated_total(&self) -> Option<u64> {
        let base = self.charset.len() as u64;
        let mut total = 0u64;
        for len in self.min_length..=self.max_length {
            total = total.checked_add(base.checked_pow(len as u32)?)?;
        }
        Some(total)
    }

    fn checkpoint(&self) -> Option<String> {
        Some(format!(
            "{}:{}:{}",
            self.current_length,
            self.indices
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(","),
            self.exhausted as u8
        ))
    }

    fn restore(&mut self, checkpoint: &str) -> Result<()> {
        let parts: Vec<&str> = checkpoint.splitn(3, ':').collect();
        if parts.len() != 3 {
            bail!("Invalid checkpoint");
        }
        self.current_length = parts[0].parse()?;
        self.indices = if parts[1].is_empty() {
            vec![]
        } else {
            parts[1]
                .split(',')
                .map(|s| s.parse::<usize>())
                .collect::<Result<Vec<_>, _>>()?
        };
        self.exhausted = parts[2] == "1";
        Ok(())
    }

    fn name(&self) -> &str {
        "brute-force"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence() {
        let mut src = BruteForceSource::new("ab".into(), 1, 2).unwrap();
        let mut batch = Vec::new();
        src.fill_batch(&mut batch);
        assert_eq!(&*batch[0], b"a");
        assert_eq!(&*batch[1], b"b");
    }

    #[test]
    fn total() {
        let src = BruteForceSource::new("ab".into(), 1, 3).unwrap();
        assert_eq!(src.estimated_total(), Some(2 + 4 + 8));
    }
}
