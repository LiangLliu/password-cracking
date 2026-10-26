# 测试文件

[English](README.md) | **中文**

端到端测试和手动检查使用的加密文件。除表格中特别说明外，密码都是 `92eo`。

| 文件 | 格式 | 加密方式或用途 |
|------|------|----------------|
| `test.zip` | ZIP | ZipCrypto |
| `test_aes.zip` | ZIP | AES-256，由 7-Zip 创建 |
| `test_multi.zip` | ZIP | 两个加密条目，密码相同 |
| `test_mixed.zip` | ZIP | 第一个条目未加密，第二个加密 |
| `test.pdf` | PDF | RC4-128（R3） |
| `test_aes128.pdf` | PDF | AES-128（R4） |
| `test_r6.pdf` | PDF | AES-256（R6，Algorithm 2.B） |
| `test_xref.pdf` | PDF | 使用 xref stream 的 AES-256 R6 |
| `test_owner_r4.pdf` | PDF | AES-128，仅 owner 密码 |
| `test_owner_r6.pdf` | PDF | AES-256，仅 owner 密码 |
| `test.docx` | Word | Agile AES-256 |
| `test.xlsx` | Excel | Agile AES-256 |
| `test.pptx` | PowerPoint | Agile AES-256 |
| `test_unicode.docx` | Word | Agile AES-256，密码为 `密码92eo` |

只接受 owner 密码的 PDF 需要加上 `--pdf-owner`。Unicode 测试文件用于覆盖
UTF-16LE 密码编码。

## 运行测试文件

```bash
cargo build --release

for f in test.zip test_aes.zip test_multi.zip test_mixed.zip \
         test.pdf test_aes128.pdf test_r6.pdf test_xref.pdf \
         test.docx test.xlsx test.pptx; do
    printf '%-20s ' "$f"
    ./target/release/password-cracking -q -f "examples/$f" \
        dictionary -w wordlists/common-passwords.txt
done

for f in test_owner_r4.pdf test_owner_r6.pdf; do
    printf '%-20s ' "$f"
    ./target/release/password-cracking -q -f "examples/$f" \
        dictionary -w wordlists/common-passwords.txt --pdf-owner
done
```

其他攻击模式：

```bash
./target/release/password-cracking -f examples/test.zip \
    brute-force -c alnum --min-length 1 --max-length 4

./target/release/password-cracking -f examples/test.zip mask -m "?d?d?l?l"

./target/release/password-cracking -f examples/test.zip hybrid \
    -w wordlists/common-passwords.txt --capitalize --l33t
```

端到端测试使用 `cargo test --test e2e`。

## 重新生成

Python 生成脚本使用 [uv](https://docs.astral.sh/uv/)：

```bash
cd examples
uv sync
uv run python create_test_files.py
```

`test_r6.pdf` 不重新生成，因为它是 Algorithm 2.B 的已知答案测试。`test_aes.zip`
需要 7-Zip：

```bash
printf 'payload\n' > payload.txt
7z a -tzip -p92eo -mem=AES256 test_aes.zip payload.txt
```

`test_multi.zip` 和 `test_mixed.zip` 使用 Info-ZIP 的 `zip` 命令。
`test_unicode.docx` 的密码为 `密码92eo`，使用 `msoffcrypto-tool` Python 包
生成。如果使用不同参数重新生成测试文件，请同时更新表格和端到端测试。
