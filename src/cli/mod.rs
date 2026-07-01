use crate::generators::{GeneratorMode, rules::Rule};
use crate::utils::{self, charsets};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "password-cracking",
    version,
    about = "High-performance document password cracker",
    long_about = "Cracks password-protected ZIP, PDF, and Office documents.\n\
                  Supports dictionary, brute-force, and hybrid (rule-based) attacks.\n\
                  Automatically detects file format and encryption type."
)]
pub struct Cli {
    /// Target file to crack
    #[arg(short, long)]
    pub file: PathBuf,

    /// Number of threads (default: all logical cores)
    #[arg(short, long)]
    pub threads: Option<usize>,

    /// Quiet mode: only show the result
    #[arg(short, long)]
    pub quiet: bool,

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
        /// Character set: digits, lower, upper, special, alnum, all, or custom string
        #[arg(short, long, default_value = "alnum")]
        charset: String,
        /// Minimum password length
        #[arg(long, default_value = "1")]
        min_length: usize,
        /// Maximum password length
        #[arg(long, default_value = "6")]
        max_length: usize,
    },
    /// Hybrid attack: dictionary words + mutation rules
    Hybrid {
        /// Path to wordlist
        #[arg(short, long)]
        wordlist: PathBuf,
        /// Capitalize first letter
        #[arg(long)]
        capitalize: bool,
        /// Convert to uppercase
        #[arg(long)]
        upper: bool,
        /// Convert to lowercase
        #[arg(long)]
        lower: bool,
        /// Apply l33t-speak substitutions (a→@, e→3, i→1, o→0, s→$)
        #[arg(long)]
        l33t: bool,
        /// Reverse the word
        #[arg(long)]
        reverse: bool,
        /// Duplicate the word (e.g. pass→passpass)
        #[arg(long)]
        duplicate: bool,
        /// Append 0..=N to each word
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
                reverse,
                duplicate,
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
                if *reverse {
                    rules.push(Rule::Reverse);
                }
                if *duplicate {
                    rules.push(Rule::Duplicate);
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

    if !cli.quiet {
        println!("Password Cracker");
        println!("================");
        println!("Target: {}", cli.file.display());
    }

    let engine = crate::engine::CrackerEngine::new(&cli.file, source, cli.threads)?;
    let result = engine.crack(cli.quiet)?;

    if !cli.quiet {
        println!("\nResult");
        println!("======");
        println!("Duration: {}", utils::format_duration(result.duration));
        println!("Attempts: {}", utils::format_number(result.attempts));
        println!("Speed:    {:.0} passwords/sec", result.speed);
    }

    match result.password {
        Some(pw) => {
            if cli.quiet {
                println!("{pw}");
            } else {
                println!("\nPassword found: {pw}");
            }
        }
        None => {
            if !cli.quiet {
                println!("\nPassword not found");
            } else {
                eprintln!("not found");
            }
        }
    }

    Ok(())
}
