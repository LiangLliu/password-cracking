[English](USER_GUIDE.md) | **中文**

# 用户指南

`password-cracking` 的完整使用指南。命令行参考也可以通过
`password-cracking --help` 和 `password-cracking man` 获取。

## 目录

1. [安装](#installation)
2. [快速开始](#quick-start)
3. [工作原理](#how-it-works)
4. [文件格式支持](#file-format-support)
5. [攻击模式](#attack-modes)
   - [字典](#dictionary-attack)
   - [暴力破解](#brute-force-attack)
   - [Mask](#mask-attack)
   - [混合](#hybrid-attack)
6. [全局选项](#global-options)
7. [检查点与断点续跑](#checkpoint-and-resume)
8. [字符集](#character-sets)
9. [变异规则](#mutation-rules)
10. [词表](#wordlists)
11. [静默模式与退出码](#quiet-mode-and-exit-codes)
12. [Shell 补全与 man page](#shell-completions-and-man-page)
13. [性能建议](#performance-tips)
14. [示例](#examples)
15. [排错](#troubleshooting)
16. [限制](#limitations)

---

<a id="installation"></a>
## 安装

### 包管理器

```bash
# Homebrew (macOS / Linux)
brew install LiangLliu/tap/password-cracking

# Debian / Ubuntu / Raspberry Pi OS (signed APT repository)
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://lianglliu.github.io/password-cracking/key.gpg \
  | sudo tee /etc/apt/keyrings/password-cracking.gpg > /dev/null
echo "deb [signed-by=/etc/apt/keyrings/password-cracking.gpg] https://lianglliu.github.io/password-cracking stable main" \
  | sudo tee /etc/apt/sources.list.d/password-cracking.list
sudo apt update && sudo apt install password-cracking

# Docker
docker run --rm -v "$PWD:/data" ghcr.io/lianglliu/password-cracking --version
```

Arch Linux（`yay -S password-cracking-bin`）和 `cargo install
password-cracking` 会在 AUR 包和 crates.io token 配置完成后可用。当前状态参见
[分发渠道](DISTRIBUTION.md)。

### 静态二进制文件

```bash
curl -fsSL https://raw.githubusercontent.com/LiangLliu/password-cracking/master/scripts/install.sh | sh
```

安装脚本会校验发布的 SHA-256，在 Linux 上优先选择静态 musl 构建，并安装到
`/usr/local` 或 `~/.local`。

### 从源码构建

```bash
git clone https://github.com/LiangLliu/password-cracking
cd password-cracking
cargo build --release
# binary: target/release/password-cracking
```

---

<a id="quick-start"></a>
## 快速开始

`examples/` 中的 fixture 使用的密码是 `92eo`。

```bash
password-cracking -f examples/test.zip dictionary -w wordlists/common-passwords.txt
```

```text
Password Cracker
================
Target: examples/test.zip

Format:  ZIP (ZipCrypto)
Attack:  dictionary
Threads: 10
Keyspace: 33

Result
======
Duration: 0s
Attempts: 33
Speed:    3518 passwords/sec

Password found: 92eo
```

在终端上，搜索运行时会显示进度条。当 stdout 不是终端时（管道、CI 日志），进度条
会被抑制。

---

<a id="how-it-works"></a>
## 工作原理

`password-cracking` 由两个相互独立的部分组成：

- **`PasswordSource`** 生成候选：词表行、暴力破解组合、Mask 组合，或带有变异
  的词表单词。
- **`PasswordVerifier`** 用候选对目标文件进行校验。

一个生成器线程填充有界批次，并通过 FIFO 通道将其发送给固定大小的 worker 线程
池。在存在廉价过滤器时，引擎采用两阶段检查：

1. `quick_check()` 以较低成本拒绝大多数错误密码。只有 ZIP ZipCrypto 有真正有
   意义的过滤器（12 字节的加密头）。
2. `verify()` 执行完整的密码校验。

对于 PDF 和 Office，校验本身就是昂贵的步骤，因此 `quick_check()` 会有意返回
`true`，密码只会被校验一次。

批次大小在 32 到 4096 个候选之间自适应，目标约为每批 250 ms。这样对于慢速格式
（Office）能保持检查点及时，对于快速格式（ZIP）则能摊薄同步开销。

---

<a id="file-format-support"></a>
## 文件格式支持

格式通过魔数字节检测；扩展名仅用于区分 OOXML（`.docx`/`.xlsx`/`.pptx`）与普通
ZIP 归档。

| 格式 | 扩展名 | 加密 | 典型速度（10 核 M2 Pro） |
|--------|------------|------------|-------------------------------|
| ZIP | `.zip` | ZipCrypto、AES-128/192/256 | 11M+ 个密码/秒（ZipCrypto） |
| PDF | `.pdf` | RC4-40、RC4-128、AES-128、AES-256 R5/R6 | 约 530K 个密码/秒（RC4-128） |
| Office | `.docx`、`.xlsx`、`.pptx` | Agile AES-128/192/256 | 500–800 个密码/秒 |

PDF 的 owner 密码需要显式启用（`--pdf-owner`），因为额外的检查会使每个候选的
成本大约增加到三倍。

---

<a id="attack-modes"></a>
## 攻击模式

<a id="dictionary-attack"></a>
### 字典

尝试词表中的每一行。支持单个文件和目录。

```bash
# Single file
password-cracking -f document.zip dictionary -w passwords.txt

# Directory: all .txt files, sorted, recursively
password-cracking -f document.zip dictionary -w ./wordlists/

# Remove duplicates (memory grows with the number of unique candidates)
password-cracking -f document.zip dictionary -w /data/rockyou.txt --dedup
```

词表以内存映射方式读取，并按字节偏移访问。无论文件多大，内存占用都保持平稳。
每行会裁剪掉 ASCII 空白；空行和以 `#` 开头的行会被跳过。非 UTF-8 词表（例如
GBK）也能工作，因为候选按字节处理。

<a id="brute-force-attack"></a>
### 暴力破解

为 `min-length..=max-length` 范围内的每个长度生成字符集的所有组合。

```bash
# 4-digit PIN
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 4

# 1-6 digits
password-cracking -f document.zip brute-force -c digits --min-length 1 --max-length 6

# Lowercase, 1-4 characters
password-cracking -f document.zip brute-force -c lower --min-length 1 --max-length 4

# Custom character set
password-cracking -f document.zip brute-force -c "abc123!@#" --min-length 1 --max-length 8
```

精确长度的密钥空间（`C^L`）：

| 字符集 | 长度 4 | 长度 6 | 长度 8 |
|---------|----------|----------|----------|
| `digits` (10) | 10,000 | 1,000,000 | 100,000,000 |
| `lower` (26) | 456,976 | 308,915,776 | 208,827,064,576 |
| `alnum` (62) | 14,776,336 | 56,800,235,584 | 218,340,105,584,896 |
| `all` (93) | 74,805,201 | 646,990,183,449 | 5,595,818,096,650,401 |

<a id="mask-attack"></a>
### Mask

Mask 攻击为每个位置指定一个字符集。当你知道密码的形态时，它是最佳选择。

```bash
# 92eo = digit digit lower lower
password-cracking -f document.zip mask -m "?d?d?l?l"

# Capital + 8 lower + 2 digits
password-cracking -f document.docx mask -m "?u?l?l?l?l?l?l?l?d?d"

# Literal text: "admin" followed by two digits
password-cracking -f document.zip mask -m "admin?d?d"

# A literal question mark
password-cracking -f document.zip mask -m "what??"
```

| 占位符 | 字符集 | 大小 |
|-------------|---------------|------|
| `?d` | 数字 | 10 |
| `?l` | 小写字母 | 26 |
| `?u` | 大写字母 | 26 |
| `?s` | 可打印特殊字符 | 31 |
| `?a` | 所有可打印 ASCII | 93 |
| `??` | 字面 `?` | 1 |

任何其他字符都按字面处理。多字节 UTF-8 字面量会逐字节保留。一个 mask 最多可以
有 128 个位置。

<a id="hybrid-attack"></a>
### 混合

对每个词表单词应用变异规则。原始单词始终包含在内。

```bash
# Capitalize and apply l33t substitutions
password-cracking -f document.zip hybrid -w wordlists/common-passwords.txt --capitalize --l33t

# Append 0-99 to each word
password-cracking -f document.zip hybrid -w wordlists/common-passwords.txt --append-digits 99

# All rules at once
password-cracking -f document.zip hybrid -w wordlists/common-passwords.txt \
  --capitalize --upper --lower --l33t --reverse --duplicate --append-digits 99
```

每个单词的密钥空间：`1 (original) + number of active rules + (N + 1)`，其中 `N`
是 `--append-digits` 的值。在 33 个单词且使用
`--capitalize --append-digits 99` 时，总数为 `33 × (1 + 1 + 100) = 3,366`。

参见[变异规则](#mutation-rules)。

---

<a id="global-options"></a>
## 全局选项

| 选项 | 短选项 | 说明 |
|--------|-------|-------------|
| `--file <PATH>` | `-f` | 目标文件（攻击模式必需） |
| `--threads <N>` | `-t` | worker 线程数（默认：所有逻辑核心；最小 1） |
| `--quiet` | `-q` | 仅打印结果 |
| `--checkpoint <PATH>` | | 定期将进度保存到 PATH |
| `--resume` | | 从检查点文件断点续跑 |
| `--checkpoint-interval <SECS>` | | 两次检查点写入之间的秒数（默认 5） |
| `--pdf-owner` | | 同时尝试 PDF owner 密码（约慢 3 倍） |
| `--help` | `-h` | 显示帮助 |
| `--version` | `-V` | 显示版本 |

全局选项可以出现在子命令之前或之后：

```bash
password-cracking -f document.zip -t 4 dictionary -w passwords.txt
password-cracking dictionary -w passwords.txt -f document.zip -t 4
```

---

<a id="checkpoint-and-resume"></a>
## 检查点与断点续跑

长时间的字典、暴力破解、Mask 和混合运行可以中断后再恢复，而不会重新校验已经
校验过的候选。

```bash
# Start; a checkpoint is written every 5 seconds
password-cracking -f document.pdf brute-force -c alnum --min-length 1 --max-length 8 \
  --checkpoint progress.ck

# Continue after Ctrl-C, a crash, or a reboot
password-cracking -f document.pdf brute-force -c alnum --min-length 1 --max-length 8 \
  --checkpoint progress.ck --resume

# Custom interval
password-cracking ... --checkpoint progress.ck --checkpoint-interval 10
```

检查点保存的是**已验证前沿**：最早一个尚未完全验证的批次的起始位置。因此断点
续跑绝不会跳过那些只生成过或只部分检查过的候选。强制杀死进程最多丢失一个检查点
间隔加上正在处理中的批次。

该文件包含三行：

```text
brute-force
5:0,0,0,0:1
/absolute/path/to/document.pdf
```

- 来源名称（`dictionary`、`brute-force`、`mask`、`hybrid`）
- 来源相关的位置
- 规范化的目标路径

如果来源或目标与检查点不一致，断点续跑会拒绝运行。

注意：

- 搜索结束（找到或穷尽）时会删除检查点。
- 对于字典攻击，词表在两次运行之间必须保持不变。目录中的文件按排序顺序处理，
  以保证稳定性。
- 使用 `--dedup` 时，断点续跑后不会重建去重集合。重复项可能会被重试；这无害，
  只是额外的工作。

---

<a id="character-sets"></a>
## 字符集

| 名称 | 字符 | 数量 |
|------|------------|-------|
| `digits` | `0123456789` | 10 |
| `lower` | `abcdefghijklmnopqrstuvwxyz` | 26 |
| `upper` | `ABCDEFGHIJKLMNOPQRSTUVWXYZ` | 26 |
| `special` | `` !@#$%^&*()-_=+[]{}\|;:'",.<>/?`~ `` | 31 |
| `alnum` | 数字 + 小写 + 大写 | 62 |
| `all` | 数字 + 小写 + 大写 + 特殊字符 | 93 |

任何其他字符串都会被当作自定义字符集。应避免重复字符；它们只会重复工作。

---

<a id="mutation-rules"></a>
## 变异规则

| 规则 | 标志 | 输入 `password` → 输出 |
|------|------|---------------------------|
| 首字母大写 | `--capitalize` | `Password` |
| 转大写 | `--upper` | `PASSWORD` |
| 转小写 | `--lower` | `password` |
| L33t | `--l33t` | `p@$$w0rd` |
| 反转 | `--reverse` | `drowssap` |
| 重复 | `--duplicate` | `passwordpassword` |
| 追加数字 | `--append-digits N` | `password0` … `passwordN` |

L33t 替换：`a→@`、`e→3`、`i→1`、`o→0`、`s→$`。

规则变体是从原始单词独立生成的，而不是累积生成的。例如，
`--capitalize --l33t` 会生成 `password`、`Password` 和 `p@$$w0rd`，
而不是 `P@$$w0rd`。

---

<a id="wordlists"></a>
## 词表

内置词表：

| 文件 | 内容 | 行数 |
|------|----------|-------|
| `common-passwords.txt` | 常见密码 | 33 |
| `chinese-common.txt` | 常见中文密码 | 40 |
| `pins-4digit.txt` | 0000–9999 | 10,000 |
| `pins-6digit.txt` | 000000–999999 | 1,000,000 |

目录模式会递归加载每个 `.txt` 文件，按排序顺序：

```bash
password-cracking -f document.zip dictionary -w wordlists/
```

创建你自己的词表时，使用 UTF-8 或原始字节文本文件，每行一个候选。以 `#` 开头的
行是注释。

---

<a id="quiet-mode-and-exit-codes"></a>
## 静默模式与退出码

`-q` 只在 stdout 上打印密码。诊断信息发送到 stderr。

```bash
PASSWORD=$(password-cracking -q -f document.zip dictionary -w passwords.txt)
```

| 退出码 | 含义 |
|-----------|---------|
| `0` | 找到密码 |
| `1` | 搜索已穷尽，未找到密码 |
| `2` | 输入无效或运行时错误（详情见 stderr） |

在静默模式下，未找到密码时会向 stderr 打印 `not found`；stdout 保持为空。

---

<a id="shell-completions-and-man-page"></a>
## Shell 补全与 man page

```bash
# Bash
password-cracking completions bash > ~/.local/share/bash-completion/completions/password-cracking

# Zsh
password-cracking completions zsh > "${fpath[1]}/_password-cracking"

# Fish
password-cracking completions fish > ~/.config/fish/completions/password-cracking.fish

# Man page
password-cracking man > /tmp/password-cracking.1
man /tmp/password-cracking.1
```

软件包构建（`deb`/`rpm`/`apk`、Homebrew）会自动安装这些内容。

---

<a id="performance-tips"></a>
## 性能建议

1. **先从字典开始。** 它比暴力破解便宜几个数量级。
2. **使用能奏效的最小字符集。** 每个位置上，digits 比 alnum 小 6.2 倍。
3. **限制长度。** 每多一个位置，密钥空间就会乘以字符集大小。
4. **知道密码形态时使用 Mask 攻击。** `?d?d?l?l` 是 67,600 个候选，而 `?a?a?a?a`
   是 7480 万个。
5. **让它用满所有核心。** 默认使用所有逻辑 CPU；只有在机器共享时才用 `-t`
   限制。
6. **对于 Office，先使用字典/混合。** Agile 加密每个候选需要约 100K 次 SHA-512
   迭代。

在 11M 个密码/秒的速度下，精确长度的 ZIP ZipCrypto 搜索的近似时间：

| 字符集 | 长度 4 | 长度 6 | 长度 8 |
|---------|----------|----------|----------|
| digits | 瞬间 | 瞬间 | 9 秒 |
| lower | 瞬间 | 28 秒 | 5.3 小时 |
| alnum | 1.3 秒 | 1.4 小时 | 230 天 |
| all | 7 秒 | 16 小时 | 16 年 |

PDF 和 Office 的表格以及基准测试方法参见[性能指南](PERFORMANCE.md)。

---

<a id="examples"></a>
## 示例

### 忘记的 ZIP 密码

你记得它是四个小写字母：

```bash
password-cracking -f backup.zip brute-force -c lower --min-length 4 --max-length 4
```

你记得它以 `pass` 开头，并以两个小写字母结尾：

```bash
password-cracking -f backup.zip mask -m "pass?l?l"
```

### 受 4–6 位 PIN 保护的 PDF

```bash
password-cracking -f statement.pdf brute-force -c digits --min-length 4 --max-length 6
```

### 只接受 owner 密码的 PDF

```bash
password-cracking -f document.pdf dictionary -w passwords.txt --pdf-owner
```

`--pdf-owner` 为 RC4-40/128、AES-128 和 AES-256（R5/R6）启用 owner 校验。默认
关闭，因为它会使每个候选的成本大约增加到三倍。

### 使用常见密码的 Office 文档

```bash
# Dictionary first — Office is slow
password-cracking -f report.docx dictionary -w wordlists/common-passwords.txt

# Large list: streaming keeps memory flat; --dedup removes duplicates
password-cracking -f report.docx dictionary -w /data/rockyou.txt --dedup

# Mutations: capitalize + append 0-99
password-cracking -f report.docx hybrid -w wordlists/common-passwords.txt \
  --capitalize --append-digits 99
```

### 批量处理

```bash
#!/usr/bin/env bash
set -u

for file in ~/documents/*.{zip,pdf,docx,xlsx,pptx}; do
    [ -f "$file" ] || continue
    printf 'Cracking %s ... ' "$file"
    if password-cracking -q -f "$file" dictionary -w wordlists/ 2>/dev/null; then
        :
    else
        echo "not found"
    fi
done
```

`if` 能起作用，是因为 CLI 在未找到密码时会以退出码 1 退出。

### 内置 fixture

```bash
cd examples
uv run python create_test_files.py   # regenerate fixtures
cd ..

for fmt in zip pdf docx xlsx pptx; do
    printf '%-6s ' "$fmt"
    ./target/release/password-cracking -q -f "examples/test.$fmt" \
        dictionary -w wordlists/common-passwords.txt
done
```

---

<a id="troubleshooting"></a>
## 排错

| 现象 | 原因 / 修复 |
|---------|-------------|
| `password not found` 且退出码 1 | 搜索空间已穷尽。检查词表、字符集和长度范围。 |
| 词表中所有内容都被跳过 | 以 `#` 开头的行是注释。移除前缀或编辑文件。 |
| 仅 owner 密码的 PDF 文件未找到 | 添加 `--pdf-owner`。 |
| Office：`Unsupported Office encryption version` | 不支持旧式 CryptoAPI/ECMA-376 加密；仅支持 Agile（v4）。 |
| `Checkpoint belongs to target …` | 检查点是为另一个文件创建的。使用新的检查点路径。 |
| 断点续跑找不到本应存在的密码 | 两次运行之间词表或攻击参数发生了变化。断点续跑不会重新扫描已验证的前缀。 |
| Office 搜索非常慢 | 符合预期：每个候选约 100K 次 SHA-512 迭代。使用字典/混合。 |
| 读取词表时 `Permission denied` | 检查文件权限；容器可能需要 `--user`。 |
| 进度条缺失 | stdout 不是终端。这是有意为之。 |

---

<a id="limitations"></a>
## 限制

- **Office**：仅支持 Agile 加密（v4）；旧式 CryptoAPI/ECMA-376 会被拒绝。
- **RAR5 / 7z**：未实现。
- **PDF**：RC4/AES 的 user 和 owner 密码；空密码和证书加密不在范围内。
- **ZIP**：ZipCrypto 和 AES；不支持分卷/多卷归档。如果各个条目使用不同的密码，
  没有任何单一密码能通过验证器。
- **Windows**：不支持。
- **字典断点续跑**假定词表未发生变化。
