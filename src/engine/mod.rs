use crate::formats::{PasswordVerifier, VerifyOptions};
use crate::generators::{CandidateBatch, PasswordSource, WorkShard};
use anyhow::{Context, Result, bail};
use crossbeam_channel::{Receiver, Sender, bounded};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Condvar, Mutex, Once,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
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

/// Cache-line padded wrapper to prevent false sharing across worker cores.
#[repr(align(128))]
struct CachePadded<T>(T);

enum WorkPayload {
    Shard(Box<WorkShard>),
    Batch(CandidateBatch),
}

struct WorkItem {
    seq: u64,
    adaptive_used: usize,
    guided_capped: bool,
    payload: WorkPayload,
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
    target: PathBuf,
}

/// Periodic progress persistence, enabling `--resume`.
#[derive(Clone)]
struct CheckpointConfig {
    path: PathBuf,
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
        let thread_count = thread_count
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1)
            })
            .max(1);

        Ok(Self {
            verifier: Arc::from(verifier),
            source,
            thread_count,
            checkpoint: None,
            target: file_path.to_path_buf(),
        })
    }

    /// Persist the generator position to `path` every `interval`.
    pub fn with_checkpoint(mut self, path: PathBuf, interval: Duration) -> Self {
        self.checkpoint = Some(CheckpointConfig { path, interval });
        self
    }

    pub fn crack(mut self, quiet: bool) -> Result<CrackResult> {
        let start = Instant::now();
        let total = self.source.estimated_total();
        let remaining = self.source.remaining_estimate();
        let format = self.verifier.format_name();
        let encryption = self.verifier.encryption_info();
        let source_name_owned = self.source.name().to_string();
        let config_sig = self.source.config_signature();

        if !quiet {
            println!("\nFormat:  {format} ({encryption})");
            println!("Attack:  {source_name_owned}");
            println!("Threads: {}", self.thread_count);
            if let Some(t) = total {
                println!("Keyspace: {}", crate::utils::format_number(t));
            }
        }

        let pb = if quiet {
            ProgressBar::hidden()
        } else {
            build_progress_bar(remaining)
        };
        pb.enable_steady_tick(Duration::from_millis(100));

        let found = Arc::new(CachePadded(AtomicBool::new(false)));
        let found_password: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let attempts = Arc::new(CachePadded(AtomicU64::new(0)));
        let stop_signal = Arc::new((Mutex::new(false), Condvar::new()));

        let queue_cap = (self.thread_count + 2).max(2);
        let (tx, rx): (Sender<WorkItem>, Receiver<WorkItem>) = bounded(queue_cap);
        let (recycle_tx, recycle_rx): (Sender<CandidateBatch>, Receiver<CandidateBatch>) =
            bounded(queue_cap + 2);

        // Tracks which dispatched work items have been fully verified. The
        // checkpoint is the start position of the earliest incomplete item, so
        // resuming never skips unverified candidates.
        let tracker = if self.checkpoint.is_some() && self.source.checkpoint().is_some() {
            Some(Arc::new(Mutex::new(BatchTracker::default())))
        } else {
            None
        };

        // Adaptive batch/shard size: start small so slow verifiers (Office)
        // produce checkpoints and progress updates quickly, then grow up to
        // 65,536 for fast ones.
        let batch_size = Arc::new(CachePadded(AtomicUsize::new(32)));
        let thread_count = self.thread_count;

        // Generator thread
        let src_found = Arc::clone(&found);
        let gen_tracker = tracker.clone();
        let gen_batch_size = Arc::clone(&batch_size);
        let src_handle = std::thread::spawn(move || {
            let mut seq = 0u64;
            while !src_found.0.load(Ordering::Relaxed) {
                let start_ckpt = self.source.checkpoint().unwrap_or_default();
                let adaptive_used = gen_batch_size.0.load(Ordering::Relaxed);
                let (max, guided_capped) = match self.source.remaining_estimate() {
                    Some(rem) if rem > 0 => {
                        let div = (thread_count * 4).max(1) as u64;
                        let guided = usize::try_from((rem / div).max(1)).unwrap_or(usize::MAX);
                        if guided < adaptive_used {
                            (guided, true)
                        } else {
                            (adaptive_used, false)
                        }
                    }
                    _ => (adaptive_used, false),
                };

                let (payload, more) = if let Some(shard) = self.source.take_shard(max) {
                    (WorkPayload::Shard(Box::new(shard)), true)
                } else {
                    let mut batch = recycle_rx
                        .try_recv()
                        .unwrap_or_else(|_| CandidateBatch::with_capacity(max.min(4096), max * 12));
                    batch.clear();
                    let more = self.source.fill_batch(&mut batch, max);
                    if batch.is_empty() {
                        if let Some(t) = &gen_tracker {
                            let mut t = t.lock().unwrap();
                            t.exhausted = true;
                            t.next_start = self.source.checkpoint().unwrap_or_default();
                        }
                        break;
                    }
                    (WorkPayload::Batch(batch), more)
                };

                if let Some(t) = &gen_tracker {
                    let mut t = t.lock().unwrap();
                    t.in_flight.insert(seq, start_ckpt);
                    t.next_start = self.source.checkpoint().unwrap_or_default();
                    if !more {
                        t.exhausted = true;
                    }
                }

                if tx
                    .send(WorkItem {
                        seq,
                        adaptive_used,
                        guided_capped,
                        payload,
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

        // Progress + checkpoint thread (Condvar eliminates any shutdown sleep tail).
        let prog_attempts = Arc::clone(&attempts);
        let prog_pb = pb.clone();
        let checkpoint = self.checkpoint.clone();
        let prog_tracker = tracker.clone();
        let prog_stop = Arc::clone(&stop_signal);
        let source_name = source_name_owned;
        let target = canonical_target_string(&self.target);
        let prog_handle = std::thread::spawn(move || {
            let (lock, cvar) = &*prog_stop;
            let mut last_checkpoint = Instant::now();
            let mut guard = lock.lock().unwrap();
            while !*guard {
                prog_pb.set_position(prog_attempts.0.load(Ordering::Relaxed));

                if let (Some(cfg), Some(t)) = (&checkpoint, &prog_tracker)
                    && last_checkpoint.elapsed() >= cfg.interval
                {
                    let pos = t.lock().unwrap().safe_position();
                    if let Some(pos) = pos {
                        write_checkpoint(cfg, &source_name, &pos, &target, &config_sig);
                    }
                    last_checkpoint = Instant::now();
                }

                let (next_guard, _) = cvar
                    .wait_timeout(guard, Duration::from_millis(100))
                    .unwrap();
                guard = next_guard;
            }
        });

        // Persistent worker threads pull work items in FIFO order. This keeps
        // the checkpoint frontier meaningful.
        let mut workers = Vec::with_capacity(self.thread_count);
        for i in 0..self.thread_count {
            let rx = rx.clone();
            let recycle_tx = recycle_tx.clone();
            let verifier = Arc::clone(&self.verifier);
            let found = Arc::clone(&found);
            let attempts = Arc::clone(&attempts);
            let pwd = Arc::clone(&found_password);
            let tracker = tracker.clone();
            let batch_size = Arc::clone(&batch_size);
            let worker_stop = Arc::clone(&stop_signal);

            workers.push(
                std::thread::Builder::new()
                    .name(format!("cracker-{i}"))
                    .spawn(move || {
                        let mut dict_batch = CandidateBatch::with_capacity(4096, 64 * 1024);
                        while let Ok(item) = rx.recv() {
                            if found.0.load(Ordering::Relaxed) {
                                if let WorkPayload::Batch(b) = item.payload {
                                    let _ = recycle_tx.try_send(b);
                                }
                                break;
                            }
                            let batch_started = Instant::now();
                            let matched_pw = match item.payload {
                                WorkPayload::Batch(batch) => {
                                    let res = verifier.verify_batch(&batch, &found.0);
                                    if res.attempts > 0 {
                                        attempts.0.fetch_add(res.attempts, Ordering::Relaxed);
                                    }
                                    let hit =
                                        res.matched.map(|idx| batch.get(idx).unwrap().to_vec());
                                    let _ = recycle_tx.try_send(batch);
                                    hit
                                }
                                WorkPayload::Shard(shard) => shard.execute(
                                    &*verifier,
                                    &found.0,
                                    &attempts.0,
                                    &mut dict_batch,
                                ),
                            };

                            if let Some(pw_bytes) = matched_pw {
                                if !found.0.swap(true, Ordering::AcqRel) {
                                    let pw = String::from_utf8_lossy(&pw_bytes).into_owned();
                                    *pwd.lock().unwrap() = Some(pw);
                                    let (lock, cvar) = &*worker_stop;
                                    *lock.lock().unwrap() = true;
                                    cvar.notify_all();
                                }
                                break;
                            }

                            // Only mark completed if we didn't abort mid-item.
                            if !found.0.load(Ordering::Relaxed)
                                && let Some(t) = &tracker
                            {
                                t.lock().unwrap().completed.insert(item.seq);
                            }

                            // Tune the next batch/shard without chain-doubling across workers.
                            if !item.guided_capped {
                                let elapsed = batch_started.elapsed();
                                if elapsed < Duration::from_millis(250) {
                                    let _ = batch_size.0.compare_exchange(
                                        item.adaptive_used,
                                        (item.adaptive_used * 2).min(65_536),
                                        Ordering::Relaxed,
                                        Ordering::Relaxed,
                                    );
                                } else if elapsed > Duration::from_secs(1) {
                                    let _ = batch_size.0.compare_exchange(
                                        item.adaptive_used,
                                        (item.adaptive_used / 2).max(8),
                                        Ordering::Relaxed,
                                        Ordering::Relaxed,
                                    );
                                }
                            }
                        }
                    })?,
            );
        }
        // Workers own the receivers; dropping ours lets channels close cleanly.
        drop(rx);
        drop(recycle_tx);

        for w in workers {
            let _ = w.join();
        }

        // Stop generator and progress thread cleanly before finalizing output.
        found.0.store(true, Ordering::Release);
        {
            let (lock, cvar) = &*stop_signal;
            *lock.lock().unwrap() = true;
            cvar.notify_all();
        }
        let _ = src_handle.join();
        let _ = prog_handle.join();

        let total_attempts = attempts.0.load(Ordering::Relaxed);
        let duration = start.elapsed();

        pb.disable_steady_tick();
        pb.set_position(total_attempts);
        let found_pw = found_password.lock().unwrap().take();
        match &found_pw {
            Some(pw) => pb.finish_with_message(format!("Found: {pw}")),
            None => pb.finish_with_message("Not found"),
        }

        let speed = if duration.as_secs_f64() > 0.0 {
            total_attempts as f64 / duration.as_secs_f64()
        } else {
            0.0
        };

        Ok(CrackResult {
            password: found_pw,
            attempts: total_attempts,
            duration,
            speed,
        })
    }
}

fn checkpoint_tmp_path(path: &Path) -> PathBuf {
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    PathBuf::from(tmp)
}

/// Canonicalized target path string used in checkpoint files.
pub fn canonical_target_string(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

/// Validates a 4-line checkpoint file against `target` and `source` and restores
/// the generator state.
pub fn restore_checkpoint(
    ckpt_path: &Path,
    target: &Path,
    source: &mut dyn PasswordSource,
) -> Result<()> {
    let content = std::fs::read_to_string(ckpt_path)
        .with_context(|| format!("Cannot read checkpoint {}", ckpt_path.display()))?;
    let mut lines = content.lines();
    let name = lines.next().unwrap_or_default();
    let state = lines.next().unwrap_or_default();
    let saved_target = lines.next().unwrap_or_default();
    let saved_sig = lines.next().unwrap_or_default();
    if !saved_target.is_empty() {
        let current = canonical_target_string(target);
        if saved_target != current {
            bail!(
                "Checkpoint belongs to target '{saved_target}', but this run targets '{current}'"
            );
        }
    }
    if name != source.name() {
        bail!("Checkpoint belongs to attack '{name}', but this run is '{}'", source.name());
    }
    let current_sig = source.config_signature();
    if !saved_sig.is_empty() && !current_sig.is_empty() && saved_sig != current_sig {
        bail!("Checkpoint parameters do not match this run ('{saved_sig}' != '{current_sig}')");
    }
    source
        .restore(state)
        .with_context(|| format!("Invalid checkpoint state in {}", ckpt_path.display()))
}

/// Atomically persist `"{source name}\n{state}\n{target path}\n{config signature}\n"`.
fn write_checkpoint(
    cfg: &CheckpointConfig,
    source_name: &str,
    state: &str,
    target: &str,
    config_signature: &str,
) {
    static WARN_ONCE: Once = Once::new();
    let tmp = checkpoint_tmp_path(&cfg.path);
    let content = format!("{source_name}\n{state}\n{target}\n{config_signature}\n");
    let res = std::fs::write(&tmp, content).and_then(|()| std::fs::rename(&tmp, &cfg.path));
    if let Err(err) = res {
        WARN_ONCE.call_once(|| {
            eprintln!("Warning: failed to write checkpoint {}: {err}", cfg.path.display());
        });
    }
}

/// Tracks the batch frontier to derive a resume point that never skips
/// unverified candidates.
#[derive(Default)]
struct BatchTracker {
    /// Sequence -> source position at the batch's start.
    in_flight: BTreeMap<u64, String>,
    /// Sequence numbers whose batches have been fully verified.
    completed: HashSet<u64>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generators::PasswordSource;
    use crate::generators::rules::{Rule, RuleSource};
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn batch_tracker_advances_frontier_in_order() {
        let mut tracker = BatchTracker {
            next_start: "pos0".into(),
            ..Default::default()
        };
        tracker.in_flight.insert(0, "pos0".into());
        tracker.in_flight.insert(1, "pos1".into());
        tracker.in_flight.insert(2, "pos2".into());
        tracker.next_start = "pos3".into();

        // Completing 1 out-of-order cannot advance past 0.
        tracker.completed.insert(1);
        assert_eq!(tracker.safe_position().as_deref(), Some("pos0"));

        // Completing 0 advances past both 0 and 1 to 2.
        tracker.completed.insert(0);
        assert_eq!(tracker.safe_position().as_deref(), Some("pos2"));

        // Completing 2 empties in-flight; returns next_start unless exhausted.
        tracker.completed.insert(2);
        assert_eq!(tracker.safe_position().as_deref(), Some("pos3"));

        tracker.exhausted = true;
        assert_eq!(tracker.safe_position(), None);
    }

    #[test]
    fn checkpoint_tmp_path_appends_extension_safely() {
        assert_eq!(
            checkpoint_tmp_path(Path::new("/tmp/state.ckpt")),
            PathBuf::from("/tmp/state.ckpt.tmp")
        );
        assert_eq!(
            checkpoint_tmp_path(Path::new("/tmp/state.tmp")),
            PathBuf::from("/tmp/state.tmp.tmp")
        );
    }

    #[test]
    fn rule_source_checkpoint_and_restore_lossless_via_batch() {
        let mut dict = NamedTempFile::new().unwrap();
        writeln!(dict, "alpha").unwrap();
        writeln!(dict, "beta").unwrap();
        let path = dict.path().to_path_buf();

        let rules = vec![Rule::Capitalize, Rule::AppendDigits(1), Rule::Upper];
        let mut full = RuleSource::new(&path, rules.clone()).unwrap();
        let mut expected = Vec::new();
        let mut batch = CandidateBatch::new();
        while full.fill_batch(&mut batch, 2) {
            for item in &batch {
                expected.push(item.to_vec());
            }
        }

        let mut split = RuleSource::new(&path, rules.clone()).unwrap();
        split.fill_batch(&mut batch, 2);
        let mut got: Vec<Vec<u8>> = batch.iter().map(|b| b.to_vec()).collect();
        let ckpt = split.checkpoint().unwrap();

        let mut resumed = RuleSource::new(&path, rules).unwrap();
        resumed.restore(&ckpt).unwrap();
        while resumed.fill_batch(&mut batch, 2) {
            for item in &batch {
                got.push(item.to_vec());
            }
        }
        assert_eq!(got, expected);
    }

    #[test]
    fn restore_checkpoint_validates_target_attack_and_signature() {
        let mut dict = NamedTempFile::new().unwrap();
        writeln!(dict, "alpha").unwrap();
        let dict_path = dict.path().to_path_buf();
        let target = Path::new("examples/test.zip");
        let target_str = canonical_target_string(target);

        let mut src = RuleSource::new(&dict_path, vec![Rule::Upper]).unwrap();
        let state = src.checkpoint().unwrap();
        let sig = src.config_signature();

        let ckpt = NamedTempFile::new().unwrap();
        std::fs::write(ckpt.path(), format!("hybrid\n{state}\n{target_str}\n{sig}\n")).unwrap();
        restore_checkpoint(ckpt.path(), target, &mut src).unwrap();
        assert_eq!(src.checkpoint().as_deref(), Some(state.as_str()));

        // Mismatched signature is rejected.
        let mut wrong_sig_src = RuleSource::new(&dict_path, vec![Rule::Lower]).unwrap();
        assert!(restore_checkpoint(ckpt.path(), target, &mut wrong_sig_src).is_err());

        // Mismatched target is rejected.
        assert!(restore_checkpoint(ckpt.path(), Path::new("examples/test.pdf"), &mut src).is_err());
    }
}
