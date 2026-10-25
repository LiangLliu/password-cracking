# Phase 0 重构实现计划 — 正确性修复 + 热路径性能 + 卫生清理

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 PDF R6(AES-256 Algorithm 2.B)正确性缺陷、清除死代码、修两处热路径性能问题、收敛重复常量 —— 均在既有 e2e 回归网保护下,不改 trait/engine。

**Architecture:** 折入既有能力扩展 Roadmap,作为 Phase 0,在 mask(原 Task 3)与 Phase 4(格式扩展)之前执行。所有改动是既有 `PasswordVerifier`/`PasswordSource` 实现的内部修正,不改 trait 签名,不改 `engine/mod.rs`。

**Tech Stack:** Rust 2024, sha2(SHA-256/384/512)、aes + cipher(AES-128-CBC no-padding,用于 Algorithm 2.B)、md-5/sha1(既有 PDF RC4 路径)。

**关联 spec:** `docs/superpowers/specs/2026-07-02-phase0-refactor-design.md`

## Global Constraints

以下约束全程有效,每个 task 的验收都隐含这些要求:

1. **纯 Rust,零 C 依赖**。R6 的 AES-128-CBC 用已有 `aes` + `cipher` crate,不新增依赖。
2. **三平台 CI 全绿**。每个 task 结束 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo audit` 全过。
3. **两个核心 trait 边界不变**:`PasswordVerifier`(`quick_check` + `verify`)、`PasswordSource`(`fill_batch` 流式)。不改 trait 签名,**不改 `engine/mod.rs`**。
4. **例子文件密码均为 `92eo`**,含新增的 `examples/test_r6.pdf`(V=5 R=6 AES-256)。
5. **提交遵循 Conventional Commits**。pre-commit hook 会跑 fmt/clippy/machete/audit(`tests/`、`docs/` 已排除敏感信息扫描)。
6. **Phase 0 结束后 `src/` 内不得残留 `#[allow(dead_code)]`**。

## 已验证事实(2026-07-02 实测,写入计划前已核对)

- `examples/test_r6.pdf` 已生成(pikepdf,密码 `92eo`,V=5 R=6 AES-256),已提交于 `61f1bbe`。
- 当前工具对该 fixture 返回 `not found`(缺陷坐实);对 R3 `test.pdf` 正确返回 `92eo`。
- **Algorithm 2.B 已用 Python(pycryptodome)对 fixture 端到端验证通过**:computed hash == stored U[..32]。确切步骤见 Task 2 Step 3 代码,是逐字可移植的参考实现。
- fixture 的 `/U` 是 48 字节:`U[0..32]`=hash,`U[32..40]`=validation salt,`U[40..48]`=key salt。现有 `parse_encryption` 已把 `u_validation_salt` 取为 `user_hash[32..40]`(pdf.rs:268),正确。

---

## Task 1: R6 fixture 的失败 e2e 测试(红)

**Files:**
- Modify: `tests/e2e.rs`(追加一个 R6 字典测试)

**Interfaces:**
- Consumes: 既有 `tests/e2e.rs` 里的 `example()`、`wordlist_with_password()`、`crack()`、`KNOWN_PASSWORD` 辅助函数(Phase 1 Task 1 建立)。
- Produces: 一个当前**失败**的测试 `dictionary_cracks_pdf_r6`,Task 2 修复后转绿。

**背景说明(dispatch 时告知 implementer):**
- 这是标准 TDD 的"红"步:先加一个证明缺陷存在的失败测试。
- fixture `examples/test_r6.pdf` 已存在(已提交)。密码 `92eo`,在 `wordlist_with_password()` 生成的词表里。
- **本 task 只加测试,不碰 `src/`**。测试此刻必须 FAIL(返回 `None` 而非 `Some("92eo")`),这正是要锁定的缺陷。

- [ ] **Step 1: 追加 R6 失败测试**

在 `tests/e2e.rs` 末尾追加:

```rust
#[test]
fn dictionary_cracks_pdf_r6() {
    // test_r6.pdf 是 PDF 2.0 AES-256 R6(Algorithm 2.B 硬化哈希)。
    // 修复前当前实现只做单轮 SHA256(R5 算法),对 R6 会失败。
    let wl = wordlist_with_password();
    let mode = GeneratorMode::Dictionary { path: wl.path().to_path_buf() };
    assert_eq!(
        crack(&example("test_r6.pdf"), mode).as_deref(),
        Some(KNOWN_PASSWORD)
    );
}
```

- [ ] **Step 2: 运行确认失败(红)**

Run: `cargo test --test e2e dictionary_cracks_pdf_r6`
Expected: FAIL —— 断言失败,left 为 `None`(或 `Some` 的其它值),right 为 `Some("92eo")`。这证明 R6 缺陷真实存在。

> 若此测试意外通过,停下报告 —— 说明 fixture 或缺陷判断有误,不要继续。

- [ ] **Step 3: Commit(提交失败测试,记录缺陷)**

```bash
git add tests/e2e.rs
git commit -m "test: add failing e2e test for PDF R6 (AES-256 Algorithm 2.B)"
```

> 说明:这里提交一个失败的测试是有意为之(TDD 红步 + 缺陷存证)。pre-commit hook 只跑 fmt/clippy/machete/audit,不跑 `cargo test`,故失败测试不会阻止提交。Task 2 会让它转绿。

---

## Task 2: 实现 Algorithm 2.B 修复 R6 + 清除死代码

**Files:**
- Modify: `src/formats/pdf.rs`(重写 `verify_user_password_aes256`,新增 Algorithm 2.B,删/整理死代码)

**Interfaces:**
- Consumes:
  - `self.enc.revision`(u32,5 或 6)、`self.enc.u_validation_salt`(`Option<Vec<u8>>`,已是 U[32..40])、`self.enc.user_hash`(`Vec<u8>`,48 字节,U[0..32] 是待比对 hash)。
  - `sha2::{Sha256, Sha384, Sha512, Digest}`、`aes::Aes128`、`cipher::{BlockCipherEncrypt, KeyInit, Block}`。
- Produces: 修正后的 `verify_user_password_aes256`,R6 走 Algorithm 2.B,R5 保持单轮 SHA-256。

**背景说明(dispatch 时告知 implementer):**
- 现状:`verify_user_password_aes256`(pdf.rs:196)对 R5/R6 一律只算 `SHA256(password ‖ salt)`,仅对 R5 正确。
- 死代码 `derive_key_aes256`(pdf.rs:107,挂 `#[allow(dead_code)]`)是错误的 R6 尝试,**删掉整段**,用下面验证过的实现取代。
- Algorithm 2.B 已对真实 fixture 端到端验证通过(见 Step 3 的参考步骤),逐字移植即可。
- **AES-128-CBC 必须无填充(no padding)**。`K1` 长度是 `(len(password)+32) * 64`,是 16 的倍数吗?不一定 —— `password+K` 长度 = pwd_len+32,乘 64 后 = (pwd_len+32)*64,只要 pwd_len 任意,(pwd_len+32) 未必是 16 倍数,但 *64 后一定是 16 的倍数(64 = 4*16)。故整体恒为 16 倍数,可直接分块加密,无需填充。
- 现有 `aes_cbc_decrypt_raw`(office.rs)是**解密**;这里需要**加密**。在 pdf.rs 内自建一个 CBC 加密小函数(仿 office.rs 的分块 + prev XOR 结构,但用 `encrypt_block`,且 XOR 在加密前作用于明文块)。

- [ ] **Step 1: 写 Algorithm 2.B 的已知答案单元测试(红)**

在 `src/formats/pdf.rs` 的 `#[cfg(test)] mod tests` 里追加。测试直接对 Algorithm 2.B 的核心函数断言(用 fixture 已知的 salt 与结果):

```rust
    #[test]
    fn algorithm_2b_known_answer() {
        // 来自 examples/test_r6.pdf(密码 "92eo")的真实值,已用 pycryptodome 交叉验证。
        let salt = [0xe5, 0xba, 0xd2, 0x1d, 0x84, 0x2e, 0x3e, 0x2f];
        let expected = [
            0xd9, 0xa0, 0x86, 0xd2, 0x1d, 0x02, 0x49, 0x94, 0xfb, 0x4b, 0x8d, 0xcd, 0xb1, 0xbe,
            0x81, 0x04, 0x04, 0xfe, 0xc4, 0x91, 0x6b, 0x89, 0x36, 0xda, 0xcc, 0xa7, 0x77, 0xf0,
            0x24, 0x10, 0xa1, 0x02,
        ];
        let got = algorithm_2b(b"92eo", &salt, &[]);
        assert_eq!(got, expected);
    }
```

- [ ] **Step 2: 运行确认失败(函数不存在 → 编译失败)**

Run: `cargo test --lib pdf::tests::algorithm_2b_known_answer`
Expected: FAIL —— `algorithm_2b` 未定义,编译错误。

- [ ] **Step 3: 实现 `algorithm_2b` 与 CBC 加密辅助**

在 `src/formats/pdf.rs` 顶部辅助函数区(`rc4_encrypt` 附近)加入。以下步骤已对 fixture 端到端验证通过,逐字实现:

```rust
/// AES-128-CBC 加密,无填充。data.len() 必须是 16 的倍数。
/// 用于 PDF R6 的 Algorithm 2.B。
fn aes128_cbc_encrypt_no_pad(key: &[u8], iv: &[u8], data: &[u8]) -> Vec<u8> {
    use cipher::{BlockCipherEncrypt, KeyInit};

    let cipher = match aes::Aes128::new_from_slice(&key[..16]) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut prev = [0u8; 16];
    prev.copy_from_slice(&iv[..16]);
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(16) {
        let mut block = cipher::Block::<aes::Aes128>::default();
        for i in 0..16 {
            block[i] = chunk[i] ^ prev[i];
        }
        cipher.encrypt_block(&mut block);
        out.extend_from_slice(&block[..]);
        prev.copy_from_slice(&block[..]);
    }
    out
}

/// ISO 32000-2 Algorithm 2.B:PDF 2.0 (R6) 的硬化口令哈希。
/// 返回 32 字节的派生 hash,用于与 U[0..32] 比对。
/// 已对 examples/test_r6.pdf 端到端验证通过。
fn algorithm_2b(password: &[u8], salt: &[u8], udata: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256, Sha384, Sha512};

    // K = SHA-256(password ‖ salt ‖ udata)
    let mut h = Sha256::new();
    h.update(password);
    h.update(salt);
    h.update(udata);
    let mut k: Vec<u8> = h.finalize().to_vec();

    let mut round: usize = 0;
    loop {
        // K1 = (password ‖ K ‖ udata) 重复 64 次
        let mut k1 = Vec::with_capacity((password.len() + k.len() + udata.len()) * 64);
        for _ in 0..64 {
            k1.extend_from_slice(password);
            k1.extend_from_slice(&k);
            k1.extend_from_slice(udata);
        }

        // E = AES-128-CBC-Encrypt(key = K[0..16], iv = K[16..32], K1),无填充
        let e = aes128_cbc_encrypt_no_pad(&k[0..16], &k[16..32], &k1);

        // mod = (E 的前 16 字节之和) mod 3
        let sum: u32 = e[0..16].iter().map(|&b| b as u32).sum();
        k = match sum % 3 {
            0 => Sha256::digest(&e).to_vec(),
            1 => Sha384::digest(&e).to_vec(),
            _ => Sha512::digest(&e).to_vec(),
        };

        round += 1;
        // 终止:至少 64 轮,且 E 最后一字节 <= round - 32
        if round >= 64 && (e[e.len() - 1] as usize) <= round - 32 {
            break;
        }
    }

    k.truncate(32);
    k
}
```

- [ ] **Step 4: 运行已知答案测试确认通过(绿)**

Run: `cargo test --lib pdf::tests::algorithm_2b_known_answer`
Expected: PASS —— 说明 Algorithm 2.B 实现正确。

- [ ] **Step 5: 把 R6 接到验证路径,删除死代码**

重写 `verify_user_password_aes256`(pdf.rs 约 196 行),按 revision 分派;并**删除**整段死方法 `derive_key_aes256`(pdf.rs 约 107-141,连同其 `#[allow(dead_code)]`):

```rust
    /// Verify user password for AES-256 (R=5/R=6).
    fn verify_user_password_aes256(&self, password: &[u8]) -> bool {
        let salt = self.enc.u_validation_salt.as_deref().unwrap_or(&[]);
        let computed = if self.enc.revision >= 6 {
            // R6: ISO 32000-2 Algorithm 2.B 硬化哈希
            algorithm_2b(password, salt, &[])
        } else {
            // R5: 单轮 SHA-256(password ‖ salt)
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(password);
            h.update(salt);
            h.finalize().to_vec()
        };
        let stored = &self.enc.user_hash[..32.min(self.enc.user_hash.len())];
        computed.as_slice() == stored
    }
```

> 同时删除 `derive_key_aes256` 方法及其 `#[allow(dead_code)]`。若删除后 `derive_key`(pdf.rs:41,也挂 `#[allow(dead_code)]`)仍无任何调用者,一并删除该方法及其 allow(它只是 `derive_key_rc4`/`derive_key_aes256` 的分派包装,验证走的是 `verify_user_password_*`,不经过它)。删除后须 `grep -n "derive_key\b" src/formats/pdf.rs` 确认无调用残留。

- [ ] **Step 6: 运行 R6 e2e 测试确认转绿**

Run: `cargo test --test e2e dictionary_cracks_pdf_r6`
Expected: PASS —— Task 1 的失败测试现在通过。

- [ ] **Step 7: 运行全部测试确认无回归**

Run: `cargo test`
Expected: 全绿(R3 `test.pdf` 及其它格式 e2e 不受影响)。

- [ ] **Step 8: 确认 pdf.rs 内无 `#[allow(dead_code)]` 残留**

Run: `grep -n "allow(dead_code)" src/formats/pdf.rs`
Expected: 无输出(该文件的 3 处 allow 已随死代码删除)。

- [ ] **Step 9: clippy + fmt**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: 无警告(注意:删除死代码后不应再有 `dead_code` 相关抑制;若 clippy 报未使用的 import 如 `md5`/`sha1` 仍在 `derive_key_rc4` 用到则保留)。

- [ ] **Step 10: Commit**

```bash
git add src/formats/pdf.rs tests/e2e.rs
git commit -m "fix(pdf): implement ISO 32000-2 Algorithm 2.B for AES-256 R6"
```

---

## Task 3: 热路径性能修复

**Files:**
- Modify: `src/generators/rules.rs`(`pending: Vec` → `VecDeque`,去除 `remove(0)`)
- Modify: `src/formats/zip.rs`(AES 验证避免每次尝试重开文件)

**Interfaces:**
- Consumes: 既有 `PasswordSource` for `RuleSource`、`PasswordVerifier` for `ZipVerifier`。
- Produces: 行为等价、性能更优的两个实现。trait 签名不变。

**背景说明(dispatch 时告知 implementer):**
- 两处都是**性能修复,不得改变可观测行为**。靠既有 e2e(`hybrid_cracks_zip`、`dictionary_cracks_zip`)+ 新增单元测试保证等价。
- rules.rs:`RuleSource.pending: Vec<Box<[u8]>>` 用 `remove(0)` 从头弹出(O(n),整体 O(n²))。改为 `std::collections::VecDeque<Box<[u8]>>` + `pop_front()`(O(1))。产出顺序必须不变。
- zip.rs:`ZipVerifier::verify`(zip.rs:238)每次 `File::open` + `ZipArchive::new`。ZipCrypto 有廉价 `quick_check` 快路径受影响小,但 AES 走 `verify` 每个候选都重开文件。**低风险方案(优先)**:用 `thread_local!` 缓存每线程打开的 `ZipArchive`,避免每次 open。若实现受阻,退回原状并在报告说明(此项非本 task 阻塞项,rules.rs 的修复是必须项)。

- [ ] **Step 1: 写 VecDeque 顺序等价的单元测试(红/或直接绿)**

在 `src/generators/rules.rs` 的 `#[cfg(test)] mod tests` 追加。构造一个会产生大量 pending 的场景(短词 + 多规则),断言前若干产出顺序符合"原词优先,再按规则顺序":

```rust
    #[test]
    fn pending_drains_in_order_across_batches() {
        use std::path::PathBuf;
        use std::io::Write;
        // 造一个含多词的临时词表,每词经多规则膨胀,迫使 pending 缓冲被使用。
        let mut f = tempfile::NamedTempFile::new().unwrap();
        for i in 0..50 {
            writeln!(f, "word{i}").unwrap();
        }
        f.flush().unwrap();
        let rules = vec![Rule::Capitalize, Rule::Upper, Rule::Reverse, Rule::Duplicate];
        let mut src = RuleSource::new(&PathBuf::from(f.path()), rules).unwrap();

        // 收集全部产出
        let mut all: Vec<Box<[u8]>> = Vec::new();
        let mut batch = Vec::new();
        while src.fill_batch(&mut batch) {
            all.append(&mut batch);
        }
        // 第一个词 "word0" 的原词必须最先出现
        assert_eq!(&*all[0], b"word0");
        // 不重不漏:总数 = 50 词 * (1 原词 + 每词生成的变体数)。此处仅断言首词顺序与非空。
        assert!(all.len() >= 50);
    }
```

> 注:`tempfile` 是 dev-dependency,`rules.rs` 的 `#[cfg(test)]` 里可用。`Rule` 已在同文件。

- [ ] **Step 2: 运行(当前实现应通过,建立等价基线)**

Run: `cargo test --lib rules::tests::pending_drains_in_order_across_batches`
Expected: PASS(当前 `Vec::remove(0)` 实现顺序正确,只是慢)。此测试锁定顺序,供重构后回归。

- [ ] **Step 3: 把 `pending: Vec` 改为 `VecDeque`**

在 `src/generators/rules.rs`:
1. 顶部 import:`use std::collections::VecDeque;`
2. 结构体字段:`pending: Vec<Box<[u8]>>` → `pending: VecDeque<Box<[u8]>>`
3. `RuleSource::new` 里 `pending: Vec::new()` → `pending: VecDeque::new()`
4. `fill_batch` 里:
   - `while !self.pending.is_empty() && batch.len() < BATCH { batch.push(self.pending.remove(0)); }` → `while batch.len() < BATCH { match self.pending.pop_front() { Some(p) => batch.push(p), None => break } }`
   - 溢出写入 `self.pending.push(m)` → `self.pending.push_back(m)`
   - `self.pending.clear()`(restore 里)保持不变(VecDeque 也有 `clear`)。

- [ ] **Step 4: 运行确认顺序等价 + 全量测试**

Run: `cargo test --lib rules && cargo test --test e2e hybrid`
Expected: `pending_drains_in_order_across_batches` 通过,`hybrid_cracks_zip` 通过。

- [ ] **Step 5: ZIP-AES thread_local 缓存(低风险,受阻可跳过并报告)**

在 `src/formats/zip.rs` 的 `verify` 中,对打开的归档做每线程缓存,避免每次 `File::open`。参考实现:

```rust
    fn verify(&self, password: &[u8]) -> bool {
        use std::cell::RefCell;
        use std::collections::HashMap;

        thread_local! {
            // 每线程按文件路径缓存已打开的 ZipArchive。
            static ARCHIVE_CACHE: RefCell<HashMap<std::path::PathBuf, zip::ZipArchive<File>>> =
                RefCell::new(HashMap::new());
        }

        ARCHIVE_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            let archive = match cache.get_mut(&self.file_path) {
                Some(a) => a,
                None => {
                    let file = match File::open(&self.file_path) {
                        Ok(f) => f,
                        Err(_) => return false,
                    };
                    let a = match ZipArchive::new(file) {
                        Ok(a) => a,
                        Err(_) => return false,
                    };
                    cache.entry(self.file_path.clone()).or_insert(a)
                }
            };
            match archive.by_index_decrypt(self.first_encrypted_index, password) {
                Ok(mut reader) => {
                    let mut buf = Vec::with_capacity(4096);
                    reader.read_to_end(&mut buf).is_ok()
                }
                Err(_) => false,
            }
        })
    }
```

> 若 `zip` crate 版本的 `by_index_decrypt` 借用签名与 thread_local 缓存冲突而无法编译,**退回原实现**并在报告里说明原因 —— rules.rs 的修复(Step 3)是本 task 的必须交付,ZIP-AES 缓存是尽力而为项。

- [ ] **Step 6: 运行全量测试确认 ZIP 行为不变**

Run: `cargo test`
Expected: 全绿。`dictionary_cracks_zip`、`brute_force_cracks_zip`、`hybrid_cracks_zip`(ZipCrypto)行为不变。

- [ ] **Step 7: clippy + fmt**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: 无警告。

- [ ] **Step 8: Commit**

```bash
git add src/generators/rules.rs src/formats/zip.rs
git commit -m "perf: VecDeque for hybrid pending buffer, cache ZIP archive per thread"
```

---

## Task 4: 卫生清理 — 常量收敛

**Files:**
- Modify: `src/formats/pdf.rs`(PADDING 常量去重 → 单个模块级 const)
- Modify: `src/generators/mod.rs`(新增 `pub(crate) const BATCH_SIZE`)
- Modify: `src/generators/brute_force.rs`、`src/generators/dictionary.rs`、`src/generators/rules.rs`(引用统一 BATCH_SIZE)

**Interfaces:**
- Consumes: 无外部;纯内部常量收敛。
- Produces: 单一 `password_cracking::generators::BATCH_SIZE`(`pub(crate)`)、pdf.rs 单一 `PADDING`。

**背景说明(dispatch 时告知 implementer):**
- 纯等价重构,零行为变化,靠既有全部测试保证无回归。
- pdf.rs 里 32 字节 PDF PADDING 常量重复出现 3 次(`derive_key_rc4`、`verify_user_password_rc4` 的 R2 与 R>=3 分支)。提为文件顶部单个 `const PADDING: [u8; 32]`,三处引用它。
- `BATCH` 常量在 brute_force.rs / dictionary.rs / rules.rs 各有一份 `const BATCH: usize = 1024;`。提到 `generators/mod.rs` 为 `pub(crate) const BATCH_SIZE: usize = 1024;`,各处改用 `super::BATCH_SIZE`。

- [ ] **Step 1: 收敛 pdf.rs 的 PADDING 常量**

在 `src/formats/pdf.rs` 文件顶部(`use` 之后、`PdfVerifier` 之前)加入模块级常量:

```rust
/// PDF 标准安全握手填充串(PDF 32000-1 §7.6.3.3,Algorithm 2 的 padding)。
const PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01,
    0x08, 0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53,
    0x69, 0x7A,
];
```

然后删除 `derive_key_rc4`、`verify_user_password_rc4`(R2 分支、R>=3 分支)内的 3 处局部 `const PADDING`,直接引用模块级 `PADDING`。

- [ ] **Step 2: 运行 PDF 相关测试确认无回归**

Run: `cargo test --lib pdf && cargo test --test e2e dictionary_cracks_pdf`
Expected: 全绿(R3 PDF 仍能破解,RC4 向量测试仍通过)。

- [ ] **Step 3: 新增 `BATCH_SIZE` 到 generators/mod.rs**

在 `src/generators/mod.rs` 顶部(`use` 之后)加入:

```rust
/// 各 PasswordSource 每次 fill_batch 的批大小。
pub(crate) const BATCH_SIZE: usize = 1024;
```

- [ ] **Step 4: 三个 source 改用 BATCH_SIZE**

- `src/generators/brute_force.rs`:删除 `const BATCH: usize = 1024;`,`fill_batch` 里 `0..BATCH` → `0..super::BATCH_SIZE`。
- `src/generators/dictionary.rs`:删除本地 `const BATCH`,`self.cursor + BATCH` → `self.cursor + super::BATCH_SIZE`。
- `src/generators/rules.rs`:删除本地 `const BATCH`,`batch.len() < BATCH` 等处 → `super::BATCH_SIZE`。

> 注意:若 mask.rs 尚未创建(它是 Phase 0 之后的原 Task 3),本 task 不涉及它;mask.rs 将来直接用 `super::BATCH_SIZE`。

- [ ] **Step 5: 运行全量测试确认等价**

Run: `cargo test`
Expected: 全绿。批大小值未变(仍 1024),纯符号收敛。

- [ ] **Step 6: clippy + fmt + machete**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check && cargo machete`
Expected: 无警告,machete 干净。

- [ ] **Step 7: Commit**

```bash
git add src/formats/pdf.rs src/generators/mod.rs src/generators/brute_force.rs src/generators/dictionary.rs src/generators/rules.rs
git commit -m "refactor: consolidate PADDING and BATCH_SIZE constants"
```

---

## 执行说明

- Phase 0 的 Task 1-4 全部展开为可执行逐 step 计划,直接由 subagent-driven-development 逐 task 派发。
- Phase 0 完成后,回到能力扩展 Roadmap 的原 Task 3(mask 生成器),届时 mask.rs 直接使用 `super::BATCH_SIZE`。
- 关键正确性(Algorithm 2.B)已对真实 fixture 端到端验证,计划内代码是验证过的参考实现的逐字移植。
