use crate::formats::PasswordVerifier;
use crate::generators::PasswordSource;
use anyhow::Result;
use crossbeam_channel::{bounded, Receiver, Sender};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

/// Outcome of a cracking session.
#[derive(Debug)]
pub struct CrackResult {
    pub password: Option<String>,
    pub attempts: u64,
    pub duration: Duration,
    pub speed: f64,
}

/// The core cracking engine.
///
/// Drives a [`PasswordSource`] through a [`PasswordVerifier`] using a pool of
/// Rayon worker threads.
pub struct CrackerEngine {
    verifier: Arc<dyn PasswordVerifier>,
    source: Box<dyn PasswordSource>,
    thread_count: usize,
}

impl CrackerEngine {
    pub fn new(
        file_path: &Path,
        source: Box<dyn PasswordSource>,
        thread_count: Option<usize>,
    ) -> Result<Self> {
        let verifier = crate::formats::create_verifier(file_path)?;
        let thread_count = thread_count.unwrap_or_else(num_cpus::get);

        Ok(Self {
            verifier: Arc::from(verifier),
            source,
            thread_count,
        })
    }

    pub fn crack(mut self, quiet: bool) -> Result<CrackResult> {
        let start = Instant::now();
        let total = self.source.estimated_total();
        let format = self.verifier.format_name();
        let encryption = self.verifier.encryption_info();
        let source_name = self.source.name();

        if !quiet {
            println!("\nFormat:  {format} ({encryption})");
            println!("Attack:  {source_name}");
            println!("Threads: {}", self.thread_count);
            if let Some(t) = total {
                println!("Keyspace: {}", crate::utils::format_number(t));
            }
        }

        let pb = if quiet {
            ProgressBar::hidden()
        } else {
            build_progress_bar(total)
        };
        pb.enable_steady_tick(Duration::from_millis(100));

        let found = Arc::new(AtomicBool::new(false));
        let found_password: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let attempts = Arc::new(AtomicU64::new(0));

        type PasswordBatch = Vec<Box<[u8]>>;

        // Bounded channel keeps memory flat.
        let (tx, rx): (Sender<PasswordBatch>, Receiver<PasswordBatch>) =
            bounded(self.thread_count * 2);

        // Generator thread
        let src_found = Arc::clone(&found);
        let src_handle = std::thread::spawn(move || {
            let mut batch: Vec<Box<[u8]>> = Vec::with_capacity(1024);
            while !src_found.load(Ordering::Relaxed) {
                batch.clear();
                if !self.source.fill_batch(&mut batch) {
                    if !batch.is_empty() {
                        let _ = tx.send(batch);
                    }
                    break;
                }
                if tx.send(std::mem::take(&mut batch)).is_err() {
                    break;
                }
            }
        });

        // Progress thread
        let prog_found = Arc::clone(&found);
        let prog_attempts = Arc::clone(&attempts);
        let prog_pb = pb.clone();
        let prog_handle = std::thread::spawn(move || {
            while !prog_found.load(Ordering::Relaxed) {
                prog_pb.set_position(prog_attempts.load(Ordering::Relaxed));
                std::thread::sleep(Duration::from_millis(100));
            }
        });

        // Worker pool
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.thread_count)
            .thread_name(|i| format!("cracker-{i}"))
            .build()?;

        let verifier = Arc::clone(&self.verifier);
        let work_found = Arc::clone(&found);
        let work_attempts = Arc::clone(&attempts);
        let work_pwd = Arc::clone(&found_password);
        let work_pb = pb.clone();

        pool.scope(|scope| {
            while let Ok(batch) = rx.recv() {
                if work_found.load(Ordering::Relaxed) {
                    break;
                }
                let verifier = Arc::clone(&verifier);
                let found = Arc::clone(&work_found);
                let attempts = Arc::clone(&work_attempts);
                let pwd = Arc::clone(&work_pwd);
                let pb = work_pb.clone();

                scope.spawn(move |_| {
                    for password in batch {
                        if found.load(Ordering::Relaxed) {
                            break;
                        }
                        attempts.fetch_add(1, Ordering::Relaxed);

                        // Two-phase: quick_check filters, verify confirms.
                        if verifier.quick_check(&password) && verifier.verify(&password) {
                            found.store(true, Ordering::Release);
                            let pw = String::from_utf8_lossy(&password).into_owned();
                            let current = attempts.load(Ordering::Relaxed);
                            pb.set_position(current);
                            pb.disable_steady_tick();
                            pb.finish_with_message(format!("Found: {pw}"));
                            *pwd.lock().unwrap() = Some(pw);
                            break;
                        }
                    }
                });
            }
        });

        // Signal generator + progress to stop
        found.store(true, Ordering::Release);
        let _ = src_handle.join();
        let _ = prog_handle.join();

        let total_attempts = attempts.load(Ordering::Relaxed);
        let duration = start.elapsed();

        if found_password.lock().unwrap().is_none() {
            pb.set_position(total_attempts);
            pb.finish_with_message("Not found");
        }

        let speed = if duration.as_secs_f64() > 0.0 {
            total_attempts as f64 / duration.as_secs_f64()
        } else {
            0.0
        };

        Ok(CrackResult {
            password: found_password.lock().unwrap().clone(),
            attempts: total_attempts,
            duration,
            speed,
        })
    }
}

fn build_progress_bar(total: Option<u64>) -> ProgressBar {
    match total {
        Some(n) => {
            let pb = ProgressBar::new(n);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template(
                        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} \
                         ({percent}%) {per_sec} ETA:{eta}",
                    )
                    .unwrap()
                    .progress_chars("#>-"),
            );
            pb
        }
        None => {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} [{elapsed_precise}] {pos} {per_sec}")
                    .unwrap(),
            );
            pb
        }
    }
}
