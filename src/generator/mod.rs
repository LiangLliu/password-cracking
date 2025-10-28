use anyhow::Result;

pub mod brute_force;
pub mod dictionary;

use brute_force::BruteForceGenerator;
use dictionary::DictionaryGenerator;

#[derive(Debug, Clone)]
pub enum GeneratorMode {
    Dictionary {
        path: String,
    },
    BruteForce {
        charset: String,
        min_length: usize,
        max_length: usize,
    },
    Hybrid {
        dictionary_path: String,
        append_numbers: bool,
        append_special: bool,
    },
}

pub trait PasswordGenerator: Send + Sync {
    fn next(&mut self) -> Option<String>;
    fn reset(&mut self);
    fn total_combinations(&self) -> Option<u64>;
}

pub fn create_generator(mode: GeneratorMode) -> Result<Box<dyn PasswordGenerator>> {
    match mode {
        GeneratorMode::Dictionary { path } => {
            Ok(Box::new(DictionaryGenerator::new(&path)?))
        }
        GeneratorMode::BruteForce { charset, min_length, max_length } => {
            Ok(Box::new(BruteForceGenerator::new(charset, min_length, max_length)?))
        }
        GeneratorMode::Hybrid { .. } => {
            todo!("Hybrid mode not implemented yet")
        }
    }
}