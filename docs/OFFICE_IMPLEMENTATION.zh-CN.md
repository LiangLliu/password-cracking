[English](OFFICE_IMPLEMENTATION.md) | **中文**

# Office 内部实现

`src/formats/office.rs` 验证 Agile 加密的 Office 文件（Office 2010+）的密码。不支持旧式 CryptoAPI/ECMA-376 标准加密。

## 容器

| 容器 | 扩展名 | 加密方式 |
|-----------|------------|------------|
| OLE2 (CFB) | `.doc`, `.xls`, `.ppt`（旧式） | Agile 或旧式 |
| OOXML (ZIP) | `.docx`, `.xlsx`, `.pptx` (2007+) | Agile，加密时封装在 OLE2 中 |

加密的 OOXML 文件是一个 OLE2 复合文件，包含：

- `EncryptionInfo` — 版本头加上 Agile XML
- `EncryptedPackage` — 加密后的 OOXML ZIP

验证器只需要 `EncryptionInfo`；它从不解密该包。

## Agile 加密参数

XML 位于 `EncryptionInfo` 流的第 8 个字节处：

```text
bytes 0-1: 版本（4 = Agile）
bytes 2-3: 保留
bytes 4-7: 保留/标志
bytes 8+ : UTF-8 XML（p:encryption）
```

`p:encryptedKey` 中的相关属性：

| 属性 | 含义 |
|-----------|---------|
| `spinCount` | 哈希迭代次数（通常为 100,000） |
| `keyBits` | AES 密钥长度：128/192/256 |
| `hashSize` | 哈希输出大小（字节） |
| `blockSize` | AES 块大小（必须为 16） |
| `hashAlgorithm` | SHA-1/256/384/512 |
| `saltValue` | base64 盐 |
| `encryptedVerifierHashInput` | base64 编码的加密验证器输入 |
| `encryptedVerifierHashValue` | base64 编码的加密验证器哈希 |

`spinCount` 上限为 5,000,000。真实文件使用 100,000；损坏或恶意文件如果请求数十亿次迭代，会被拒绝，而不是让破解程序挂起。

## 密码验证

参考：[MS-OFFCRYPTO 2.3.7.1](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-offcrypto/)。

### 1. 迭代哈希

```text
H = hash(salt || password_utf16le)
for i in 0..spinCount:
    H = hash(i_as_4_bytes_le || H)
```

顺序很重要：先盐，后密码；先计数器，后上一次哈希。颠倒任何一处都会得到一个看起来有效但永远无法匹配的实现。

### 2. 按用途派生的密钥

```text
key1 = hash(H || blkKey_VerifierHashInput)[:keyBits/8]
key2 = hash(H || blkKey_EncryptedVerifierHashValue)[:keyBits/8]
```

块密钥常量：

```rust
const BLK_KEY_VERIFIER_HASH_INPUT: [u8; 8] =
    [0xFE, 0xA7, 0xD2, 0x76, 0x3B, 0x4B, 0x9E, 0x79];
const BLK_KEY_ENCRYPTED_VERIFIER_HASH_VALUE: [u8; 8] =
    [0xD7, 0xAA, 0x0F, 0x6D, 0x30, 0x61, 0x34, 0x4E];
```

### 3. 解密并比较

```text
verifier_input = AES-CBC-Decrypt(key1, iv=salt, encryptedVerifierHashInput)
computed       = hash(verifier_input)
stored         = AES-CBC-Decrypt(key2, iv=salt, encryptedVerifierHashValue)
match          = computed[:hashSize] == stored[:hashSize]
```

IV 是盐，而不是零。加密数据按块对齐；不使用 PKCS#7 填充。

## OLE2 与 XML 解析

- `cfb` 打开复合文件并读取 `EncryptionInfo` 流。
- 一个小型属性提取器在固定的 Agile XML 中查找 `name="value"` 对。不需要完整的 XML 解析器，因为在 encrypted-key 元素中，schema 是扁平的且没有命名空间。
- Base64 值使用 `base64` crate 解码。

## AES-CBC

`aes_cbc_decrypt_raw` 逐块执行 AES 解密（ECB），并与前一个密文块进行 XOR，第一个块使用盐作为 IV。AES-128、AES-192 和 AES-256 按密钥长度选择。

## 性能

Agile 验证有意设计得很昂贵：

- 约 100,000 次哈希迭代（典型文件中为 SHA-512）
- 两次 AES-CBC 解密
- 对验证器输入进行一次哈希

实测吞吐量：在 10 核 M2 Pro 上为 **约 585 个密码/秒**（取决于 `spinCount` 和哈希算法，为 500–800/s）。这就是对 Office 文件强烈推荐字典攻击和混合攻击的原因。

## 测试

测试文件使用 `msoffcrypto-tool` 生成：

```python
from msoffcrypto.format.ooxml import OOXMLFile

with open("test.docx", "rb") as f, open("encrypted.docx", "wb") as out:
    OOXMLFile(f).encrypt("92eo", out)
```

生成的文件是使用 Agile AES-256 和 SHA-512 的 OLE2 封装 OOXML。

测试文件：`test.docx`、`test.xlsx`、`test.pptx`（密码均为 `92eo`）。

## 限制

- **仅 Agile（v4）**：旧式 CryptoAPI/ECMA-376 `.doc`/`.xls`/`.ppt` 文件会被拒绝，并给出明确的错误。
- **非 ASCII 密码**：有效 UTF-8 的候选会被解码并编码为 UTF-16LE，因此中文和其他 Unicode 密码可以正常工作（`test_unicode.docx` 覆盖了这一点）。不是有效 UTF-8 的候选会回退到逐字节 UTF-16LE 编码。
- **EncryptedPackage** 不会被解密；验证器哈希是权威的。
- 只检查密码验证器。解密后包的完整性不会被校验（恢复密码也不需要）。
