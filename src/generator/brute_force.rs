use super::PasswordGenerator;
use anyhow::{bail, Result};

pub struct BruteForceGenerator {
    charset: Vec<char>,
    current_length: usize,
    min_length: usize,
    max_length: usize,
    current_indices: Vec<usize>,
    exhausted: bool,
}

impl BruteForceGenerator {
    pub fn new(charset: String, min_length: usize, max_length: usize) -> Result<Self> {
        if charset.is_empty() {
            bail!("Charset cannot be empty");
        }
        if min_length > max_length {
            bail!("Min length cannot be greater than max length");
        }
        if max_length > 20 {
            bail!("Max length too large (max: 20)");
        }

        let charset: Vec<char> = charset.chars().collect();
        let current_indices = vec![0; min_length];

        Ok(Self {
            charset,
            current_length: min_length,
            min_length,
            max_length,
            current_indices,
            exhausted: false,
        })
    }

    fn increment_indices(&mut self) -> bool {
        let charset_len = self.charset.len();

        for i in (0..self.current_length).rev() {
            if self.current_indices[i] < charset_len - 1 {
                self.current_indices[i] += 1;
                return true;
            } else {
                self.current_indices[i] = 0;
            }
        }

        // All combinations for current length exhausted
        false
    }

    fn indices_to_string(&self) -> String {
        self.current_indices
            .iter()
            .map(|&idx| self.charset[idx])
            .collect()
    }
}

impl PasswordGenerator for BruteForceGenerator {
    fn next(&mut self) -> Option<String> {
        if self.exhausted {
            return None;
        }

        if self.current_length == 0 {
            self.exhausted = true;
            return None;
        }

        // Generate current password
        let password = self.indices_to_string();

        // Try to increment indices
        if !self.increment_indices() {
            // Move to next length
            self.current_length += 1;
            if self.current_length > self.max_length {
                self.exhausted = true;
            } else {
                self.current_indices = vec![0; self.current_length];
            }
        }

        Some(password)
    }

    fn reset(&mut self) {
        self.current_length = self.min_length;
        self.current_indices = vec![0; self.min_length];
        self.exhausted = false;
    }

    fn total_combinations(&self) -> Option<u64> {
        let charset_len = self.charset.len() as u64;
        let mut total = 0u64;

        for length in self.min_length..=self.max_length {
            match charset_len.checked_pow(length as u32) {
                Some(combinations) => total = total.saturating_add(combinations),
                None => return None, // Overflow
            }
        }

        Some(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brute_force_simple() {
        let mut gen = BruteForceGenerator::new("abc".to_string(), 1, 2).unwrap();

        let passwords: Vec<String> = std::iter::from_fn(|| gen.next()).take(20).collect();

        assert_eq!(passwords[0], "a");
        assert_eq!(passwords[1], "b");
        assert_eq!(passwords[2], "c");
        assert_eq!(passwords[3], "aa");
        assert_eq!(passwords[4], "ab");
    }

    #[test]
    fn test_total_combinations() {
        let gen = BruteForceGenerator::new("ab".to_string(), 1, 3).unwrap();
        assert_eq!(gen.total_combinations(), Some(2 + 4 + 8)); // 2^1 + 2^2 + 2^3
    }
}
