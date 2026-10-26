[English](PERFORMANCE.md) | **中文**

# 性能指南

在 Apple M2 Pro（10 核 CPU）上测量，Rust 1.97，使用 `opt-level=3`、`lto="thin"`、`codegen-units=1` 的发布构建。请将这些数字视为数量级参考；加密参数（尤其是 Office 的 `spinCount`）和 CPU 代际都会改变结果。

## 按格式和加密方式

| 格式 | 加密方式 | 速度 | 瓶颈 |
|--------|-----------|-------|------------|
| ZIP | ZipCrypto | 11.0M–11.5M 密码/秒 | CRC32 + 带 12 字节头过滤的流密码 |
| ZIP | AES-256 | 约 15K 密码/秒 | 每个候选的 PBKDF2 + AES + HMAC（由 `zip` 处理） |
| PDF | RC4-128 | 510K–540K 密码/秒 | MD5 密钥派生 + 20 轮 RC4 |
| PDF | AES-128 | 约 515K 密码/秒 | MD5 密钥派生 + 20 轮 RC4（R4 U 校验） |
| PDF | AES-256 R6 | 约 7.6K 密码/秒 | Algorithm 2.B 哈希迭代 |
| PDF（owner） | RC4-128 | 约 150K 密码/秒 | user 密码校验 + owner 密码恢复（`--pdf-owner`） |
| Office | Agile AES-256 | 500–800 密码/秒（实测 585） | 每个候选约 10 万次 SHA-512 迭代 |

`--pdf-owner` 会使 PDF 的开销大约增至三倍，因为每个候选都要同时执行 user 密码和 owner 密码校验。

## 按攻击模式（ZIP ZipCrypto）

| 攻击方式 | 候选数 | 时间 | 吞吐量 |
|--------|-----------|------|------------|
| 字典，33 个词 | 33 | <0.1 秒 | 受启动开销主导 |
| 暴力破解，数字 1–6 | 1,111,110 | 0.1 秒 | 10.6M/秒 |
| 暴力破解，字母数字 1–4 | 15,018,570 | 1.4 秒 | 11.3M/秒 |
| Mask `?d?d?l?l` | 67,600 | 0.01 秒 | 约 10M/秒 |
| 混合攻击，33 个词 + 2 条规则 | 99 | <0.1 秒 | 受启动开销主导 |
| 混合攻击，33 个词 + `--append-digits 99` | 3,366 | 0.2 秒 | 22K/秒 |

短时间运行主要受进程启动、线程创建和字典映射支配，而非验证本身。

## 密钥空间与时间

定长密钥空间（`C^L`）。时间均为近似值，并假设理想线性扩展。

### ZIP ZipCrypto（11M/秒）

| 字符集 | 长度 4 | 长度 6 | 长度 8 |
|---------|----------|----------|----------|
| 数字（10） | 10,000 | 1,000,000 | 100,000,000 |
| 小写字母（26） | 456,976 | 308,915,776 | 208,827,064,576 |
| 字母数字（62） | 14,776,336 | 56,800,235,584 | 218,340,105,584,896 |
| 全部（93） | 74,805,201 | 646,990,183,449 | 5,595,818,096,650,401 |

| 时间 | 长度 4 | 长度 6 | 长度 8 |
|------|----------|----------|----------|
| 数字 | 瞬间 | 瞬间 | 9 秒 |
| 小写字母 | 瞬间 | 28 秒 | 5.3 小时 |
| 字母数字 | 1.3 秒 | 1.4 小时 | 230 天 |
| 全部 | 7 秒 | 16 小时 | 16 年 |

### PDF RC4-128（530K/秒）

| 时间 | 长度 4 | 长度 6 | 长度 8 |
|------|----------|----------|----------|
| 数字 | 瞬间 | 2 秒 | 3.1 分钟 |
| 小写字母 | 0.9 秒 | 10 分钟 | 4.5 天 |
| 字母数字 | 28 秒 | 30 小时 | 13 年 |
| 全部 | 2.4 分钟 | 14 天 | 335 年 |

### Office Agile AES-256（600/秒）

| 时间 | 长度 4 | 长度 6 | 长度 8 |
|------|----------|----------|----------|
| 数字 | 17 秒 | 28 分钟 | 1.9 天 |
| 小写字母 | 13 分钟 | 6 天 | 11 年 |
| 字母数字 | 6.8 小时 | 3 年 | 11,500 年 |

> Office 比 ZIP ZipCrypto 慢约 18,000 倍。对于现实中的密码，字典攻击和混合攻击是仅有的实用选项。

## 引擎设计

- **常驻 FIFO worker 线程。** 批次按顺序从有界通道中取出。这使检查点前沿保持有意义；LIFO 调度器（例如 Rayon 的工作窃取队列）会导致早期批次尚未完成而后续批次已完成。
- **自适应批次划分。** 共享目标值从 32 开始，每批次后翻倍或减半，目标约为 250 ms，上限为 4096。慢速格式使用小批次以及时写入检查点；快速格式使用大批次以摊销通道同步开销。
- **流式数据源。** 字典候选通过内存映射并按字节偏移读取。无论字典大小如何，内存占用保持平稳。
- **有界通道。** 容量等于 worker 线程数，因此生成器不会大幅超前于验证。

## 两阶段验证

`PasswordVerifier` 暴露 `quick_check` 和 `verify`。引擎先调用 `quick_check`，仅当其返回 true 时才调用 `verify`。

| 格式 | `quick_check` | `verify` | 实际收益 |
|--------|---------------|----------|----------------|
| ZIP ZipCrypto | 解密 12 字节头，比较一个字节 | 解密所有加密条目，校验 CRC | 约 20× |
| ZIP AES | `true` | AES/HMAC 校验 | 无 |
| PDF | `true` | 密码哈希校验 | 无 |
| Office | `true` | 10 万次迭代哈希 + AES | 无 |

对于 ZIP ZipCrypto，头部检查只需 12 字节的工作量即可拒绝约 99.6% 的错误密码。对于没有更廉价过滤器的格式，让 `quick_check` 返回 `true` 可避免重复支付验证开销。

## 优化策略

1. **优先使用字典。** 对每种格式而言，这实际上都是瞬时完成的。
2. **使用尽可能小的字符集。** 每个位置上，`all`（93）比 `digits`（10）大 9.3 倍。
3. **限定长度范围。** 每增加一个位置，候选数就乘以字符集大小。
4. **已知口令形态时使用 Mask 攻击。** `?d?d?l?l` 有 67,600 个候选；`?a?a?a?a` 有 7,480 万个。
5. **用满所有核心。** 默认使用所有逻辑 CPU。仅在共享机器上通过 `-t` 限制。
6. **除非确有需要，否则不要在超大字典上使用 `--dedup`。** 它会为每个唯一候选存储一个哈希；默认流式模式使用常量内存。

## 构建配置

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
panic = "abort"
strip = true
```

- `opt-level = 3` — 针对热点哈希/加密循环的最大优化。
- `lto = "thin"` — 跨 crate 内联，同时避免完整 LTO 的编译时间。
- `codegen-units = 1` — 更好的优化，构建更慢。
- `panic = "abort"` — 二进制更小，且不含栈展开表。
- `strip = true` — 发布二进制中不包含调试符号。

Linux 软件包由静态 musl 二进制构建，后者具有相同的优化配置。

## 复现基准测试

```bash
cargo build --release
BIN=./target/release/password-cracking

# ZIP ZipCrypto, alnum 1-4 (15M candidates)
time "$BIN" -q -f examples/test.zip brute-force -c alnum --min-length 1 --max-length 4

# PDF RC4-128, digits 1-6 (1.1M candidates)
time "$BIN" -q -f examples/test.pdf brute-force -c digits --min-length 1 --max-length 6

# Office Agile AES-256, digits 1-3 (1,110 candidates)
time "$BIN" -q -f examples/test.docx brute-force -c digits --min-length 1 --max-length 3

# Mask
time "$BIN" -q -f examples/test.zip mask -m "?d?d?l?l"
```

请使用发布构建。调试构建会慢数倍，不具代表性。

## 性能分析

在 Linux 上，`perf` 表现良好：

```bash
perf record -g ./target/release/password-cracking ...
perf report
```

在 macOS 上，可以使用 Instruments（Time Profiler）或 `cargo flamegraph`。热点路径包括：

- ZIP：`ZipCrypto::update`、`crc32_byte`、`quick_check`
- PDF：MD5/RC4 密钥派生和 `algorithm_2b`
- Office：`derive_hash`（10 万次迭代循环）、AES-CBC

对这些函数的修改应在修改前后分别做基准测试；微小的性能回退会在数百万个候选上累积放大。
