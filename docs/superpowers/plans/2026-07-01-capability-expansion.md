# 能力扩展实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 `password-cracking` 分四个阶段增加端到端测试、断点续跑、字典流式化、mask 攻击、以及老 Office/RAR5/7z 三种格式的密码验证支持。

**Architecture:** 所有新能力都作为现有两个核心 trait(`PasswordVerifier` 格式端、`PasswordSource` 生成端)的新实现接入。Phase 1 先建立端到端回归保护网,后续每个 Phase 都在此保护下推进。Phase 2 与 Phase 4 各有一个需人确认的设计门,执行到该门前必须停下与人敲定,不得自行预判。

**Tech Stack:** Rust 2024 edition, clap v4, rayon + crossbeam-channel, memmap2, sha1/sha2/md-5/rc4/aes/cbc/pbkdf2/hmac crypto crates, tempfile(dev),cfb(OLE2)。

## Global Constraints

以下约束全程有效,每个 task 的验收都隐含这些要求(逐字摘自 spec 第 0 节):

1. **纯 Rust,零 C 依赖**。不引入任何需要 C 工具链或系统库的 crate。
2. **三平台 CI 全绿**(Linux / macOS / Windows)。每个 Phase 结束时 `cargo fmt --check`、`cargo clippy`、`cargo test`、`cargo audit` 全部通过。
3. **两个核心 trait 的抽象边界不变**:`PasswordVerifier`(`quick_check` + `verify` 两阶段)、`PasswordSource`(`fill_batch` 流式产出,永不将整个搜索空间放入内存)。所有新能力作为这两个 trait 的**新实现**接入,**不改引擎**(`engine/mod.rs`)。**唯一例外**:Phase 2 的断点续跑落盘机制,须在 Phase 2 启动门经人确认后方可触碰 engine。
4. **每个 Phase 结束时都是可发布、测试全绿的状态**。Phase 之间严格按依赖顺序。
5. **提交信息用中文或英文均可,遵循 Conventional Commits**(`feat:` / `fix:` / `test:` / `chore:` / `docs:` / `style:`)。pre-commit hook 会自动跑 fmt/clippy/machete/audit,提交前须让其通过。
6. **所有例子文件密码均为 `92eo`**(`examples/test.zip|pdf|docx|xlsx|pptx`)。

---

## Phase 1 — 端到端集成测试 + 依赖清理

Phase 1 建立回归保护网并清理依赖噪音。完成后不改动任何 `src/` 生产逻辑(仅 `Cargo.toml` 的 machete 元数据)。

### Task 1: 端到端集成测试 `tests/e2e.rs`

**Files:**
- Create: `tests/e2e.rs`
- Reference(只读,理解公开 API): `src/lib.rs`, `src/engine/mod.rs:26-46`, `src/generators/mod.rs:29-62`, `src/cli/mod.rs:149-159`

**Interfaces:**
- Consumes(来自 `src/lib.rs` 的公开导出):
  - `password_cracking::generators::{GeneratorMode, create_source}`
  - `password_cracking::engine::CrackerEngine`
  - `CrackerEngine::new(file_path: &Path, source: Box<dyn PasswordSource>, thread_count: Option<usize>) -> Result<CrackerEngine>`
  - `CrackerEngine::crack(self, quiet: bool) -> Result<CrackResult>`
  - `CrackResult { password: Option<String>, attempts: u64, duration: Duration, speed: f64 }`
  - `GeneratorMode::Dictionary { path: PathBuf }`
  - `GeneratorMode::BruteForce { charset: String, min_length: usize, max_length: usize }`
  - `GeneratorMode::Hybrid { dictionary_path: PathBuf, rules: Vec<Rule> }`
  - `password_cracking::generators::rules::Rule`(枚举,含 `Rule::Capitalize` 等)
- Produces: 无(测试 crate,不被其它 task 消费)。但确立了后续所有 Phase 都要保持通过的回归基线。

**背景说明(dispatch 时告知 implementer):**
- `create_source(mode)` 返回 `Result<Box<dyn PasswordSource>>`,再传给 `CrackerEngine::new`。
- `CrackerEngine::crack(true)`(quiet=true)会抑制进度条与打印,适合测试。
- **注意副作用**:`DictionarySource::new` 当前会 `println!("Loaded N unique passwords")`(`src/generators/dictionary.rs:29`),测试输出里会有这行,属正常现象,不要为消除它而改生产代码(那是 Phase 2 的事)。
- 例子文件相对 crate 根目录的路径:`examples/test.zip` 等。集成测试的当前工作目录是 crate 根(`CARGO_MANIFEST_DIR`),可用 `Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/test.zip")` 稳健定位,避免依赖 CWD。

- [ ] **Step 1: 写辅助函数与第一个失败测试(字典攻击 ZIP)**

创建 `tests/e2e.rs`:

```rust
//! 端到端集成测试:通过公开 API 驱动完整破解流程,断言破出已知密码 `92eo`。
//! 不依赖 target/release 二进制,不使用 shell。

use password_cracking::engine::CrackerEngine;
use password_cracking::generators::rules::Rule;
use password_cracking::generators::{GeneratorMode, create_source};
use std::io::Write;
use std::path::PathBuf;

/// 例子文件的已知密码。
const KNOWN_PASSWORD: &str = "92eo";

/// 定位 crate 根下的例子文件,避免依赖当前工作目录。
fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
}

/// 写一个临时词表文件,返回持有它的 NamedTempFile(离开作用域即删除)。
/// 词表包含若干干扰词 + 已知密码。
fn wordlist_with_password() -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("create temp wordlist");
    writeln!(f, "wrong1").unwrap();
    writeln!(f, "wrong2").unwrap();
    writeln!(f, "{KNOWN_PASSWORD}").unwrap();
    writeln!(f, "wrong3").unwrap();
    f.flush().unwrap();
    f
}

/// 用给定 GeneratorMode 对给定文件跑一次破解,返回找到的密码。
fn crack(file: &std::path::Path, mode: GeneratorMode) -> Option<String> {
    let source = create_source(mode).expect("create source");
    let engine = CrackerEngine::new(file, source, Some(2)).expect("build engine");
    let result = engine.crack(true).expect("crack runs");
    result.password
}

#[test]
fn dictionary_cracks_zip() {
    let wl = wordlist_with_password();
    let mode = GeneratorMode::Dictionary {
        path: wl.path().to_path_buf(),
    };
    assert_eq!(crack(&example("test.zip"), mode).as_deref(), Some(KNOWN_PASSWORD));
}
```

- [ ] **Step 2: 运行确认失败(编译期,函数/路径未就绪前不会失败,但需确认框架跑起来)**

Run: `cargo test --test e2e dictionary_cracks_zip`
Expected: PASS(此测试针对已存在的生产代码,应直接通过——它锁定的是已验证基线)。若失败,说明公开 API 签名与计划不符,停下核对 `src/lib.rs` 导出。

> 说明:本 task 是"为既有行为补测试",不是 TDD 新功能。测试从一开始就应通过;它的价值是**回归保护**。若某条断言意外失败,是发现了真实回归,须报告而非修改断言迁就。

- [ ] **Step 3: 补齐其余四个格式的字典测试**

在 `tests/e2e.rs` 追加:

```rust
#[test]
fn dictionary_cracks_pdf() {
    let wl = wordlist_with_password();
    let mode = GeneratorMode::Dictionary { path: wl.path().to_path_buf() };
    assert_eq!(crack(&example("test.pdf"), mode).as_deref(), Some(KNOWN_PASSWORD));
}

#[test]
fn dictionary_cracks_docx() {
    let wl = wordlist_with_password();
    let mode = GeneratorMode::Dictionary { path: wl.path().to_path_buf() };
    assert_eq!(crack(&example("test.docx"), mode).as_deref(), Some(KNOWN_PASSWORD));
}

#[test]
fn dictionary_cracks_xlsx() {
    let wl = wordlist_with_password();
    let mode = GeneratorMode::Dictionary { path: wl.path().to_path_buf() };
    assert_eq!(crack(&example("test.xlsx"), mode).as_deref(), Some(KNOWN_PASSWORD));
}

#[test]
fn dictionary_cracks_pptx() {
    let wl = wordlist_with_password();
    let mode = GeneratorMode::Dictionary { path: wl.path().to_path_buf() };
    assert_eq!(crack(&example("test.pptx"), mode).as_deref(), Some(KNOWN_PASSWORD));
}
```

- [ ] **Step 4: 运行五个字典测试确认全部通过**

Run: `cargo test --test e2e dictionary_`
Expected: 5 passed。

- [ ] **Step 5: 补 brute-force、hybrid、负向测试**

追加:

```rust
#[test]
fn brute_force_cracks_zip() {
    // "92eo" = 数字数字小写小写,落在 alnum 长度 1..=4 空间内。
    let mode = GeneratorMode::BruteForce {
        charset: password_cracking::utils::charsets::alphanumeric(),
        min_length: 1,
        max_length: 4,
    };
    assert_eq!(crack(&example("test.zip"), mode).as_deref(), Some(KNOWN_PASSWORD));
}

#[test]
fn hybrid_cracks_zip() {
    // 词表里放小写形式,靠原词直出命中(hybrid 总是包含原词)。
    let mut f = tempfile::NamedTempFile::new().unwrap();
    writeln!(f, "decoy").unwrap();
    writeln!(f, "{KNOWN_PASSWORD}").unwrap();
    f.flush().unwrap();
    let mode = GeneratorMode::Hybrid {
        dictionary_path: f.path().to_path_buf(),
        rules: vec![Rule::Capitalize, Rule::L33t],
    };
    assert_eq!(crack(&example("test.zip"), mode).as_deref(), Some(KNOWN_PASSWORD));
}

#[test]
fn wrong_wordlist_returns_none() {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    writeln!(f, "nope1").unwrap();
    writeln!(f, "nope2").unwrap();
    f.flush().unwrap();
    let mode = GeneratorMode::Dictionary { path: f.path().to_path_buf() };
    assert_eq!(crack(&example("test.zip"), mode), None);
}
```

> 注意:`brute_force_cracks_zip` 需要 `password_cracking::utils` 为公开模块。`src/lib.rs:5` 已有 `pub mod utils;`,`charsets` 是 `utils` 下的 `pub mod`(`src/utils/mod.rs:5`),`alphanumeric()` 是 `pub fn`。若访问受限,停下核对,不要改生产可见性以外的东西(实际应可直接访问)。

- [ ] **Step 6: 运行全部 e2e 测试**

Run: `cargo test --test e2e`
Expected: 8 passed(5 字典 + brute-force + hybrid + 负向)。

- [ ] **Step 7: 运行全量测试确认无回归**

Run: `cargo test`
Expected: 原 12 单元测试 + 8 集成测试全部通过。

- [ ] **Step 8: Commit**

```bash
git add tests/e2e.rs
git commit -m "test: add end-to-end integration tests for all five formats"
```

---

### Task 2: 依赖清理(cargo-machete)

**Files:**
- Modify: `Cargo.toml`(仅新增 `[package.metadata.cargo-machete]` 段;若确认有真正未使用的依赖则删除对应行)

**Interfaces:**
- Consumes: 无
- Produces: 无

**背景说明(dispatch 时告知 implementer):**
- 目标是让 `cargo machete` 无噪音,**但不得为了让它闭嘴而移除实际被用到的依赖**。判定规则:能在 `src/` 里 grep 到引用 = 保留(machete 误报,加 ignored);grep 不到任何引用 = 才可移除。
- `memmap2` 属特殊情况:当前**确实未被引用**,但 **Phase 2 会启用**。因此本 task **不移除它**,而是加入 ignored 列表并注释说明。
- 已知 crypto 依赖多为间接/宏/条件使用,machete 常误报。必须逐一 grep 核实。

- [ ] **Step 1: 跑 machete 拿到当前报告**

Run: `cargo machete`
Expected: 输出一批 "unused dependencies"(如 `cbc crc32fast ecb hmac humansize md-5 memmap2 pbkdf2 rc4 thiserror` 等,以实际输出为准)。

- [ ] **Step 2: 逐一 grep 核实每个被报依赖是否真的在用**

对报告里的**每一个** crate 名,执行(把下划线/连字符两种形式都查,Rust 里 `md-5` crate 的 use 名是 `md5`,`crc32fast` 就是 `crc32fast`):

Run(示例,对每个依赖都要查):
```bash
# crate 名 -> 代码里的 use 名可能不同,两种都查
grep -rn "md5\|md-5" src/
grep -rn "rc4" src/
grep -rn "cbc" src/
grep -rn "ecb" src/
grep -rn "hmac" src/
grep -rn "pbkdf2" src/
grep -rn "crc32fast" src/
grep -rn "humansize" src/
grep -rn "thiserror" src/
grep -rn "memmap2\|Mmap" src/
```

记录每个的结论:有引用=保留(加 ignored),无引用=候选移除。

> 已知事实(供交叉验证,仍须自行 grep 确认):`md5`/`rc4` 用于 `src/formats/pdf.rs`;`cbc`/`aes` 用于 `src/formats/office.rs`。这些是误报,必须保留。`humansize` 在当前代码可能确实无引用(`utils::format_number` 是手写的,未用 humansize)——若 grep 确认无引用,可移除。`thiserror` 若无 `#[derive(Error)]` 使用也可移除。`crc32fast` 若 `zip.rs` 用的是自建 crc32 表(`src/formats/zip.rs:34`)而非 crc32fast crate,则可能无引用——须 grep 确认。

- [ ] **Step 3: 根据核实结果编辑 `Cargo.toml`**

对**误报**(有引用但 machete 仍报,如 crypto crates 经宏使用)与 **memmap2**(Phase 2 启用),加入 ignored 列表。在 `Cargo.toml` 末尾追加(实际 ignored 内容以 Step 2 核实结果为准):

```toml
[package.metadata.cargo-machete]
# machete 无法识别经宏/条件使用的 crypto 依赖,这些均在 formats/ 中实际使用。
# memmap2 将在 Phase 2(字典流式化)启用,此处临时忽略以避免 pre-commit 噪音。
ignored = ["memmap2"]  # 追加 Step 2 确认为"有引用但被误报"的其它 crate
```

对**真正无任何引用**的依赖(如 grep 确认 `humansize` 无引用),从 `[dependencies]` 段删除对应行。

> 决策原则:每一项都要有依据。移除=grep 零引用;ignored=grep 有引用或 Phase 2 将用。不允许无依据操作。

- [ ] **Step 4: 验证 machete 干净**

Run: `cargo machete`
Expected: "Done!" 且无 unused 列表(或仅剩 ignored 的不再报)。

- [ ] **Step 5: 验证构建与测试无回归**

Run: `cargo build && cargo test`
Expected: 构建成功,全部测试通过(移除依赖不应影响任何功能)。

- [ ] **Step 6: 验证 clippy 与 fmt**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: 无警告,格式合规。

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore(deps): clean up unused dependencies, ignore false positives"
```

---

## Phase 2 — 断点续跑 + 字典流式化

> ⚠️ **PHASE 2 启动门(执行到此必须停下,与人敲定后方可继续)**
>
> 本 Phase 有两个 spec 明确标注的待确认项,**不得自行预判**:
>
> 1. **断点续跑落盘机制触碰 engine**(与全局约束 3 冲突)。备选:(a) 引擎新增可选 checkpoint 回调;(b) 生成器自持久化。需人选定。
> 2. **字典去重行为变更**(当前全量 HashSet 去重 → 流式化后放弃或改为可选 `--dedup`)。这是用户可见行为改变,需人确认。
>
> **执行方(subagent-driven controller)到达此门时的动作**:暂停派发,把两个决策连同各自备选方案呈现给人,得到明确答复后,再据答复补写 Phase 2 的逐 task 步骤(当前计划不预写 Phase 2 的代码级步骤,因其形态取决于上述决策)。

**Phase 2 完成后的验收标准(供门后细化时参照):**
- 长时间 brute-force 可用 `--checkpoint <PATH>` 落盘、`--resume` 恢复,恢复后不重复已扫描空间。
- 字典源不再全量进内存;超大词表内存平稳。
- 从 `Cargo.toml` 的 machete ignored 移除 `memmap2`(此时已真正使用)。
- Phase 1 的 8 个集成测试仍全绿。

---

## Phase 3 — Mask 攻击模式

Phase 3 完全不碰 engine 和现有 source,纯新增。可在 Phase 1 之后独立执行(不依赖 Phase 2)。

### Task 3: `MaskSource` 生成器

**Files:**
- Create: `src/generators/mask.rs`
- Modify: `src/generators/mod.rs`(增加 `pub mod mask;`,`GeneratorMode::Mask` 变体,`create_source` 分支)

**Interfaces:**
- Consumes:
  - `super::PasswordSource` trait(`src/generators/mod.rs:11-27`),须实现 `fill_batch`/`estimated_total`/`checkpoint`/`restore`/`name`。
  - `crate::utils::charsets`(`DIGITS`/`LOWERCASE`/`UPPERCASE`/`SPECIAL`,`src/utils/mod.rs:5-18`)。
- Produces:
  - `pub struct MaskSource`
  - `MaskSource::new(mask: &str) -> anyhow::Result<MaskSource>`
  - 新枚举变体 `GeneratorMode::Mask { mask: String }`(供 Task 4 的 CLI 消费)

**背景说明(dispatch 时告知 implementer):**
- 参照 `src/generators/brute_force.rs` 的进位逻辑,但**每个位置的字符集可不同**(brute-force 是所有位置同一字符集)。
- `PasswordSource` trait 要求 `fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>) -> bool`,batch 满 1024 或耗尽;`estimated_total(&self) -> Option<u64>`;`checkpoint(&self) -> Option<String>`;`restore(&mut self, &str) -> Result<()>`;`name(&self) -> &str`。
- Mask token 规则:`?d`=数字,`?l`=小写,`?u`=大写,`?s`=特殊,`?a`=全部(数字+小写+大写+特殊),`??`=字面 `?`,其它字面字符=单元素字符集。首版**不**支持自定义 `-1` 字符集(YAGNI)。

- [ ] **Step 1: 写失败测试——mask 解析与位置字符集**

创建 `src/generators/mask.rs`,先写测试模块:

```rust
use super::PasswordSource;
use crate::utils::charsets;
use anyhow::{Result, bail};

/// hashcat 式 mask 攻击:逐位独立字符集,笛卡尔积流式产出。
pub struct MaskSource {
    /// 每个位置的候选字节集合。
    positions: Vec<Vec<u8>>,
    /// 当前各位置索引。
    indices: Vec<usize>,
    exhausted: bool,
}

impl MaskSource {
    pub fn new(mask: &str) -> Result<Self> {
        let positions = parse_mask(mask)?;
        if positions.is_empty() {
            bail!("Mask produced zero positions");
        }
        let len = positions.len();
        Ok(Self {
            positions,
            indices: vec![0; len],
            exhausted: false,
        })
    }
}

/// 把 mask 字符串解析为逐位置字符集。
fn parse_mask(mask: &str) -> Result<Vec<Vec<u8>>> {
    let mut positions = Vec::new();
    let bytes = mask.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'?' {
            if i + 1 >= bytes.len() {
                bail!("Mask ends with dangling '?'");
            }
            let set: Vec<u8> = match bytes[i + 1] {
                b'd' => charsets::DIGITS.bytes().collect(),
                b'l' => charsets::LOWERCASE.bytes().collect(),
                b'u' => charsets::UPPERCASE.bytes().collect(),
                b's' => charsets::SPECIAL.bytes().collect(),
                b'a' => charsets::all().bytes().collect(),
                b'?' => vec![b'?'],
                other => bail!("Unknown mask token: ?{}", other as char),
            };
            positions.push(set);
            i += 2;
        } else {
            positions.push(vec![bytes[i]]);
            i += 1;
        }
    }
    Ok(positions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mixed_mask() {
        // ?d?l = 数字 + 小写
        let pos = parse_mask("?d?l").unwrap();
        assert_eq!(pos.len(), 2);
        assert_eq!(pos[0].len(), 10); // digits
        assert_eq!(pos[1].len(), 26); // lowercase
    }

    #[test]
    fn parse_literal_and_escape() {
        // a?? = 字面 'a' + 字面 '?'
        let pos = parse_mask("a??").unwrap();
        assert_eq!(pos.len(), 2);
        assert_eq!(pos[0], vec![b'a']);
        assert_eq!(pos[1], vec![b'?']);
    }

    #[test]
    fn dangling_question_errors() {
        assert!(parse_mask("?d?").is_err());
    }

    #[test]
    fn unknown_token_errors() {
        assert!(parse_mask("?z").is_err());
    }
}
```

- [ ] **Step 2: 运行解析测试确认通过**

Run: `cargo test --lib mask::tests::parse`
Expected: `parse_mixed_mask`、`parse_literal_and_escape` 通过;`dangling_question_errors`、`unknown_token_errors` 通过。

> 这些是纯解析测试,`MaskSource::new` 已依赖 `parse_mask`,应直接通过。

- [ ] **Step 3: 写失败测试——`PasswordSource` 序列与总数**

在 `mask.rs` 的 `tests` 模块追加:

```rust
    #[test]
    fn sequence_two_positions() {
        // ?d?d 的前几个应是 00, 01, 02...(按进位)
        let mut src = MaskSource::new("?d?d").unwrap();
        let mut batch = Vec::new();
        src.fill_batch(&mut batch);
        assert_eq!(&*batch[0], b"00");
        assert_eq!(&*batch[1], b"01");
        assert_eq!(&*batch[10], b"10");
    }

    #[test]
    fn estimated_total_is_product() {
        // ?d?l = 10 * 26 = 260
        let src = MaskSource::new("?d?l").unwrap();
        assert_eq!(src.estimated_total(), Some(260));
    }

    #[test]
    fn literal_prefix() {
        // "9?d" = '9' 后跟一个数字 -> 90,91,...,99
        let mut src = MaskSource::new("9?d").unwrap();
        let mut batch = Vec::new();
        src.fill_batch(&mut batch);
        assert_eq!(&*batch[0], b"90");
        assert_eq!(&*batch[9], b"99");
        assert_eq!(batch.len(), 10);
    }
```

- [ ] **Step 4: 运行确认失败**

Run: `cargo test --lib mask::tests::sequence_two_positions`
Expected: FAIL —— `PasswordSource` 尚未实现(`fill_batch` 不存在 / 编译错误)。

- [ ] **Step 5: 实现 `PasswordSource` for `MaskSource`**

在 `mask.rs` 中(`tests` 模块之前)追加实现。进位逻辑:最后一位最快变化(与 brute_force.rs 一致,最右侧先进位)。

```rust
impl MaskSource {
    /// 构造当前索引对应的候选。
    fn current(&self) -> Box<[u8]> {
        let mut buf = Vec::with_capacity(self.positions.len());
        for (pos, &idx) in self.positions.iter().zip(&self.indices) {
            buf.push(pos[idx]);
        }
        buf.into_boxed_slice()
    }

    /// 索引进位;返回 false 表示已到末尾(全部耗尽)。
    fn increment(&mut self) -> bool {
        for i in (0..self.positions.len()).rev() {
            if self.indices[i] + 1 < self.positions[i].len() {
                self.indices[i] += 1;
                return true;
            }
            self.indices[i] = 0;
        }
        false
    }
}

impl PasswordSource for MaskSource {
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>) -> bool {
        if self.exhausted {
            return false;
        }
        const BATCH: usize = 1024;
        batch.clear();
        for _ in 0..BATCH {
            if self.exhausted {
                break;
            }
            batch.push(self.current());
            if !self.increment() {
                self.exhausted = true;
            }
        }
        !batch.is_empty()
    }

    fn estimated_total(&self) -> Option<u64> {
        let mut total: u64 = 1;
        for pos in &self.positions {
            total = total.checked_mul(pos.len() as u64)?;
        }
        Some(total)
    }

    fn checkpoint(&self) -> Option<String> {
        Some(format!(
            "{}:{}",
            self.indices
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(","),
            self.exhausted as u8
        ))
    }

    fn restore(&mut self, checkpoint: &str) -> Result<()> {
        let parts: Vec<&str> = checkpoint.splitn(2, ':').collect();
        if parts.len() != 2 {
            bail!("Invalid mask checkpoint");
        }
        self.indices = if parts[0].is_empty() {
            vec![0; self.positions.len()]
        } else {
            parts[0]
                .split(',')
                .map(|s| s.parse::<usize>())
                .collect::<Result<Vec<_>, _>>()?
        };
        if self.indices.len() != self.positions.len() {
            bail!("Mask checkpoint length mismatch");
        }
        self.exhausted = parts[1] == "1";
        Ok(())
    }

    fn name(&self) -> &str {
        "mask"
    }
}
```

- [ ] **Step 6: 运行 mask 全部单元测试**

Run: `cargo test --lib mask`
Expected: 全部通过(解析 4 个 + 序列/总数/字面 3 个)。

- [ ] **Step 7: 接入 `generators/mod.rs`**

修改 `src/generators/mod.rs`:

1. 在模块声明区(`pub mod brute_force;` 附近,`src/generators/mod.rs:3-5`)加:
```rust
pub mod mask;
```

2. 在 `GeneratorMode` 枚举(`src/generators/mod.rs:31-44`)追加变体:
```rust
    Mask {
        mask: String,
    },
```

3. 在 `create_source`(`src/generators/mod.rs:47-62`)的 `match` 追加分支:
```rust
        GeneratorMode::Mask { mask } => Ok(Box::new(mask::MaskSource::new(&mask)?)),
```

- [ ] **Step 8: 运行确认接入后仍全绿**

Run: `cargo build && cargo test --lib`
Expected: 编译通过,所有单元测试通过。

- [ ] **Step 9: Commit**

```bash
git add src/generators/mask.rs src/generators/mod.rs
git commit -m "feat(generators): add hashcat-style mask attack source"
```

---

### Task 4: `mask` CLI 子命令 + 集成测试

**Files:**
- Modify: `src/cli/mod.rs`(`AttackMode` 增加 `Mask` 子命令,`build_generator_mode` 增加分支)
- Modify: `tests/e2e.rs`(增加 mask 集成测试)

**Interfaces:**
- Consumes:
  - `GeneratorMode::Mask { mask: String }`(Task 3 产出)
  - clap `Subcommand` 派生(`src/cli/mod.rs:34-81` 现有模式)
- Produces:
  - CLI 子命令 `mask --mask <MASK>`

**背景说明(dispatch 时告知 implementer):**
- 参照 `AttackMode::BruteForce`(`src/cli/mod.rs:42-53`)的写法增加 `Mask` 子命令。
- `build_generator_mode`(`src/cli/mod.rs:84-146`)的 `match` 需增加 `AttackMode::Mask { .. }` 分支,直接返回 `GeneratorMode::Mask { mask: mask.clone() }`。无需 wordlist 校验(mask 不读文件)。
- `92eo` = 数字数字小写小写,对应 mask `?d?d?l?l`,可命中,用于集成测试。

- [ ] **Step 1: 增加 `Mask` 子命令到 `AttackMode`**

在 `src/cli/mod.rs` 的 `AttackMode` 枚举末尾(`Hybrid { .. }` 之后,`src/cli/mod.rs:80` 的 `}` 之前)追加:

```rust
    /// Mask attack: per-position character sets (hashcat-style, e.g. ?u?l?l?d?d)
    Mask {
        /// Mask string. Tokens: ?d digit, ?l lower, ?u upper, ?s special, ?a all, ?? literal '?'
        #[arg(short, long)]
        mask: String,
    },
```

- [ ] **Step 2: 增加 `build_generator_mode` 分支**

在 `src/cli/mod.rs` 的 `build_generator_mode` 的 `match &self.mode` 里(`Hybrid` 分支之后,`src/cli/mod.rs:145` 的 `}` 之前)追加:

```rust
            AttackMode::Mask { mask } => Ok(GeneratorMode::Mask { mask: mask.clone() }),
```

- [ ] **Step 3: 构建确认 CLI 编译通过**

Run: `cargo build`
Expected: 编译成功。

- [ ] **Step 4: 手动冒烟测试(可选但推荐)**

Run: `cargo run --release -- -q -f examples/test.zip mask -m '?d?d?l?l'`
Expected: 输出 `92eo`。

- [ ] **Step 5: 写 mask 集成测试**

在 `tests/e2e.rs` 追加:

```rust
#[test]
fn mask_cracks_zip() {
    // "92eo" = 数字数字小写小写 -> ?d?d?l?l
    let mode = GeneratorMode::Mask { mask: "?d?d?l?l".to_string() };
    assert_eq!(crack(&example("test.zip"), mode).as_deref(), Some(KNOWN_PASSWORD));
}
```

- [ ] **Step 6: 运行 mask 集成测试**

Run: `cargo test --test e2e mask_cracks_zip`
Expected: PASS。

- [ ] **Step 7: 运行全量测试确认无回归**

Run: `cargo test`
Expected: 全部通过(单元 + 集成,含新增 mask 测试)。

- [ ] **Step 8: clippy + fmt**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: 无警告,格式合规。

- [ ] **Step 9: Commit**

```bash
git add src/cli/mod.rs tests/e2e.rs
git commit -m "feat(cli): add mask attack subcommand"
```

---

## Phase 4 — 扩展格式(仅验证)

> ⚠️ **PHASE 4 启动门(执行到此必须停下,与人敲定 + 调研后方可继续)**
>
> spec 明确标注 Phase 4 有需专门调研确认的项,**不得自行预判**:
>
> 1. **7z 的 verify-only 校验点**比 RAR5 复杂(7z header 本身可能被加密),需确认校验方式(可能需解密 header 并校验 CRC)。
> 2. **RAR4 是否支持**(还是仅 RAR5,RAR4 只识别并 bail 提示)。
> 3. **测试文件来源**:三种新格式各需一个已知密码的加密样本文件加入 `examples/`,需确定如何生成(工具链、是否纯 Rust 可生成、密码是否统一为 `92eo`)。
>
> **执行方到达此门时的动作**:暂停派发,先就上述三项做调研并把结论 + 建议呈现给人;得到明确答复后,再据答复补写 Phase 4 的逐 task 步骤(每种格式一个 `PasswordVerifier` 实现 task + 一个集成测试 task,形态参照现有 `src/formats/zip.rs|pdf.rs|office.rs` 与 `detect_format`)。

**Phase 4 完成后的验收标准(供门后细化时参照):**
- 老 Office RC4(CryptoAPI):识别 EncryptionInfo 的 RC4 版本(version=2/3),SHA-1 密钥派生 + RC4 校验值比对。
- RAR5:识别 magic `Rar!\x1a\x07\x01\x00`,解析加密 header,PBKDF2-HMAC-SHA256 派生 + 校验值比对。
- 7z:识别 magic `7z\xbc\xaf\x27\x1c`,SHA-256 迭代派生 + AES-256-CBC 校验(具体校验点门后确定)。
- `detect_format`(`src/formats/mod.rs:52-86`)正确按 magic bytes 识别新格式。
- 每种格式一个集成测试破出已知密码。
- 三平台 CI 全绿,无新增 C 依赖,与现有验证器模式一致(`quick_check`/`verify`/`format_name`/`encryption_info`)。

---

## 执行说明

- Phase 1(Task 1-2)与 Phase 3(Task 3-4)已完全展开为可执行的逐 step 计划,可直接由 subagent-driven-development 逐 task 派发。
- Phase 2 与 Phase 4 各设一个**启动门**:执行方到达门时暂停,把 spec 标注的待确认项呈现给人,得到答复后再补写该 Phase 的代码级 task(其形态取决于人的决策,故不在此预判)。
- Phase 3 不依赖 Phase 2,可在 Phase 1 后先做;若人希望先过 Phase 2 的门,亦可按序执行。
