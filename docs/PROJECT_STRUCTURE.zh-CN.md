[English](PROJECT_STRUCTURE.md) | **中文**

# 项目结构

## 目录布局

```text
password-cracking/
├── Cargo.toml                     Rust manifest (edition 2024, MSRV 1.88)
├── Cargo.lock                     Locked dependency graph
├── rustfmt.toml                   Formatting configuration
├── LICENSE                        MIT
├── README.md / README_CN.md       English / Chinese overview
├── Dockerfile                     scratch image around the static musl binary
│
├── src/
│   ├── main.rs                    Process entry point, exit codes 0/1/2
│   ├── lib.rs                     Public modules
│   ├── cli/mod.rs                 clap definitions and run()
│   ├── engine/mod.rs              Worker pool, adaptive batches, checkpoints
│   ├── formats/
│   │   ├── mod.rs                 PasswordVerifier trait, VerifyOptions, auto-detection
│   │   ├── zip.rs                 ZipCrypto + AES ZIP
│   │   ├── pdf.rs                 RC4/AES PDF, user + owner passwords
│   │   └── office.rs              Agile-encrypted Office (OLE2 + XML)
│   ├── generators/
│   │   ├── mod.rs                 PasswordSource trait, GeneratorMode, factory
│   │   ├── dictionary.rs          mmap streaming dictionary, optional dedup
│   │   ├── brute_force.rs         Index-based brute force
│   │   ├── mask.rs                Per-position mask source
│   │   └── rules.rs               Hybrid dictionary + mutation rules
│   └── utils/mod.rs               Charsets, formatting, validation
│
├── tests/e2e.rs                   End-to-end tests over examples/
│
├── examples/
│   ├── create_test_files.py       Fixture generator (pikepdf, msoffcrypto-tool)
│   ├── README.md / README.zh-CN.md
│   ├── test.zip                    ZipCrypto
│   ├── test_aes.zip                AES-256 ZIP (7z)
│   ├── test_multi.zip              Two encrypted entries
│   ├── test_mixed.zip              Unencrypted first entry, encrypted second
│   ├── test.pdf                    RC4-128 (R3)
│   ├── test_aes128.pdf             AES-128 (R4)
│   ├── test_r6.pdf                 AES-256 (R6, Algorithm 2.B)
│   ├── test_xref.pdf               AES-256 R6 with xref stream
│   ├── test_owner_r4.pdf           AES-128, owner password only
│   ├── test_owner_r6.pdf           AES-256, owner password only
│   ├── test.docx / .xlsx / .pptx   Agile AES-256
│   └── pyproject.toml / uv.lock    Python tooling for fixture generation
│
├── wordlists/
│   ├── common-passwords.txt       33 common passwords
│   ├── chinese-common.txt         40 common Chinese passwords
│   ├── pins-4digit.txt            10,000 four-digit PINs
│   └── pins-6digit.txt            1,000,000 six-digit PINs
│
├── docs/
│   ├── README.md / README.zh-CN.md
│   ├── USER_GUIDE.md / .zh-CN.md
│   ├── DISTRIBUTION.md / .zh-CN.md
│   ├── PERFORMANCE.md / .zh-CN.md
│   ├── PROJECT_STRUCTURE.md / .zh-CN.md
│   ├── DEVELOPMENT.md / .zh-CN.md
│   ├── CI-CD.md / .zh-CN.md
│   ├── RELEASE_LINUX.md / .zh-CN.md
│   ├── ZIP_IMPLEMENTATION.md / .zh-CN.md
│   ├── PDF_IMPLEMENTATION.md / .zh-CN.md
│   ├── OFFICE_IMPLEMENTATION.md / .zh-CN.md
│   └── superpowers/               Historical design records (not maintained)
│
├── packaging/
│   ├── nfpm.yaml                  deb / rpm / apk configuration
│   ├── apt/index.html             APT landing page
│   ├── apt/password-cracking.gpg  Public signing key
│   ├── aur/PKGBUILD.tmpl
│   ├── aur/.SRCINFO.tmpl
│   └── homebrew/password-cracking.rb.tmpl
│
├── scripts/
│   ├── fmt.sh                     Format + clippy (+ tests)
│   ├── release.sh                 Version bump + tag
│   ├── install.sh                 Static binary installer
│   ├── gen-package-manifests.py   Homebrew/AUR manifest renderer
│   ├── build-apt-repo.sh          Signed APT repository builder
│   ├── test-package-manifests.sh  Manifest smoke test
│   ├── check-large-files.sh
│   └── install-hooks.sh
│
└── .github/workflows/
    ├── ci.yml                     Quality, tests, 8-target build, release
    ├── publish.yml                APT/Homebrew/AUR/GHCR/crates.io
    └── release-drafter.yml
```

## 核心抽象

### `PasswordVerifier`（`src/formats/mod.rs`）

```rust
pub trait PasswordVerifier: Send + Sync {
    /// Cheap filter. Return true if the password may be correct.
    fn quick_check(&self, password: &[u8]) -> bool;
    /// Authoritative check.
    fn verify(&self, password: &[u8]) -> bool;
    fn format_name(&self) -> &'static str;
    fn encryption_info(&self) -> &str;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VerifyOptions {
    pub try_pdf_owner: bool,
}
```

`quick_check` 仅对 ZIP ZipCrypto（12 字节头）是真正的过滤器。PDF 和 Office 返回 `true`，因此引擎只调用一次 `verify`。`VerifyOptions` 将 CLI 层策略传入验证器构造函数。

### `PasswordSource`（`src/generators/mod.rs`）

```rust
pub trait PasswordSource: Send {
    /// Fill up to `max` candidates. Returns false when exhausted.
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>, max: usize) -> bool;
    fn estimated_total(&self) -> Option<u64>;
    /// Opaque position; None if the source is not resumable.
    fn checkpoint(&self) -> Option<String>;
    fn restore(&mut self, state: &str) -> anyhow::Result<()>;
    fn name(&self) -> &'static str;
}
```

数据源从不将完整密钥空间实例化到内存中。字典数据源按字节偏移读取内存映射文件；暴力破解和 Mask 根据索引计算候选；混合攻击维护一个大小受限的待处理队列。

### `CrackerEngine`（`src/engine/mod.rs`）

```text
fill_batch(max) ──► bounded FIFO channel ──► worker threads
                                              │
                        quick_check() ──► verify()
                                              │
                              mark batch completed
                                              │
                              verified checkpoint frontier
```

- 常驻 FIFO worker 线程使完成顺序大致有序，这是获得有意义的检查点前沿所必需的。
- 批次目标从 32 开始，向每批次约 250 ms 自适应调整，上限为 4096。
- 通道容量等于 worker 线程数，因此生成器不会大幅超前。
- 批次跟踪器记录每个批次开始时的数据源位置。检查点是尚未完成的最早批次的起点。
- 进度线程按固定间隔更新进度条并写入检查点。

## 格式检测

| 魔数字节 | 格式 | 说明 |
|-------------|--------|-------|
| `PK\x03\x04` | ZIP | 同时也是 OOXML 容器 |
| `%PDF-` | PDF | |
| `D0 CF 11 E0 A1 B1 1A E1` | Office（OLE2） | 采用 Agile 加密的 OOXML 封装在 OLE2 中 |
| `PK\x03\x04` + OOXML 部件名 | Office（未加密的 OOXML） | 未加密；作为不支持格式拒绝 |

加密的 Office 文件是 OLE2 复合文档；扩展名用于区分 `.docx/.xlsx/.pptx` 与 `.doc/.xls/.ppt`，但两者在 Agile 加密下都使用相同的 `EncryptionInfo` / `EncryptedPackage` 流。

## 数据流

1. `cli::run()` 校验目标并构建 `GeneratorMode`。
2. `create_source()` 返回具体的 `PasswordSource`。
3. `create_verifier_with_options()` 探测文件并返回 `PasswordVerifier`。
4. `CrackerEngine::crack()` 启动生成器、worker 线程和进度线程。
5. 成功时，worker 线程保存密码、设置停止标志并结束进度条。
6. CLI 打印结果并以 0（找到）、1（未找到）或 2（错误）退出。

## 依赖

| 类别 | crate |
|----------|--------|
| CLI | `clap` 4.6（derive、cargo）、`clap_complete` 4.6、`clap_mangen` 0.3 |
| 并发 | `crossbeam-channel` 0.5、`num_cpus` 1.17 |
| 进度 | `indicatif` 0.18 |
| 错误处理 | `anyhow` 1.0 |
| ZIP | `zip` 9.1（`aes-crypto`、`deflate`） |
| 哈希 | `sha2` 0.11、`sha1` 0.11、`md-5` 0.11 |
| 加密算法 | `aes` 0.9、`cipher` 0.5 |
| OLE2 | `cfb` 0.15 |
| 字典 | `memmap2` 0.9 |
| 工具 | `base64` 0.23 |
| 开发 | `tempfile` 3.27 |

所有依赖均保持为最新的兼容版本；Dependabot 每周跟踪更新。

## 扩展点

- **新增格式**：实现 `PasswordVerifier`，在 `src/formats/mod.rs` 中添加检测和构造逻辑，添加测试样本和端到端测试。
- **新增生成器**：实现 `PasswordSource`，添加一个 `GeneratorMode` 变体、一个 CLI 子命令以及单元/端到端测试。
- **新增软件包渠道**：在 `packaging/` 下添加模板，扩展 `scripts/gen-package-manifests.py`，在 `publish.yml` 中添加一个任务。

完整工作流见 [DEVELOPMENT.md](DEVELOPMENT.md)。
