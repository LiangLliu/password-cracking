use clap::{Parser, Subcommand};
use password_cracking::{CrackerEngine, GeneratorMode};
use password_cracking::utils::{validate_file, validate_wordlist, format_duration, format_number, charsets};
use std::path::PathBuf;
use anyhow::{Result, Context};

#[derive(Parser)]
#[command(
    name = "password-cracking",
    about = "High-performance document password cracker",
    version,
    author
)]
struct Cli {
    /// Target file to crack
    #[arg(short, long)]
    file: PathBuf,

    /// Number of threads to use (default: auto-detect)
    #[arg(short, long)]
    threads: Option<usize>,

    /// Performance mode: balanced, aggressive (default: aggressive)
    #[arg(short = 'p', long, default_value = "aggressive")]
    performance: String,

    /// Attack mode
    #[command(subcommand)]
    mode: AttackMode,
}

#[derive(Subcommand)]
enum AttackMode {
    /// Dictionary attack
    Dictionary {
        /// Path to dictionary file
        #[arg(short, long)]
        wordlist: PathBuf,
    },
    /// Brute force attack
    BruteForce {
        /// Character set to use (digits, lower, upper, special, alnum, all)
        #[arg(short, long, default_value = "alnum")]
        charset: String,

        /// Minimum password length
        #[arg(long, default_value = "1")]
        min_length: usize,

        /// Maximum password length
        #[arg(long, default_value = "6")]
        max_length: usize,
    },
    /// Hybrid attack (dictionary + variations)
    Hybrid {
        /// Path to dictionary file
        #[arg(short, long)]
        wordlist: PathBuf,

        /// Append numbers to passwords
        #[arg(long)]
        numbers: bool,

        /// Append special characters
        #[arg(long)]
        special: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // 验证目标文件
    validate_file(&cli.file)
        .with_context(|| format!("Invalid target file: {:?}", cli.file))?;

    // 创建生成器模式
    let generator_mode = match cli.mode {
        AttackMode::Dictionary { wordlist } => {
            validate_wordlist(&wordlist)?;
            GeneratorMode::Dictionary {
                path: wordlist.to_string_lossy().to_string(),
            }
        }
        AttackMode::BruteForce { charset, min_length, max_length } => {
            let charset_str = match charset.as_str() {
                "digits" => charsets::DIGITS.to_string(),
                "lower" => charsets::LOWERCASE.to_string(),
                "upper" => charsets::UPPERCASE.to_string(),
                "special" => charsets::SPECIAL.to_string(),
                "alnum" => charsets::alphanumeric(),
                "all" => charsets::all(),
                custom => custom.to_string(),
            };

            GeneratorMode::BruteForce {
                charset: charset_str,
                min_length,
                max_length,
            }
        }
        AttackMode::Hybrid { wordlist, numbers, special } => {
            validate_wordlist(&wordlist)?;
            GeneratorMode::Hybrid {
                dictionary_path: wordlist.to_string_lossy().to_string(),
                append_numbers: numbers,
                append_special: special,
            }
        }
    };

    // 显示破解信息
    println!("Document Password Cracker");
    println!("========================");
    println!("Target: {:?}", cli.file);

    // 创建破解引擎
    let engine = CrackerEngine::new(&cli.file, generator_mode, cli.threads, Some(&cli.performance))?;

    // 开始破解
    println!("Starting password cracking...");
    let result = engine.crack()?;

    // 显示结果
    println!("\nCracking completed!");
    println!("==================");
    println!("Duration: {}", format_duration(result.duration));
    println!("Attempts: {}", format_number(result.attempts));
    println!("Speed: {:.0} passwords/second", result.speed);

    if let Some(password) = result.password {
        println!("\n✅ Password found: {}", password);
    } else {
        println!("\n❌ Password not found in search space");
    }

    Ok(())
}
