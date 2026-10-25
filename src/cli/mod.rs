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
    /// Target file to crack (not needed for completions/man)
    #[arg(short, long)]
    pub file: Option<PathBuf>,

    /// Number of threads (default: all logical cores)
    #[arg(short, long, global = true)]
    pub threads: Option<usize>,

    /// Quiet mode: only show the result
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Save progress to this file periodically, allowing --resume
    #[arg(long, global = true, value_name = "PATH")]
    pub checkpoint: Option<PathBuf>,

    /// Resume from the file given by --checkpoint
    #[arg(long, global = true, requires = "checkpoint")]
    pub resume: bool,

    /// Seconds between checkpoint writes (default: 5)
    #[arg(long, global = true, default_value = "5", requires = "checkpoint")]
    pub checkpoint_interval: u64,

    /// Also try PDF owner passwords (slower: ~3x verification cost)
    #[arg(long, global = true)]
    pub pdf_owner: bool,

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
        /// Remove duplicate candidates (uses memory proportional to uniques)
        #[arg(long)]
        dedup: bool,
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
    /// Generate shell completions or a man page (Unix integration)
    Completions {
        /// Shell: bash, zsh, fish, elvish, powershell
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Generate the roff man page
    Man,
    /// Mask attack: per-position character sets (e.g. ?u?l?l?l?d?d)
    Mask {
        /// Mask pattern (?d digits, ?l lower, ?u upper, ?s special, ?a all, ?? literal ?)
        #[arg(short, long)]
        mask: String,
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
            AttackMode::Completions { .. } | AttackMode::Man => {
                anyhow::bail!("Completions/Man do not build a generator")
            }
            AttackMode::Dictionary { wordlist, dedup } => {
                utils::validate_wordlist(wordlist)?;
                Ok(GeneratorMode::Dictionary {
                    path: wordlist.clone(),
                    dedup: *dedup,
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
            AttackMode::Mask { mask } => Ok(GeneratorMode::Mask { mask: mask.clone() }),
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

    // Unix integration commands do not need a target file.
    match &cli.mode {
        AttackMode::Completions { shell } => {
            let mut cmd = <Cli as clap::CommandFactory>::command();
            clap_complete::generate(*shell, &mut cmd, "password-cracking", &mut std::io::stdout());
            return Ok(());
        }
        AttackMode::Man => {
            let cmd = <Cli as clap::CommandFactory>::command();
            clap_mangen::Man::new(cmd).render(&mut std::io::stdout())?;
            return Ok(());
        }
        _ => {}
    }

    let file = cli
        .file
        .as_ref()
        .context("--file is required for attack modes")?;
    utils::validate_file(file)
        .with_context(|| format!("Invalid target file: {}", file.display()))?;

    let mode = cli.build_generator_mode()?;
    let mut source = crate::generators::create_source(mode)?;

    // Resume from a checkpoint before starting the engine.
    if let Some(ckpt) = &cli.checkpoint {
        if cli.resume && ckpt.exists() {
            let content = std::fs::read_to_string(ckpt)
                .with_context(|| format!("Cannot read checkpoint {}", ckpt.display()))?;
            let mut lines = content.lines();
            let name = lines.next().unwrap_or_default();
            let state = lines.next().unwrap_or_default();
            if name != source.name() {
                anyhow::bail!(
                    "Checkpoint belongs to attack '{name}', but this run is '{}'",
                    source.name()
                );
            }
            source
                .restore(state)
                .with_context(|| format!("Invalid checkpoint state in {}", ckpt.display()))?;
            if !cli.quiet {
                println!("Resumed from {}", ckpt.display());
            }
        } else if cli.resume && !cli.quiet {
            eprintln!("No checkpoint at {}; starting from scratch", ckpt.display());
        }
    }

    if !cli.quiet {
        println!("Password Cracker");
        println!("================");
        println!("Target: {}", file.display());
    }

    let options = crate::formats::VerifyOptions {
        try_pdf_owner: cli.pdf_owner,
    };
    let engine =
        crate::engine::CrackerEngine::new_with_options(file, source, cli.threads, options)?;
    let engine = match &cli.checkpoint {
        Some(path) => engine.with_checkpoint(
            path.clone(),
            std::time::Duration::from_secs(cli.checkpoint_interval.max(1)),
        ),
        None => engine,
    };
    let result = engine.crack(cli.quiet)?;

    // The search is finished (found or exhausted); a checkpoint is stale.
    if let Some(path) = &cli.checkpoint {
        let _ = std::fs::remove_file(path);
    }

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
