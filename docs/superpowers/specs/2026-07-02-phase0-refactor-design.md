# Phase 0 重构设计文档 — 正确性修复 + 热路径性能 + 卫生清理

- **日期**: 2026-07-02
- **分支**: `feature/capability-expansion`
- **状态**: 已批准(用户指示自主执行,不逐节确认)
- **前置**: Phase 1(端到端测试)已完成并提供回归保护网

## 第 0 节:背景与定位

这是折入既有「能力扩展 Roadmap」的一个**前置** Phase(Phase 0),在 Task 3(mask)与 Phase 4(格式扩展)之前执行。理由:Phase 4 会新增三个 `PasswordVerifier` 实现,应建立在**已修正、已清理**的格式代码之上,而非叠在现存的 PDF R6 缺陷与热路径性能问题上。

本 Phase **不是全量重写**。经通读全部 ~2200 行源码,项目架构(两个 trait 边界 + 解耦引擎)是健康的,全量重写会摧毁已通过 CI、已被 e2e 覆盖的价值。Phase 0 只做**证据支持的、有针对性的**修复。

### 全局约束(沿用 Roadmap,全程有效)

1. **纯 Rust,零 C 依赖**。R6 修复所需的 AES-128-CBC 用已有的 `aes` + `cipher` crate 实现。
2. **三平台 CI 全绿**。每个 task 结束 `cargo fmt --check` / `cargo clippy -D warnings` / `cargo test` / `cargo audit` 全过。
3. **两个核心 trait 边界不变**。所有改动是既有实现的内部修正,不改 trait 签名,不改 `engine/mod.rs`。
4. **例子文件密码均为 `92eo`**。新增 R6 fixture `examples/test_r6.pdf` 同样是 `92eo`。

---

## 第 1 节:PDF R6 正确性修复(本 Phase 核心)

### 缺陷(已实测确认)

`src/formats/pdf.rs` 的**在用**路径 `verify_user_password_aes256`(约 pdf.rs:196)对 R5 与 R6 一律只算 `SHA256(password ‖ validation_salt)`。这只对 **R5** 正确。**R6**(现代 Acrobat / ISO 32000-2)要求 **Algorithm 2.B 硬化哈希**:以 SHA-256 起手,随后 ≥64 轮、每轮用 AES-128-CBC 加密并按中间结果 mod 3 在 SHA-256/384/512 间切换,直到满足终止条件。

那段**试图**做 R6 的 `derive_key_aes256`(约 pdf.rs:118)逻辑错误(用了臆造的 `count_ones` + 提前 `break`),且挂着 `#[allow(dead_code)]` 从未被调用 —— 死代码**掩盖**了这个缺口。

**实测证据(2026-07-02)**:用 pikepdf 生成 `examples/test_r6.pdf`(V=5 R=6 AES-256,密码 `92eo`)。当前工具对含 `92eo` 的词表返回 `not found`;而对 R3 的 `test.pdf` 正确返回 `92eo`。缺陷真实存在,非理论推断。

### 修复方案

实现 ISO 32000-2 **Algorithm 2.B**,替换 R6 的验证路径:

- 输入:`password`(≤127 UTF-8 字节)、U 项的 validation salt(U[32..40])。
- 步骤:
  1. `K = SHA-256(password ‖ salt)`(32 字节)。
  2. 循环 `round = 0, 1, 2, ...`:
     - `K1 = (password ‖ K) 重复 64 次` 拼接。
     - `E = AES-128-CBC-Encrypt(key = K[0..16], iv = K[16..32], data = K1)`,**无填充**。
     - `mod = (E 的前 16 字节之和) mod 3`。
     - `K = SHA-{256|384|512}(E)`,依 `mod` 选 0→256 / 1→384 / 2→512。
     - 终止条件:当 `round >= 63` 且 `E 的最后一字节 <= round - 32` 时停止。
  3. 结果取 `K[0..32]`,与 U 项前 32 字节比对。
- R5 路径保持 `SHA256(password ‖ salt)` 不变(现状对 R5 正确)。

将 R5/R6 分派收敛到一处清晰的 `verify_user_password_aes256`,R6 走 Algorithm 2.B,R5 走单轮 SHA-256。删除或重构现存那段错误的 `derive_key_aes256`。

### 验收

- `examples/test_r6.pdf` 能被破出 `92eo`(新增 e2e 测试)。
- 原 R3 `test.pdf`、其余四格式的 e2e 测试仍全绿(无回归)。
- 至少一个 Algorithm 2.B 的已知答案单元测试(用 fixture 的 salt/U 做端到端断言即可,不需外部向量)。

---

## 第 2 节:清除 `#[allow(dead_code)]`

现有 4 处 `#[allow(dead_code)]`(office.rs:24 的 `block_size`;pdf.rs:29 的 `o_validation_salt`;pdf.rs:41 的 `derive_key`;pdf.rs:107 的 `derive_key_aes256`)。逐一处理:

- `derive_key_aes256`(pdf.rs:107):被第 1 节修复吸收(重构为正确的 R6 实现并被调用),`allow` 移除。
- `derive_key`(pdf.rs:41):若修复后仍无调用者,删除该死方法;若能自然用于收敛派发,则接上。以「不留未调用的私有方法」为准。
- `o_validation_salt`(pdf.rs:29)、`block_size`(office.rs:24):字段若确无读取者则删字段;若属未来 Phase 4 需要则保留但须有明确注释。判定:grep 无读取 = 删。
- **原则**:Phase 0 结束后 `src/` 内**不得残留** `#[allow(dead_code)]`。要么用起来,要么删掉。

---

## 第 3 节:热路径性能修复

### 3.1 hybrid 的 O(n²) drain(rules.rs:175)

`RuleSource::fill_batch` 用 `self.pending.remove(0)` 从 `Vec` 头部逐个弹出 —— 每次 O(n),整体 O(n²)。改为 `VecDeque<Box<[u8]>>` + `pop_front()`(O(1)),或用游标索引推进。保持产出顺序与去重语义不变。

### 3.2 ZIP-AES 每次尝试重开文件(zip.rs:238)

`ZipVerifier::verify` 每次调用都 `File::open` + `ZipArchive::new` 重新解析整个归档。对 AES ZIP(无廉价 `quick_check`,每个候选都走 `verify`)这是严重吞吐损耗。

修复取舍:`ZipArchive` 内部持有文件句柄且 `by_index_decrypt` 需要 `&mut self`,跨 rayon 线程共享需 `Send + Sync`。方案:把「AES 验证所需的最小状态」(加密条目的原始压缩数据 + AES 参数)在 `new()` 时读入内存一次,`verify` 时仅做密钥派生 + 认证,不再触碰文件系统。若该重构复杂度过高,退一步:每线程用 `thread_local` 缓存打开的 `ZipArchive`,避免每次 `open`。实现时以「不改 trait、不改 engine、ZipCrypto 快路径行为不变」为硬约束,择低风险者。

### 验收

- hybrid e2e 测试仍绿;新增一个大 `pending` 的单元测试确认顺序不变。
- ZIP-AES 若有可用 fixture 则加 e2e;ZipCrypto 的 `test.zip` 行为与速度不退化。
- 两项均不得改变 `PasswordVerifier` / `PasswordSource` trait 与 engine。

---

## 第 4 节:卫生清理(低风险,顺手)

- **PADDING 常量**:pdf.rs 中 32 字节 PDF padding 重复 3 次,提为单个模块级 `const PADDING: [u8; 32]`。
- **BATCH 常量**:`brute_force.rs` / `dictionary.rs` / `rules.rs`(及新增的 mask.rs)各自 `const BATCH: usize = 1024;`。提到 `generators/mod.rs` 为单一 `pub(crate) const BATCH_SIZE: usize = 1024;` 并各处引用。
- 纯等价重构,不改行为;靠既有测试 + e2e 保证无回归。

---

## 第 5 节:任务分解与执行

折入既有 plan,作为 Phase 0,在 Task 3 之前执行。建议 task 切分:

- **Task 0a**:新增 `examples/test_r6.pdf` fixture + 一个「当前会失败」的 R6 e2e 测试(红)。
- **Task 0b**:实现 Algorithm 2.B,修 R6,测试转绿;清除相关 dead_code(第 1、2 节)。
- **Task 0c**:热路径性能修复(第 3 节:VecDeque + ZIP-AES 状态缓存)。
- **Task 0d**:卫生清理(第 4 节:PADDING / BATCH 常量收敛)。

每个 task 走标准 SDD:implementer(TDD)+ task reviewer(spec 合规 + 质量),串行,Phase 0 完成后继续 Task 3。

### fixture 生成说明

`examples/test_r6.pdf` 已由 pikepdf 生成(纯 wheel,内含 qpdf,无需 C 工具链,不进入项目运行时依赖)。密码 `92eo`,V=5 R=6 AES-256。生成脚本片段记录在 `examples/README.md`(Phase 0 更新)。
