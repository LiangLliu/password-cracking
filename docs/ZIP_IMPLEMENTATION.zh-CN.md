[English](ZIP_IMPLEMENTATION.md) | **中文**

# ZIP 内部实现

`src/formats/zip.rs` 为 ZIP 归档实现了 `PasswordVerifier`。它同时处理传统的 ZipCrypto 和 WinZip AES。

## 加密方式

### ZipCrypto

- 基于 CRC32 和 96 位密钥调度（三个 32 位寄存器）构建的流密码
- 所有 ZIP 工具都支持
- 密码学上较弱（存在已知明文攻击），因此验证可以做得非常快
- 实测吞吐量：在 10 核 M2 Pro 上为 **11.0M–11.5M 个密码/秒**

### AES（WinZip AE-1/AE-2）

- CTR 模式下的 AES-128/192/256，使用 PBKDF2-HMAC-SHA1 密钥派生和 HMAC-SHA1 认证
- 由 `zip` crate 处理（`aes-crypto` feature）
- 实测吞吐量：**约 15K 个密码/秒**，主要开销来自每个候选的 PBKDF2（1,000 次迭代）

验证器本身不实现 AES；它委托给 `zip`，专注于检测加密类型，并按 worker 线程缓存归档。

## ZipCrypto 密钥调度

三个 32 位寄存器被初始化为固定常量：

```text
key0 = 0x12345678
key1 = 0x23456789
key2 = 0x34567890
```

对每个密码字节：

```rust
fn update(&mut self, byte: u8, table: &[u32; 256]) {
    self.k0 = crc32_byte(self.k0, byte, table);
    self.k1 = self.k1.wrapping_add(self.k0 & 0xff)
              .wrapping_mul(134_775_813).wrapping_add(1);
    self.k2 = crc32_byte(self.k2, (self.k1 >> 24) as u8, table);
}
```

密钥流字节：

```rust
fn stream_byte(&self) -> u8 {
    let temp = (self.k2 | 2) & 0xffff;
    ((temp.wrapping_mul(temp ^ 1)) >> 8) as u8
}
```

解密：`plain = cipher ^ stream_byte()`，然后用明文字节更新密钥。

## 12 字节头检查

每个 ZipCrypto 条目都以 12 字节加密头开始。解密后，最后一个字节是校验字节：

- **标准模式**（标志位 3 清零）：条目 CRC32 的高字节
- **数据描述符模式**（标志位 3 置位）：DOS 修改时间的高字节

验证器直接读取本地文件头，以获取标志、CRC32、修改时间和 12 字节头。仅解密这 12 个字节就能拒绝约 99.6% 的错误密码，比完整解密快约 20 倍。

## 多条目归档

ZIP 允许每个条目使用不同的密码。验证器会收集**每一个**加密条目，并且只有当密码能解密所有这些条目时才接受该密码：

```rust
for &index in &self.encrypted_indices {
    let mut reader = archive.by_index_decrypt(index, password)?;
    reader.read_to_end(&mut Vec::new())?;
}
```

这覆盖了：

- 多个加密条目使用同一密码的归档（`test_multi.zip`）；
- 第一个条目未加密的归档（`test_mixed.zip`）；
- 加密条目不是索引 0 的归档。

如果 ZIP 确实为每个条目使用了不同密码，那么没有任何单个密码能满足验证器；这是有意为之的限制。

## 实现说明

### 检测

`ZipVerifier::new()` 扫描中央目录并记录每个加密条目。AES 检测使用 `0x9901` AES 扩展字段：

```rust
let is_aes = entry.compression() == zip::CompressionMethod::Aes
    || entry.extra_data_fields()
        .any(|f| matches!(f, zip::ExtraField::AeXEncryption(_)));
```

必须检查扩展字段，因为 `zip` crate 在解析 AES 头之后，会将 AES 条目的压缩方法改写为内层方法（例如 Deflate）。仅依赖 `compression() == Aes` 会把 AES 归档误判为 ZipCrypto，从而使 12 字节检查拒绝每一个候选。

### 快速检查与验证

- ZipCrypto：`quick_check` 解密 12 字节并比较校验字节；`verify` 解密所有加密条目并校验 CRC32。
- AES：`quick_check` 返回 `true`；`verify` 运行一次 PBKDF2/AES/HMAC。这避免了每个候选两次付出 AES 开销。

### 归档缓存

AES 验证会为每个候选重新打开归档。线程局部的 `HashMap<PathBuf, ZipArchive<File>>` 为每个 worker 线程缓存一个已打开的归档，避免重复解析中央目录。ZIP 归档在运行期间是不可变的。

## 创建测试归档

```bash
# ZipCrypto（Info-ZIP zip）
zip -e archive.zip files...          # 提示输入密码
zip -P 92eo archive.zip files...     # 在命令行中给出密码

# AES-256（7-Zip）
7z a -tzip -p92eo -mem=AES256 archive.zip files...

# 多个加密条目
zip -P 92eo multi.zip a.txt b.txt

# 混合：第一个条目未加密，第二个加密
zip mixed.zip a.txt
zip -P 92eo mixed.zip b.txt
```

## 测试

`examples/` 中的测试文件（密码均为 `92eo`）：

| 文件 | 检查内容 |
|------|----------------|
| `test.zip` | 单个 ZipCrypto 条目 |
| `test_aes.zip` | AES-256 检测 + 验证 |
| `test_multi.zip` | 两个加密条目 |
| `test_mixed.zip` | 第一个条目未加密 |

`tests/e2e.rs` 通过公共 API 破解其中每一个文件。

## 限制

- 不支持分卷和多卷归档。
- ZIP64 支持取决于 `zip` crate。
- 密码必须能解密每一个加密条目；每个条目使用不同密码时无法用单个候选恢复。
- AES 使用 `zip` crate 的实现；支持 AE-1 和 AE-2，但验证器不会在 `encryption_info()` 中区分两者。
