[English](PDF_IMPLEMENTATION.md) | **中文**

# PDF 内部实现

`src/formats/pdf.rs` 在不解密页面内容的情况下验证 PDF 的 user 密码和 owner 密码。密码检查本身就是权威的：正确匹配 `/U`（user）或 `/O`（owner）即可。

## 支持的算法

| V | R | 算法 | 密钥长度 | PDF 版本 |
|---|---|---|----------|-------------|
| 1 | 2 | RC4 | 40-bit | 1.3 |
| 2 | 3 | RC4 | 128-bit | 1.4 |
| 4 | 4 | AES-128（内容），RC4 U 检查 | 128-bit | 1.6 |
| 5 | 5 | AES-256（SHA-256） | 256-bit | 2.0 |
| 5 | 6 | AES-256（Algorithm 2.B） | 256-bit | 2.0 |

owner 密码在 R2–R4 和 R5/R6 中受支持，并且在 CLI 上为可选项（`--pdf-owner`），因为它们大约会使每个候选的开销增至三倍。

## 解析

不需要完整的 PDF 解析器：只需要 trailer、`/Encrypt` 字典和 `/ID`。实现只有几百行，避免了一个庞大的依赖（以及促使编写自定义解析器的 `lopdf`/`time` 兼容性问题）。

1. 确认 `/Encrypt` 存在。
2. 查找 trailer：
   - 经典的 `trailer` 关键字，或
   - 包含 `/Encrypt` 的 xref stream 字典（PDF 1.5+ 文件通常没有经典 trailer）。
3. 提取 `/Encrypt`（间接引用或内联字典）和 `/ID[0]`。
4. 解析加密对象（`N 0 obj … endobj`）。
5. 解析 `/V`、`/R`、`/Length`、`/O`、`/U`、`/P`、`/EncryptMetadata`。
6. 对于 V ≥ 5，将 `/U` 和 `/O` 拆分为 `[32 字节哈希][8 字节验证盐][8 字节密钥盐]`。

重要的解析细节：

- **顶层 `/Length`**：`/CF` 包含嵌套的 `/Length` 值。解析器跟踪字典深度，只读取顶层值；否则 AES-128 文件可能被当作 40 位处理。
- **有符号 `/P`**：PDF 中的权限是有符号 32 位整数，而不是无符号。
- **十六进制字符串**：`/O` 和 `/U` 是十六进制字符串；其中的空白会被忽略。
- **间接引用**：`/Encrypt 4 0 R` 和内联字典都接受。
- **`/EncryptMetadata`**：对于 R ≥ 4，值为 `false` 时会在密钥派生输入后追加 `0xFFFFFFFF`。

## RC4 密钥派生（R2–R4）

```text
1. 将密码填充或截断至 32 字节（ISO 32000-1 padding）
2. 追加 /O（32 字节）
3. 以 4 字节小端追加 /P
4. 追加 /ID[0]
5. 如果 R >= 4 且 /EncryptMetadata 为 false，追加 0xFFFFFFFF
6. 对结果计算 MD5
7. 如果 R >= 3，对前 N 字节运行 50 次 MD5（N = 密钥长度）
8. 截断至 N 字节
```

## user 密码检查

**R = 2**：使用文件密钥对 32 字节填充进行 RC4 加密，并将结果与 `/U` 比较。

**R ≥ 3**：

```text
1. H = MD5(padding + /ID[0])
2. R0 = RC4(key, H)
3. for i in 1..=19: Ri = RC4(key XOR i, R(i-1))
4. 比较 R19 与 /U[0..16]
```

这是 RC4-40、RC4-128 和 AES-128（R4）使用的检查。AES-128 使用 AES 加密内容，但 `/U` 验证仍基于 RC4。

## AES-256 R5

```text
user:  SHA-256(password + U_validation_salt)              == U[0..32]
owner: SHA-256(password + O_validation_salt + full_U)     == O[0..32]
```

## AES-256 R6（Algorithm 2.B）

R6 用迭代构造替代单次哈希：

```text
K = SHA-256(password || salt || udata)
round = 0
loop:
    K1 = (password || K || udata) 重复 64 次
    E  = AES-128-CBC-Encrypt(key=K[0..16], iv=K[16..32], K1)   // 无填充
    K  = SHA-256/384/512(E)，由 sum(E[0..16]) % 3 选择
    round += 1
    当 round >= 64 且 E[last] <= round - 32 时停止
return K[0..32]
```

- user 密码：`salt = U_validation_salt`，`udata = []`。
- owner 密码：`salt = O_validation_salt`，`udata = 完整的 48 字节 U`。

实现包含针对 `92eo` 的已知答案测试，并已通过 `test_r6.pdf` 和 `test_xref.pdf` 验证。

## owner 密码（R2–R4，Algorithm 3）

```text
1. H = MD5(pad(owner_password)); 如果 R >= 3，进行 50 轮
2. key = H[0..N]
3. 如果 R == 2:  O_padded = RC4(key, /O)
   否则:         for i in 19..=0: O_padded = RC4(key XOR i, O_padded)
4. 将 O_padded 作为 user 密码对 /U 验证
```

对于 R5/R6，owner 哈希直接计算（见上文），使用完整的 `/U` 条目作为额外数据。

## RC4 与 AES 原语

- RC4 以内联方式实现（KSA + PRGA），因为 PDF 路径只需要对小缓冲区进行加密/解密，避免依赖可以让热循环保持简单。
- Algorithm 2.B 使用的 AES-128-CBC 实现不带填充：输入始终按块对齐，输出也不做 PKCS#7 填充。
- SHA-256/384/512 来自 `sha2` crate；MD5 来自 `md-5`。

## 性能

在 10 核 M2 Pro 上实测：

| 场景 | 速度 |
|------|-------|
| RC4-128 (R3) | 约 529K 个密码/秒 |
| AES-128 (R4) | 约 515K 个密码/秒 |
| AES-256 (R6) | 约 7.6K 个密码/秒 |
| RC4-128 配合 `--pdf-owner` | 约 150K 个密码/秒 |

R6 较慢，因为 Algorithm 2.B 会为每个候选执行数十次 AES-CBC 和哈希操作。

## 测试与测试文件

| 文件 | 加密方式 |
|------|------------|
| `test.pdf` | RC4-128 (R3) |
| `test_aes128.pdf` | AES-128 (R4) |
| `test_r6.pdf` | AES-256 (R6) |
| `test_xref.pdf` | AES-256（R6），xref stream，无经典 trailer |
| `test_owner_r4.pdf` | AES-128（R4），仅 owner 密码 |
| `test_owner_r6.pdf` | AES-256（R6），仅 owner 密码 |

全部使用密码 `92eo`。仅 owner 密码的文件需要 `--pdf-owner`（CLI）或 `VerifyOptions { try_pdf_owner: true }`（库）。

测试文件使用 `pikepdf` 生成；参见 `examples/create_test_files.py`。

## 限制

- 空密码 PDF 不会被特殊处理。
- 不支持基于证书的加密。
- 内容不会被解密，因此格式错误的加密流不会被校验；`/U` 或 `/O` 检查是权威测试。
- 带有异常盐或 `/U`/`/O` 条目被截断的 R5/R6 文件会干净地失败，而不是进行猜测。
