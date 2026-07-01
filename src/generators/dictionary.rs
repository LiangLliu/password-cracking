use super::PasswordSource;
use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::fs::{File, read_dir};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub struct DictionarySource {
    passwords: Vec<Box<[u8]>>,
    cursor: usize,
}

impl DictionarySource {
    pub fn new(path: &Path) -> Result<Self> {
        let mut passwords = Vec::new();
        if path.is_file() {
            load_file(path, &mut passwords)?;
        } else if path.is_dir() {
            load_dir(path, &mut passwords)?;
        } else {
            bail!("Not a file or directory: {}", path.display());
        }
        if passwords.is_empty() {
            bail!("No passwords found in {}", path.display());
        }
        // Dedup preserving order
        let mut seen = HashSet::new();
        passwords.retain(|p| seen.insert(p.clone()));
        println!("Loaded {} unique passwords", passwords.len());
        Ok(Self {
            passwords,
            cursor: 0,
        })
    }
}

fn load_file(path: &Path, out: &mut Vec<Box<[u8]>>) -> Result<()> {
    let file = File::open(path).with_context(|| format!("Cannot read {}", path.display()))?;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let line = line.trim();
        if !line.is_empty() && !line.starts_with('#') {
            out.push(line.as_bytes().to_vec().into_boxed_slice());
        }
    }
    Ok(())
}

fn load_dir(path: &Path, out: &mut Vec<Box<[u8]>>) -> Result<()> {
    let mut stack: Vec<PathBuf> = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in read_dir(&dir)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "txt") {
                load_file(&p, out)?;
            }
        }
    }
    Ok(())
}

impl PasswordSource for DictionarySource {
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>) -> bool {
        const BATCH: usize = 1024;
        batch.clear();
        let end = (self.cursor + BATCH).min(self.passwords.len());
        if self.cursor >= self.passwords.len() {
            return false;
        }
        batch.extend(self.passwords[self.cursor..end].iter().cloned());
        self.cursor = end;
        !batch.is_empty()
    }

    fn estimated_total(&self) -> Option<u64> {
        Some(self.passwords.len() as u64)
    }

    fn checkpoint(&self) -> Option<String> {
        Some(self.cursor.to_string())
    }

    fn restore(&mut self, checkpoint: &str) -> Result<()> {
        self.cursor = checkpoint.parse()?;
        Ok(())
    }

    fn name(&self) -> &str {
        "dictionary"
    }
}
