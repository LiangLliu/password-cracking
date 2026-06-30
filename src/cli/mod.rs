use crate::generators::{rules::Rule, GeneratorMode};
use crate::utils::{self, charsets};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "password-cracking",
    version,
    author,
    about = "High-performance document password cracker"
)]
pub struct Cli {
    /// Target file to crack
    #[arg(short, long)]
    pub file: PathBuf,

    /// Number of threads (default: all logical cores)
    #[arg(short, long)]
    pub threads: Option<usize>,

    /// Attack mode
    #[command(subcommand)]
    pub mode: AttackMode,
}

#[derive(Subcommand)]
pub enum AttackMode {
    /// Dictionary attack using a wordlist file or directory
    Dictionary {
        /// Path to wordlist file or directory of .txt files
        #[arg(short, long)]
        wordlist: PathBuf,
    },
    /// Brute-force attack over a character set
    BruteForce {
        /// Character set: digits, lower, upper, special, alnum, all, or custom
        #[arg(short, long, default_value = "alnum")]
        charset: String,
        /// Minimum password length
        #[arg(long, default_value = "1")]
        min_length: usize,
        /// Maximum password length
        #[arg(long, default_value = "6")]
        max_length: usize,
    },
    /// Hybrid attack: dictionary + mutation rules
    Hybrid {
        /// Path to wordlist
        #[arg(short, long)]
        wordlist: PathBuf,
        /// Apply capitalize rule
        #[arg(long)]
        capitalize: bool,
        /// Apply uppercase rule
        #[arg(long)]
        upper: bool,
        /// Apply lowercase rule
        #[arg(long)]
        lower: bool,
        /// Apply l33t-speak rule
        #[arg(long)]
        l33t: bool,
        /// Append up to N digits
        #[arg(long)]
        append_digits: Option<u32>,
    },
}

impl Cli {
    pub fn build_generator_mode(&self) -> Result<GeneratorMode> {
        match &self.mode {
            AttackMode::Dictionary { wordlist } => {
                utils::validate_wordlist(wordlist)?;
                Ok(GeneratorMode::Dictionary {
                    path: wordlist.clone(),
                })
            }
            AttackMode::BruteForce {
                charset,
                min_length,
                max_length,
            } => {
                let cs = resolve_charset(charset);
                Ok(GeneratorMode::BruteForce {
                    charset: cs,
                    min_length: *min_length,
                    max_length: *max_length,
                })
            }
            AttackMode::Hybrid {
                wordlist,
                capitalize,
                upper,
                lower,
                l33t,
                append_digits,
            } => {
                utils::validate_wordlist(wordlist)?;
                let mut rules = Vec::new();
                if *capitalize {
                    rules.push(Rule::Capitalize);
                }
                if *upper {
                    rules.push(Rule::Upper);
                }
                if *lower {
                    rules.push(Rule::Lower);
                }
                if *l33t {
                    rules.push(Rule::L33t);
                }
                if let Some(n) = append_digits {
                    rules.push(Rule::AppendDigits(*n));
                }
                if rules.is_empty() {
                    rules.push(Rule::Capitalize);
                }
                Ok(GeneratorMode::Hybrid {
                    dictionary_path: wordlist.clone(),
                    rules,
                })
            }
        }
    }
}

fn resolve_charset(name: &str) -> String {
    match name {
        "digits" => charsets::DIGITS.to_string(),
        "lower" => charsets::LOWERCASE.to_string(),
        "upper" => charsets::UPPERCASE.to_string(),
        "special" => charsets::SPECIAL.to_string(),
        "alnum" => charsets::alphanumeric(),
        "all" => charsets::all(),
        custom => custom.to_string(),
    }
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    utils::validate_file(&cli.file)
        .with_context(|| format!("Invalid target file: {}", cli.file.display()))?;

    let mode = cli.build_generator_mode()?;
    let source = crate::generators::create_source(mode)?;

    println!("Password Cracker");
    println!("================");
    println!("Target: {}", cli.file.display());

    let engine = crate::engine::CrackerEngine::new(&cli.file, source, cli.threads)?;
    let result = engine.crack()?;

    println!("\nResult");
    println!("======");
    println!("Duration: {}", utils::format_duration(result.duration));
    println!("Attempts: {}", utils::format_number(result.attempts));
    println!("Speed:    {:.0} passwords/sec", result.speed);

    match result.password {
        Some(pw) => println!("\nPassword found: {pw}"),
        None => println!("\nPassword not found"),
    }

    Ok(())
}
