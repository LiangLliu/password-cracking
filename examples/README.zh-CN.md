[English](README.md) | **中文**

# 测试文件

e2e 测试和手动检查使用的加密文件。除非另有说明，密码为 **`92eo`**。

| 文件 | 格式 | 加密方式 / 用途 |
|------|--------|----------------------|
| `test.zip` | ZIP | ZipCrypto |
| `test_aes.zip` | ZIP | AES-256 (7-Zip) |
| `test_multi.zip` | ZIP | 两个加密条目，同一密码 |
| `test_mixed.zip` | ZIP | 第一个条目未加密，第二个加密 |
| `test.pdf` | PDF | RC4-128 (R3) |
| `test_aes128.pdf` | PDF | AES-128 (R4) |
| `test_r6.pdf` | PDF | AES-256（R6，Algorithm 2.B） |
| `test_xref.pdf` | PDF | AES-256（R6，xref stream，无经典 trailer） |
| `test_owner_r4.pdf` | PDF | AES-128（R4），仅 owner 密码 |
| `test_owner_r6.pdf` | PDF | AES-256（R6），仅 owner 密码 |
| `test.docx` | Word | Agile AES-256 |
| `test.xlsx` | Excel | Agile AES-256 |
| `test.pptx` | PowerPoint | Agile AES-256 |
| `test_unicode.docx` | Word | Agile AES-256，密码 **`密码92eo`** |

仅 owner 密码的 PDF 的 user 密码未知；请传入 `--pdf-owner`。Unicode Office 测试文件用于覆盖 UTF-16LE 密码编码。

## 针对测试文件运行

```bash
# 先构建
cargo build --release

# 所有标准测试文件
for f in test.zip test_aes.zip test_multi.zip test_mixed.zip \
         test.pdf test_aes128.pdf test_r6.pdf test_xref.pdf \
         test.docx test.xlsx test.pptx; do
    printf '%-20s ' "$f"
    ./target/release/password-cracking -q -f "examples/$f" \
        dictionary -w wordlists/common-passwords.txt
done

# owner 密码 PDF
for f in test_owner_r4.pdf test_owner_r6.pdf; do
    printf '%-20s ' "$f"
    ./target/release/password-cracking -q -f "examples/$f" \
        dictionary -w wordlists/common-passwords.txt --pdf-owner
done

# Unicode Office 密码
printf '密码92eo\n' > /tmp/unicode.txt
./target/release/password-cracking -q -f examples/test_unicode.docx \
    dictionary -w /tmp/unicode.txt
```

其他攻击模式：

```bash
# 暴力破解："92eo" 位于 alnum 1-4 中
./target/release/password-cracking -f examples/test.zip \
    brute-force -c alnum --min-length 1 --max-length 4

# Mask：数字 数字 小写 小写
./target/release/password-cracking -f examples/test.zip mask -m "?d?d?l?l"

# 混合攻击
./target/release/password-cracking -f examples/test.zip hybrid \
    -w wordlists/common-passwords.txt --capitalize --l33t

# 字典目录
./target/release/password-cracking -f examples/test.zip dictionary -w wordlists/
```

e2e 测试套件：

```bash
cargo test --test e2e
```

## 重新生成测试文件

Python 生成器使用 [uv](https://docs.astral.sh/uv/)：

```bash
cd examples
uv sync
uv run python create_test_files.py
```

它会重新生成 `test.pdf`、`test_aes128.pdf`、`test_xref.pdf`、`test_owner_r4.pdf`、`test_owner_r6.pdf`、`test.docx`、`test.xlsx` 和 `test.pptx`。有些测试文件有意不重新生成：

- `test_r6.pdf` 支撑 Algorithm 2.B 的已知答案测试。
- `test_aes.zip` 需要 7-Zip（AES 支持）：
  ```bash
  printf 'payload\n' > payload.txt
  7z a -tzip -p92eo -mem=AES256 test_aes.zip payload.txt
  ```
- `test_multi.zip` / `test_mixed.zip` 需要 Info-ZIP `zip`：
  ```bash
  printf 'alpha\n' > a.txt; printf 'bravo\n' > b.txt
  zip -P 92eo test_multi.zip a.txt b.txt
  zip test_mixed.zip a.txt
  zip -P 92eo test_mixed.zip b.txt
  ```
- `test_unicode.docx` 使用了不同的密码：
  ```python
  import docx
  from msoffcrypto.format.ooxml import OOXMLFile

  doc = docx.Document()
  doc.add_paragraph("unicode password test")
  doc.save("tmp.docx")
  with open("tmp.docx", "rb") as f, open("test_unicode.docx", "wb") as out:
      OOXMLFile(f).encrypt("密码92eo", out)
  ```

## 重新生成的要求

- Python 3.12+
- `uv`
- `pikepdf`, `python-docx`, `openpyxl`, `python-pptx`, `msoffcrypto-tool`
  （由 `uv sync` 根据 `pyproject.toml` 安装）
- 用于 AES ZIP 的 `7z`
- 用于多条目测试文件的 `zip`

## 说明

- 测试文件很小且已提交，因此无需 Python 即可运行 `cargo test`。
- 所有测试文件仅为授权测试生成。
- 如果使用不同参数重新生成某个测试文件，e2e 测试和文档中记录的密码必须一起更新。
