use crate::formats::{PasswordVerifier, VerifyOptions};
use crate::generators::PasswordSource;
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender, bounded};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
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
/// worker threads fed by a bounded FIFO channel.
pub struct CrackerEngine {
    verifier: Arc<dyn PasswordVerifier>,
    source: Box<dyn PasswordSource>,
    thread_count: usize,
    checkpoint: Option<CheckpointConfig>,
    target: std::path::PathBuf,
}

/// Periodic progress persistence, enabling `--resume`.
#[derive(Clone)]
struct CheckpointConfig {
    path: std::path::PathBuf,
    interval: Duration,
}

impl CrackerEngine {
    pub fn new(
        file_path: &Path,
        source: Box<dyn PasswordSource>,
        thread_count: Option<usize>,
    ) -> Result<Self> {
        Self::new_with_options(file_path, source, thread_count, VerifyOptions::default())
    }

    pub fn new_with_options(
        file_path: &Path,
        source: Box<dyn PasswordSource>,
        thread_count: Option<usize>,
        options: VerifyOptions,
    ) -> Result<Self> {
        let verifier = crate::formats::create_verifier_with_options(file_path, options)?;
        let thread_count = thread_count.unwrap_or_else(num_cpus::get).max(1);

        Ok(Self {
            verifier: Arc::from(verifier),
            source,
            thread_count,
            checkpoint: None,
            target: file_path.to_path_buf(),
        })
    }

    /// Persist the generator position to `path` every `interval`.
    pub fn with_checkpoint(mut self, path: std::path::PathBuf, interval: Duration) -> Self {
        self.checkpoint = Some(CheckpointConfig { path, interval });
        self
    }

    pub fn crack(mut self, quiet: bool) -> Result<CrackResult> {
        let start = Instant::now();
        let total = self.source.estimated_total();
        let format = self.verifier.format_name();
        let encryption = self.verifier.encryption_info();
        let source_name = self.source.name();
        let source_name_owned = source_name.to_string();

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

        // Batches carry their sequence number and the source position at
        // their start, so the safe resume point can be tracked precisely.
        struct Batch {
            seq: u64,
            items: Vec<Box<[u8]>>,
        }

        // Bounded channel keeps memory flat.
        let (tx, rx): (Sender<Batch>, Receiver<Batch>) = bounded(self.thread_count.max(1));

        // Tracks which dispatched batches have been fully verified. The
        // checkpoint is the start position of the earliest incomplete batch,
        // so resuming never skips unverified candidates.
        let tracker = if self.checkpoint.is_some() && self.source.checkpoint().is_some() {
            Some(Arc::new(Mutex::new(BatchTracker::default())))
        } else {
            None
        };

        // Adaptive batch size: start small so slow verifiers (Office) produce
        // checkpoints and progress updates quickly, then grow for fast ones.
        let batch_size = Arc::new(std::sync::atomic::AtomicUsize::new(32));

        // Generator thread
        let src_found = Arc::clone(&found);
        let gen_tracker = tracker.clone();
        let gen_batch_size = Arc::clone(&batch_size);
        let src_handle = std::thread::spawn(move || {
            let mut batch: Vec<Box<[u8]>> = Vec::with_capacity(1024);
            let mut seq = 0u64;
            while !src_found.load(Ordering::Relaxed) {
                let start = self.source.checkpoint().unwrap_or_default();
                batch.clear();
                let max = gen_batch_size.load(Ordering::Relaxed);
                let more = self.source.fill_batch(&mut batch, max);
                if batch.is_empty() {
                    if let Some(t) = &gen_tracker {
                        let mut t = t.lock().unwrap();
                        t.exhausted = true;
                        t.next_start = self.source.checkpoint().unwrap_or_default();
                    }
                    break;
                }

                if let Some(t) = &gen_tracker {
                    let mut t = t.lock().unwrap();
                    t.in_flight.insert(seq, start);
                    t.next_start = self.source.checkpoint().unwrap_or_default();
                    if !more {
                        t.exhausted = true;
                    }
                }

                if tx
                    .send(Batch {
                        seq,
                        items: std::mem::take(&mut batch),
                    })
                    .is_err()
                {
                    break;
                }
                seq += 1;
                if !more {
                    break;
                }
            }
        });

        // Progress + checkpoint thread
        let prog_found = Arc::clone(&found);
        let prog_attempts = Arc::clone(&attempts);
        let prog_pb = pb.clone();
        let checkpoint = self.checkpoint.clone();
        let prog_tracker = tracker.clone();
        let source_name = source_name_owned;
        let target = self
            .target
            .canonicalize()
            .unwrap_or_else(|_| self.target.clone());
        let prog_handle = std::thread::spawn(move || {
            let mut last_checkpoint = Instant::now();
            while !prog_found.load(Ordering::Relaxed) {
                prog_pb.set_position(prog_attempts.load(Ordering::Relaxed));

                if let (Some(cfg), Some(t)) = (&checkpoint, &prog_tracker)
                    && last_checkpoint.elapsed() >= cfg.interval
                {
                    let pos = t.lock().unwrap().safe_position();
                    if let Some(pos) = pos {
                        write_checkpoint(cfg, &source_name, &pos, &target);
                    }
                    last_checkpoint = Instant::now();
                }

                std::thread::sleep(Duration::from_millis(100));
            }
        });

        // Persistent worker threads pull batches in FIFO order. This keeps
        // the checkpoint frontier meaningful (Rayon's LIFO queue would leave
        // early batches pending while later ones complete).
        let mut workers = Vec::with_capacity(self.thread_count);
        for i in 0..self.thread_count {
            let rx = rx.clone();
            let verifier = Arc::clone(&self.verifier);
            let found = Arc::clone(&found);
            let attempts = Arc::clone(&attempts);
            let pwd = Arc::clone(&found_password);
            let pb = pb.clone();
            let tracker = tracker.clone();
            let batch_size = Arc::clone(&batch_size);

            workers.push(
                std::thread::Builder::new()
                    .name(format!("cracker-{i}"))
                    .spawn(move || {
                        while let Ok(batch) = rx.recv() {
                            if found.load(Ordering::Relaxed) {
                                break;
                            }
                            let batch_started = Instant::now();
                            for password in batch.items {
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

                            if let Some(t) = &tracker {
                                t.lock().unwrap().completed.insert(batch.seq);
                            }

                            // Tune the next batch: aim for ~250ms per batch.
                            let elapsed = batch_started.elapsed();
                            let current = batch_size.load(Ordering::Relaxed);
                            if elapsed < Duration::from_millis(250) {
                                let _ = batch_size.compare_exchange(
                                    current,
                                    (current * 2).min(4096),
                                    Ordering::Relaxed,
                                    Ordering::Relaxed,
                                );
                            } else if elapsed > Duration::from_secs(1) {
                                let _ = batch_size.compare_exchange(
                                    current,
                                    (current / 2).max(8),
                                    Ordering::Relaxed,
                                    Ordering::Relaxed,
                                );
                            }
                        }
                    })?,
            );
        }
        // The workers own all receivers; dropping ours lets the channel close
        // when the generator finishes.
        drop(rx);

        for w in workers {
            let _ = w.join();
        }

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

/// Atomically persist `"{source name}\n{state}\n{target path}\n"`.
fn write_checkpoint(
    cfg: &CheckpointConfig,
    source_name: &str,
    state: &str,
    target: &std::path::Path,
) {
    let tmp = cfg.path.with_extension("tmp");
    let content = format!("{source_name}\n{state}\n{}\n", target.display());
    if std::fs::write(&tmp, content).is_ok() {
        let _ = std::fs::rename(&tmp, &cfg.path);
    }
}

/// Tracks the batch frontier to derive a resume point that never skips
/// unverified candidates.
#[derive(Default)]
struct BatchTracker {
    /// Sequence -> source position at the batch's start.
    in_flight: std::collections::BTreeMap<u64, String>,
    /// Sequence numbers whose batches have been fully verified.
    completed: std::collections::HashSet<u64>,
    /// Source position after the most recently generated batch.
    next_start: String,
    /// Whether the source has been fully generated.
    exhausted: bool,
}

impl BatchTracker {
    /// Smallest position such that every candidate before it has been verified.
    fn safe_position(&mut self) -> Option<String> {
        // Drop completed batches from the front of the in-flight window.
        while let Some((&seq, _)) = self.in_flight.iter().next() {
            if self.completed.remove(&seq) {
                self.in_flight.remove(&seq);
            } else {
                break;
            }
        }

        if let Some((_, start)) = self.in_flight.iter().next() {
            Some(start.clone())
        } else if self.exhausted {
            None
        } else {
            Some(self.next_start.clone())
        }
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
