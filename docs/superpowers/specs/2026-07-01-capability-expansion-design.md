# 能力扩展 Roadmap 设计文档

- **日期**: 2026-07-01
- **分支**: `feature/capability-expansion`
- **状态**: 设计确认中

## 第 0 节:总览与设计原则

`password-cracking` 是一个纯 Rust、零 C 依赖、三平台 CI 全绿的文档密码破解器,约 2200 行。本文档规划四个阶段的能力扩展,组织为一份 spec 内的四个 Phase,严格按依赖顺序执行。

### 不可动摇的全局约束(Global Constraints)

以下约束在四个 Phase 里全程有效,任何实现都不得违反:

1. **纯 Rust,零 C 依赖**。不引入任何需要 C 工具链或系统库的 crate。
2. **三平台 CI 全绿**(Linux / macOS / Windows)。每个 Phase 结束时 `cargo fmt --check`、`cargo clippy`、`cargo test`、`cargo audit` 全部通过。
3. **两个核心 trait 的抽象边界不变**:
   - `PasswordVerifier`(格式端):`quick_check` + `verify` 两阶段验证
   - `PasswordSource`(生成端):`fill_batch` 流式产出,永不将整个搜索空间放入内存
   所有新能力都作为这两个 trait 的**新实现**接入,**不改引擎**(`engine/mod.rs`)。
4. **每个 Phase 结束时都是可发布、测试全绿的状态**。Phase 之间严格按依赖顺序,后一个 Phase 依赖前一个建立的基础设施。

### 四个 Phase 与依赖顺序

| Phase | 内容 | 依赖 | 风险 |
|-------|------|------|------|
| 1 | 端到端集成测试 + 依赖清理 | 无(安全网,先做) | 低 |
| 2 | 断点续跑 + 字典流式化 | Phase 1(回归保护) | 中 |
| 3 | Mask 攻击模式 | Phase 1 | 低 |
| 4 | 扩展格式:老 Office RC4 / RAR5 / 7z(仅验证) | Phase 1 | 高 |

---

## 第 1 节:Phase 1 — 端到端集成测试 + 依赖清理

### 目的

`examples/` 里已有 `test.zip / test.pdf / test.docx / test.xlsx / test.pptx`(密码均为 `92eo`),但现有 12 个测试全是纯单元测试,**没有真正端到端破解这些文件的测试**。在动引擎和生成器之前,先建立端到端回归保护网。

### 已验证基线(2026-07-01 实测)

在 `feature/capability-expansion` 分支上,release 构建对五个 example 文件跑字典攻击,全部破出 `92eo`。这是集成测试要锁定的行为。

### 组件

#### 1.1 集成测试文件 `tests/e2e.rs`

新建 crate 级别集成测试(`tests/` 目录),通过公开 API(`lib.rs` 导出的 `CrackerEngine` / `create_source` / `create_verifier` 等)驱动完整破解流程,断言破出 `92eo`。

- 对 5 个 example 文件各写一个字典攻击测试(词表含 `92eo`)。
- 对至少一个文件(如 `test.zip`)写一个 brute-force 测试(`alnum`,长度 1-4,能命中 `92eo`)。
- 对至少一个文件写一个 hybrid 测试。
- 写一个"密码不在词表中 → 返回 None"的负向测试,确认不会误报。
- 测试用的临时词表通过 `tempfile`(已是 dev-dependency)在测试内生成,不新增仓库文件。

**约束**:集成测试必须通过 `lib.rs` 公开接口调用,不得依赖 `target/release` 二进制或 shell。测试要在 `cargo test` 下跨三平台稳定运行。

#### 1.2 依赖清理

pre-commit 的 cargo-machete 报告了一批"未使用依赖"。需逐一核实(machete 有误报可能,尤其是通过 re-export 或 feature 间接使用的 crate),对**确实未使用**的移除,对**误报**的加入 `[package.metadata.cargo-machete] ignored = [...]`。

- 重点:`memmap2` 声明用途是"large wordlists",但 `dictionary.rs` 实际用 `BufReader` 全量读入 `Vec`,并未使用 memmap2。**Phase 2 会真正用到 memmap2**,因此本 Phase **不移除 memmap2**,而是先加入 machete 的 ignored 列表(附注释说明 Phase 2 会启用),避免 pre-commit 噪音。
- 其余 machete 报告项(如 `thiserror`、`humansize`、`crc32fast`、`cbc`、`ecb`、`hmac`、`md-5`、`pbkdf2`、`rc4`)逐一核实:凡是被格式验证器实际用到的(如 `md-5`/`rc4`/`sha1` 在 pdf.rs、`cbc` 在 office.rs)一律保留,machete 误报的加 ignored;真正无引用的才移除。
- **不允许**为了让 machete 闭嘴而移除实际被条件编译或宏使用的依赖。每一项决定都要有依据(grep 到引用 = 保留)。

### 验收标准

- `cargo test` 通过,新增集成测试全绿。
- `cargo clippy` / `cargo fmt --check` / `cargo audit` 全绿。
- cargo-machete 无误报噪音(要么移除,要么 ignored 附理由)。
- 不改动 `engine/`、不改动格式验证器逻辑。

---

## 第 2 节:Phase 2 — 断点续跑 + 字典流式化

### 目的

两件相关的事:

1. **断点续跑**:`PasswordSource` trait 已实现 `checkpoint()` / `restore()`,三个 source 都有实现,但 CLI 完全没暴露。长时间 brute-force 一旦中断只能从头再来。
2. **字典流式化**:`DictionarySource` 违反了 trait 文档"永不将整个搜索空间放入内存"的约定 —— 它用 `BufReader` 把整个词表读入 `Vec<Box<[u8]>>`。超大词表会吃满内存。用已声明的 `memmap2` 改造成真正流式。

### 组件

#### 2.1 CLI 断点续跑开关

在 `Cli` 增加两个全局选项:

- `--checkpoint <PATH>`:定期将 source 的 `checkpoint()` 写入该文件(建议每 N 秒或每 M 个 batch 落盘一次)。
- `--resume`:启动时若 checkpoint 文件存在,读取并 `restore()` 到 source。

引擎侧需要一个机制在破解过程中周期性调用 `source.checkpoint()` 并落盘。**注意**:当前 `CrackerEngine::crack` 把 `source` move 进生成线程后无法从外部访问。设计上倾向于:生成线程在每次 `fill_batch` 后、按时间/批次间隔把 checkpoint 写入一个 `Arc<Mutex<Option<String>>>`,主线程或一个独立线程负责落盘。**此改动触及 engine,属于对全局约束 3「不改引擎」的例外**,需在实现前与人确认改动范围最小化(仅新增 checkpoint 落盘路径,不改两阶段验证与并发结构)。

> ⚠️ **待人确认(Phase 2 启动前)**:断点续跑必须触碰 engine,与全局约束 3 冲突。可选方案:(a) 引擎新增可选 checkpoint 回调;(b) 生成器自持久化。实现前需敲定,不在本文档预判。

#### 2.2 字典流式化

将 `DictionarySource` 从"全量读入 Vec"改为基于 `memmap2` 的流式读取:

- 用 `memmap2::Mmap` 映射词表文件,按行切分产出 candidate,`fill_batch` 时推进偏移量而非索引 `Vec`。
- `checkpoint()` 返回字节偏移量,`restore()` 从偏移量恢复。
- 保留现有行为:跳过空行和 `#` 注释行、trim。
- **去重取舍**:当前实现全量去重(HashSet)。流式化后无法零成本全量去重。设计倾向于**放弃跨文件全量去重**(去重需要全量在内存,与流式化目标矛盾),或提供一个可选的 `--dedup` 开关(默认关闭)。此取舍需在实现前确认。
- 目录模式(多个 .txt)如何流式化:按文件顺序依次 mmap,checkpoint 记录 (文件索引, 字节偏移)。

> ⚠️ **待人确认(Phase 2 启动前)**:去重行为变更(全量去重 → 放弃或可选)是用户可见的行为改变,需确认。

### 验收标准

- 长时间 brute-force 可用 `--checkpoint` 落盘、`--resume` 恢复,恢复后不重复已扫描的空间(brute-force 的 checkpoint 已精确)。
- 字典源不再全量进内存;超大词表内存占用平稳。
- Phase 1 的集成测试仍全绿(回归保护生效)。
- 移除 memmap2 的 machete ignored(此时已真正使用)。

---

## 第 3 节:Phase 3 — Mask 攻击模式

### 目的

当前 brute-force 是全位置统一字符集。Mask 攻击(hashcat 式)允许逐位指定不同字符集,针对性强得多。例如 `?u?l?l?l?d?d`(首字母大写 + 3 小写 + 2 数字)。

### 组件

#### 3.1 `MaskSource`(新的 `PasswordSource` 实现)

新增 `generators/mask.rs`,实现 `PasswordSource`:

- 解析 mask 字符串:`?d`=数字,`?l`=小写,`?u`=大写,`?s`=特殊,`?a`=全部,`?b`=所有字节 0x00-0xFF(可选);字面字符直接匹配;`??` 表示字面 `?`。
- 每个位置独立字符集,笛卡尔积流式产出(类似 brute-force 的进位逻辑,但每位基数不同)。
- `estimated_total` = 各位置字符集大小的乘积。
- `checkpoint`/`restore`:各位置索引序列化(与 brute-force 类似)。
- 支持自定义字符集 `-1 <charset>` … 可作为 YAGNI 候选,首版可只支持内置类。

#### 3.2 CLI 子命令 `mask`

在 `AttackMode` 增加 `Mask { mask: String }` 子命令,`build_generator_mode` 增加对应分支,`GeneratorMode` 增加 `Mask` 变体,`create_source` 增加分支。

### 验收标准

- `mask` 子命令能破解 example 文件(如 `?d?d?l?l` 之外构造一个能命中 `92eo` 的 mask:`92eo` = 数字数字小写小写 → `?d?d?l?l`)。
- 新增 mask 的单元测试(序列、总数)+ 至少一个集成测试。
- 完全不碰 engine 和现有 source。

---

## 第 4 节:Phase 4 — 扩展格式(仅验证)

### 目的

扩展格式覆盖:老 Office RC4(CryptoAPI)、RAR5、7z。**核心决策:纯 Rust、仅验证(verify-only)**,即只判断密码是否正确(通过密钥派生 + 校验值比对),不做完整解压。这保住了纯 Rust / 零 C 依赖 / 三平台的核心属性。

### 技术可行性(已核对,均落在现有依赖内)

| 格式 | 密钥派生 | 现有依赖 | 需新增 |
|------|---------|---------|--------|
| 老 Office RC4 (CryptoAPI) | SHA-1 + RC4 | `sha1` + `rc4` | 仅容器/header 解析 |
| RAR5 | PBKDF2-HMAC-SHA256 | `pbkdf2` + `hmac` + `sha2` | RAR5 header 解析 |
| 7z | SHA-256 迭代 + AES-256-CBC | `sha2` + `aes` + `cbc` | 7z signature header 解析 |

均**不需要**引入重型 crypto 依赖,只需各自的容器 header 解析逻辑。

### 组件

每个格式一个新的 `PasswordVerifier` 实现,与现有 `zip.rs / pdf.rs / office.rs` 模式严格一致:

#### 4.1 老 Office RC4 (`formats/office.rs` 扩展 或新模块)

- 当前 `office.rs` 只支持 Agile(version=4),遇到老 RC4(version=2/3,CryptoAPI)直接 bail。
- 扩展:识别 EncryptionInfo 的 RC4 CryptoAPI 版本,实现 SHA-1 密钥派生 + RC4 验证器比对。

#### 4.2 RAR5 (`formats/rar.rs`)

- magic bytes:`Rar!\x1a\x07\x01\x00`(RAR5)。RAR4(`Rar!\x1a\x07\x00`)可选择只识别不支持(bail 提示)。
- 解析 RAR5 加密 header,取出 PBKDF2 参数(salt、迭代次数)+ 校验值,派生密钥并比对。
- `formats/mod.rs` 的 `detect_format` 增加 RAR 分支。

#### 4.3 7z (`formats/sevenzip.rs`)

- magic bytes:`7z\xbc\xaf\x27\x1c`。
- 解析 7z header,取出 AES-256 的 salt/迭代参数,SHA-256 迭代派生密钥,尝试解密并校验。
- 7z 的 header 本身可能被加密,verify-only 的校验点需要确认(可能需解密 header 的 CRC 校验)。

> ⚠️ **待人确认/调研(Phase 4 启动前)**:7z 的 verify-only 校验点比 RAR5 复杂(header 可加密)。RAR4 是否支持。这些在 Phase 4 实现前需要专门调研确认,不在本文档细化。

### 验收标准

- 每个新格式有一个加密测试文件(需在 Phase 4 生成,加入 `examples/`)+ 集成测试破出已知密码。
- `detect_format` 正确按 magic bytes 识别。
- 三平台 CI 全绿,无新增 C 依赖。
- 与现有验证器模式一致(quick_check/verify、format_name、encryption_info)。

---

## 附录:执行方式

采用 subagent-driven-development:每个 Phase 拆成若干 task,逐 task 派发 implementer subagent(TDD)+ task reviewer(spec 合规 + 代码质量),Phase 内 task 串行,全部完成后做一次 whole-branch review。

标注 ⚠️ 的待确认项在对应 Phase **启动前**与人敲定,不在本设计文档预判。
