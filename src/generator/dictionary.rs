use super::PasswordGenerator;
use std::fs::{File, read_dir};
use std::io::{BufRead, BufReader};
use std::path::Path;
use anyhow::{Result, Context};

pub struct DictionaryGenerator {
    passwords: Vec<String>,
    current_index: usize,
}

impl DictionaryGenerator {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let mut passwords = Vec::new();

        if path.is_file() {
            // Single file mode
            Self::load_file(path, &mut passwords)?;
        } else if path.is_dir() {
            // Directory mode - recursively load all .txt files
            Self::load_directory(path, &mut passwords)?;
        } else {
            anyhow::bail!("Path is neither a file nor a directory: {:?}", path);
        }

        if passwords.is_empty() {
            anyhow::bail!("No passwords found in the specified path");
        }

        // Remove duplicates while preserving order
        let mut seen = std::collections::HashSet::new();
        passwords.retain(|pwd| seen.insert(pwd.clone()));

        println!("Loaded {} unique passwords from {}", passwords.len(), path.display());

        Ok(Self {
            passwords,
            current_index: 0,
        })
    }

    fn load_file(path: &Path, passwords: &mut Vec<String>) -> Result<()> {
        let file = File::open(path)
            .with_context(|| format!("Failed to open dictionary file: {:?}", path))?;

        let reader = BufReader::new(file);
        for line in reader.lines() {
            if let Ok(password) = line {
                let password = password.trim();
                if !password.is_empty() && !password.starts_with('#') {
                    passwords.push(password.to_string());
                }
            }
        }

        Ok(())
    }

    fn load_directory(path: &Path, passwords: &mut Vec<String>) -> Result<()> {
        for entry in read_dir(path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Recursively load subdirectories
                Self::load_directory(&path, passwords)?;
            } else if path.is_file() {
                // Check if it's a .txt file
                if let Some(ext) = path.extension() {
                    if ext == "txt" {
                        println!("Loading dictionary: {}", path.display());
                        Self::load_file(&path, passwords)?;
                    }
                }
            }
        }

        Ok(())
    }

    pub fn with_mutations(mut self, mutations: &[PasswordMutation]) -> Self {
        let mut mutated_passwords = Vec::new();

        for password in &self.passwords {
            mutated_passwords.push(password.clone());

            for mutation in mutations {
                if let Some(mutated) = mutation.apply(password) {
                    mutated_passwords.push(mutated);
                }
            }
        }

        self.passwords = mutated_passwords;
        self
    }
}

impl PasswordGenerator for DictionaryGenerator {
    fn next(&mut self) -> Option<String> {
        if self.current_index < self.passwords.len() {
            let password = self.passwords[self.current_index].clone();
            self.current_index += 1;
            Some(password)
        } else {
            None
        }
    }

    fn reset(&mut self) {
        self.current_index = 0;
    }

    fn total_combinations(&self) -> Option<u64> {
        Some(self.passwords.len() as u64)
    }
}

#[derive(Debug, Clone)]
pub enum PasswordMutation {
    AppendNumbers { max_digits: usize },
    PrependNumbers { max_digits: usize },
    Capitalize,
    Uppercase,
    Lowercase,
    L33tSpeak,
    AppendSpecialChars,
}

impl PasswordMutation {
    pub fn apply(&self, password: &str) -> Option<String> {
        match self {
            PasswordMutation::Capitalize => {
                let mut chars = password.chars();
                chars.next().map(|first| {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                })
            }
            PasswordMutation::Uppercase => Some(password.to_uppercase()),
            PasswordMutation::Lowercase => Some(password.to_lowercase()),
            PasswordMutation::L33tSpeak => {
                Some(password
                    .replace('a', "@")
                    .replace('e', "3")
                    .replace('i', "1")
                    .replace('o', "0")
                    .replace('s', "$"))
            }
            _ => None, // Other mutations would need more complex implementation
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_dictionary_generator() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "password1")?;
        writeln!(temp_file, "password2")?;
        writeln!(temp_file, "password3")?;
        temp_file.flush()?;

        let mut gen = DictionaryGenerator::new(temp_file.path())?;

        assert_eq!(gen.next(), Some("password1".to_string()));
        assert_eq!(gen.next(), Some("password2".to_string()));
        assert_eq!(gen.next(), Some("password3".to_string()));
        assert_eq!(gen.next(), None);

        gen.reset();
        assert_eq!(gen.next(), Some("password1".to_string()));

        Ok(())
    }

    #[test]
    fn test_mutations() {
        let mutation = PasswordMutation::Capitalize;
        assert_eq!(mutation.apply("hello"), Some("Hello".to_string()));

        let mutation = PasswordMutation::L33tSpeak;
        assert_eq!(mutation.apply("password"), Some("p@$$w0rd".to_string()));
    }
}