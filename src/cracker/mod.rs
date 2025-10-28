use crate::formats::{create_cracker, DocumentCracker};
use crate::generator::{create_generator, GeneratorMode, PasswordGenerator};
use crate::utils::format_number;
use anyhow::{Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

// 动态批次大小：线程数越多，批次越大，减少同步开销
const BASE_BATCH_SIZE: usize = 500;

pub struct CrackerEngine {
    document: Arc<Box<dyn DocumentCracker>>,
    generator: Arc<Mutex<Box<dyn PasswordGenerator>>>,
    thread_count: usize,
    found: Arc<AtomicBool>,
    attempts: Arc<AtomicU64>,
}

#[derive(Debug, Clone)]
pub struct CrackResult {
    pub password: Option<String>,
    pub attempts: u64,
    pub duration: Duration,
    pub speed: f64, // passwords per second
}

impl CrackerEngine {
    pub fn new<P: AsRef<Path>>(
        file_path: P,
        generator_mode: GeneratorMode,
        thread_count: Option<usize>,
        performance_mode: Option<&str>,
    ) -> Result<Self> {
        let document =
            create_cracker(file_path.as_ref()).context("Failed to create document cracker")?;

        let generator =
            create_generator(generator_mode).context("Failed to create password generator")?;

        // 获取CPU核心信息
        let cpu_count = num_cpus::get();
        let physical_cores = num_cpus::get_physical();

        let thread_count = thread_count.unwrap_or_else(|| {
            match performance_mode.unwrap_or("aggressive") {
                "balanced" => {
                    // 平衡模式：使用物理核心数，给系统留一些余地
                    physical_cores.saturating_sub(1).max(1)
                }
                _ => {
                    // 激进模式（默认）：使用所有逻辑核心，包括超线程
                    // 对于密码破解这种CPU密集型任务，这能获得最大性能
                    cpu_count
                }
            }
        });

        println!("\nCPU信息:");
        println!("  物理核心数: {}", physical_cores);
        println!("  逻辑核心数: {}", cpu_count);
        println!("  性能模式: {}", performance_mode.unwrap_or("aggressive"));
        println!("  使用线程数: {}", thread_count);
        println!("  批次大小: {} 密码/批次", BASE_BATCH_SIZE);

        Ok(Self {
            document: Arc::new(document),
            generator: Arc::new(Mutex::new(generator)),
            thread_count,
            found: Arc::new(AtomicBool::new(false)),
            attempts: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn crack(&self) -> Result<CrackResult> {
        let start_time = Instant::now();

        // 获取总的密码组合数
        let total_combinations = self.generator.lock().unwrap().total_combinations();

        // 创建进度条
        let progress_bar = match total_combinations {
            Some(total) => {
                println!("\n总密码组合数: {}", format_number(total));
                let pb = ProgressBar::new(total);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) | 速度: {per_sec} | 剩余时间: {eta}")
                        .unwrap()
                        .progress_chars("#>-")
                );
                pb
            }
            None => {
                // 如果无法计算总数（如字典文件），使用 spinner
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template(
                            "{spinner:.green} [{elapsed_precise}] 已尝试: {pos} | 速度: {per_sec}",
                        )
                        .unwrap(),
                );
                pb
            }
        };

        progress_bar.enable_steady_tick(std::time::Duration::from_millis(100));

        // 创建密码批次通道（减少缓冲区大小）
        let (tx, rx): (Sender<Vec<String>>, Receiver<Vec<String>>) = bounded(self.thread_count);

        // 用于存储找到的密码
        let found_password = Arc::new(Mutex::new(None));

        // 动态调整批次大小（减小批次大小以避免内存问题）
        let batch_size = BASE_BATCH_SIZE;

        // 生成器线程
        let generator = Arc::clone(&self.generator);
        let found = Arc::clone(&self.found);
        let tx_clone = tx.clone();

        let generator_thread = std::thread::spawn(move || {
            let mut batch = Vec::with_capacity(batch_size);

            while !found.load(Ordering::Relaxed) {
                let mut gen = generator.lock().unwrap();

                for _ in 0..batch_size {
                    if let Some(password) = gen.next() {
                        batch.push(password);
                    } else {
                        // 生成器耗尽
                        if !batch.is_empty() {
                            let _ = tx_clone.send(batch);
                        }
                        return;
                    }
                }

                if batch.len() == batch_size
                    && tx_clone
                        .send(std::mem::replace(&mut batch, Vec::with_capacity(batch_size)))
                        .is_err()
                {
                    return;
                }
            }
        });

        drop(tx); // 关闭发送端

        // 并行破解
        let document = Arc::clone(&self.document);
        let found_flag = Arc::clone(&self.found);
        let attempts = Arc::clone(&self.attempts);
        let found_pwd = Arc::clone(&found_password);
        let pb = Arc::new(progress_bar);

        // 使用线程池处理批次
        // 配置Rayon线程池以最大化性能
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.thread_count)
            .thread_name(|i| format!("cracker-worker-{}", i))
            .build()
            .unwrap();

        // 创建进度更新线程
        let attempts_for_progress = Arc::clone(&attempts);
        let found_for_progress = Arc::clone(&self.found);
        let pb_clone = Arc::clone(&pb);

        let progress_thread = std::thread::spawn(move || {
            while !found_for_progress.load(Ordering::Relaxed) {
                let current = attempts_for_progress.load(Ordering::Relaxed);
                pb_clone.set_position(current);
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });

        pool.scope(|scope| {
            while let Ok(batch) = rx.recv() {
                if found_flag.load(Ordering::Relaxed) {
                    break;
                }

                let document = Arc::clone(&document);
                let found_flag = Arc::clone(&found_flag);
                let attempts = Arc::clone(&attempts);
                let found_pwd = Arc::clone(&found_pwd);
                let pb = Arc::clone(&pb);

                scope.spawn(move |_| {
                    for password in batch {
                        if found_flag.load(Ordering::Relaxed) {
                            break;
                        }

                        attempts.fetch_add(1, Ordering::Relaxed);

                        match document.verify_password(&password) {
                            Ok(true) => {
                                found_flag.store(true, Ordering::Relaxed);
                                *found_pwd.lock().unwrap() = Some(password.clone());
                                // 在完成前先更新进度条到当前位置
                                let current_attempts = attempts.load(Ordering::Relaxed);
                                pb.set_position(current_attempts);
                                // 禁用自动刷新，避免进度条继续更新
                                pb.disable_steady_tick();
                                pb.finish_with_message(format!("✅ 找到密码: {}", password));
                                break;
                            }
                            Ok(false) => continue,
                            Err(_e) => {
                                // 继续尝试其他密码，不要中断
                                continue;
                            }
                        }
                    }
                });
            }
        });

        // 等待生成器线程结束
        generator_thread.join().unwrap();

        // 停止进度更新线程
        self.found.store(true, Ordering::Relaxed);
        progress_thread.join().unwrap();

        // 最终更新进度条
        let total_attempts = self.attempts.load(Ordering::Relaxed);

        // 如果已经找到密码，进度条已经在第204行通过finish_with_message结束了
        // 所以只在没找到密码的情况下更新和结束进度条
        if found_password.lock().unwrap().is_none() {
            pb.set_position(total_attempts);
            pb.finish_with_message("❌ 未找到密码");
        }

        let duration = start_time.elapsed();
        let speed = total_attempts as f64 / duration.as_secs_f64();

        let final_password = found_password.lock().unwrap().clone();

        Ok(CrackResult {
            password: final_password,
            attempts: total_attempts,
            duration,
            speed,
        })
    }
}
