# 使用指南

[English](USAGE.md) | **中文**

## 快速开始

`examples/` 中的测试文件密码为 `92eo`：

```bash
cargo build --release
./target/release/password-cracking -f examples/test.zip \
  dictionary -w wordlists/common-passwords.txt
```

程序会输出识别到的格式、攻击模式、速度和结果。在终端中运行时会显示进度条，
通过管道或写入 CI 日志时不显示。

## 文件格式

程序先读取文件魔数，无法判断时回退到扩展名。扩展名还用于区分未加密的 OOXML
文件和普通 ZIP 压缩包。

| 格式 | 扩展名 | 加密方式 |
|------|--------|----------|
| ZIP | `.zip` | ZipCrypto、AES-128/192/256 |
| PDF | `.pdf` | RC4-40、RC4-128、AES-128、AES-256 R5/R6 |
| Office | `.docx`、`.xlsx`、`.pptx`、`.docm`、`.xlsm`、`.pptm` | Agile AES-128/192/256 |

PDF owner 密码默认不检查，需要显式开启：

```bash
password-cracking -f document.pdf dictionary -w passwords.txt --pdf-owner
```

owner 检查会让每个候选的工作量增加到约三倍。Office 支持 Office 2010 及更高
版本使用的 Agile 加密。旧版 CryptoAPI/ECMA-376 文件会返回错误。

## 攻击模式

### 字典

```bash
# 单个文件
password-cracking -f document.zip dictionary -w passwords.txt

# 目录中的所有 .txt 文件，按名称排序
password-cracking -f document.zip dictionary -w wordlists/

# 去重（内存占用随唯一候选数量增长）
password-cracking -f document.zip dictionary -w rockyou.txt --dedup
```

词表通过 mmap 按字节偏移读取，文件大小不会增加内存占用。每行会去掉首尾空白，
空行和以 `#` 开头的行会跳过。候选按字节处理，因此非 UTF-8 词表也能使用。

### 暴力破解

```bash
# 4 位数字 PIN
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 4

# 1 到 6 位数字
password-cracking -f document.zip brute-force -c digits --min-length 1 --max-length 6

# 自定义字符集
password-cracking -f document.zip brute-force -c "abc123!@#" --min-length 1 --max-length 8
```

单个长度的密钥空间为 `字符集大小 ^ 长度`。每增加一位，工作量就乘以字符集大小。

### Mask

Mask 为每个位置指定字符集。

```bash
# 两位数字加两位小写字母
password-cracking -f document.zip mask -m "?d?d?l?l"

# 固定文本加两位数字
password-cracking -f document.zip mask -m "admin?d?d"

# 密码中包含字面量问号（what?）
password-cracking -f document.zip mask -m "what??"
```

| 标记 | 字符集 | 数量 |
|------|--------|------|
| `?d` | 数字 | 10 |
| `?l` | 小写字母 | 26 |
| `?u` | 大写字母 | 26 |
| `?s` | 可打印特殊字符 | 31 |
| `?a` | 可打印 ASCII | 93 |
| `??` | 字面量 `?` | 1 |

其他字符按字面量处理。一个 Mask 最多包含 128 个位置。

### 混合

混合模式对每个词表单词应用变异规则，原单词始终包含在候选中。

```bash
# 首字母大写并应用 l33t 替换
password-cracking -f document.zip hybrid -w passwords.txt --capitalize --l33t

# 为每个单词追加 0 到 99
password-cracking -f document.zip hybrid -w passwords.txt --append-digits 99

# 同时使用多条规则
password-cracking -f document.zip hybrid -w passwords.txt \
  --capitalize --upper --lower --l33t --reverse --duplicate --append-digits 99
```

使用 `--append-digits N` 时，每个单词会生成原词、每条实际改变单词的规则对应的
变体，以及 `N + 1` 个追加数字的变体。未启用追加数字时不含最后一组。规则各自
作用于原单词，不会叠加。使用 `--capitalize --l33t` 时，`password` 会生成
`password`、`Password` 和 `p@$$w0rd`。

## 选项

| 选项 | 短选项 | 说明 |
|------|--------|------|
| `--file <PATH>` | `-f` | 目标文件 |
| `--threads <N>` | `-t` | worker 线程数（默认使用全部逻辑核心） |
| `--quiet` | `-q` | 只输出结果 |
| `--checkpoint <PATH>` | | 将进度保存到文件 |
| `--resume` | | 从检查点继续 |
| `--checkpoint-interval <SECS>` | | 写入检查点的间隔秒数（默认 5） |
| `--pdf-owner` | | 同时尝试 PDF owner 密码 |
| `--help` | `-h` | 显示帮助 |
| `--version` | `-V` | 显示版本 |

全局选项可以放在子命令之前或之后：

```bash
password-cracking -f document.zip -t 4 dictionary -w passwords.txt
password-cracking dictionary -w passwords.txt -f document.zip -t 4
```

## 检查点

字典、暴力、Mask 和混合攻击都支持中断后继续。

```bash
# 开始运行，每 5 秒写入一次检查点
password-cracking -f document.pdf brute-force -c alnum --min-length 1 --max-length 8 \
  --checkpoint progress.ck

# Ctrl-C 或重启后继续
password-cracking -f document.pdf brute-force -c alnum --min-length 1 --max-length 8 \
  --checkpoint progress.ck --resume
```

检查点保存已验证的前沿，即最早一个尚未完成批次的起点。续跑从该批次开始。强制终止最多丢失一个检查点间隔加上正在处理的批次。

检查点文件包含三行：

```text
brute-force
5:0,0,0,0:1
/absolute/path/to/document.pdf
```

三行分别是攻击模式、模式内部位置和目标文件路径。如果攻击模式或目标文件不一致，
续跑会拒绝启动。

搜索结束后程序会删除检查点，无论是找到密码还是耗尽密钥空间。字典续跑假设词表
内容没有变化。续跑后 `--dedup` 不会重建去重集合，因此程序可能重复检查部分候选。

## 字符集

| 名称 | 字符 | 数量 |
|------|------|------|
| `digits` | `0123456789` | 10 |
| `lower` | `abcdefghijklmnopqrstuvwxyz` | 26 |
| `upper` | `ABCDEFGHIJKLMNOPQRSTUVWXYZ` | 26 |
| `special` | 标点和符号 | 31 |
| `alnum` | 数字、小写和大写字母 | 62 |
| `all` | 数字、小写、大写和特殊字符 | 93 |

其他值会作为自定义字符集。重复字符只会重复工作。

## 变异规则

| 规则 | 选项 | `password` 变为 |
|------|------|-----------------|
| 首字母大写 | `--capitalize` | `Password` |
| 转为大写 | `--upper` | `PASSWORD` |
| 转为小写 | `--lower` | `password` |
| L33t | `--l33t` | `p@$$w0rd` |
| 反转 | `--reverse` | `drowssap` |
| 重复 | `--duplicate` | `passwordpassword` |
| 追加数字 | `--append-digits N` | `password0` 到 `passwordN` |

L33t 替换规则：`a` 替换为 `@`，`e` 替换为 `3`，`i` 替换为 `1`，`o` 替换为
`0`，`s` 替换为 `$`。

## 脚本调用

静默模式把密码写到 stdout，诊断信息写到 stderr。

```bash
PASSWORD=$(password-cracking -q -f document.zip dictionary -w passwords.txt)
```

| 退出码 | 含义 |
|--------|------|
| `0` | 找到密码 |
| `1` | 搜索完成，未找到密码 |
| `2` | 输入无效或运行错误 |

使用 `-q` 且未找到密码时，stdout 为空，stderr 输出 `not found`。

## Shell 集成

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

`.deb`、`.rpm`、`.apk` 和 Homebrew 包会自动安装补全脚本和 man page。

## 示例

ZIP 密码由 4 个小写字母组成：

```bash
password-cracking -f backup.zip brute-force -c lower --min-length 4 --max-length 4
```

ZIP 密码以 `pass` 开头，以两个小写字母结尾：

```bash
password-cracking -f backup.zip mask -m "pass?l?l"
```

PDF 只接受 owner 密码：

```bash
password-cracking -f statement.pdf dictionary -w passwords.txt --pdf-owner
```

Office 文件应优先使用字典和混合攻击：

```bash
password-cracking -f report.docx dictionary -w wordlists/common-passwords.txt
password-cracking -f report.docx hybrid -w wordlists/common-passwords.txt \
  --capitalize --append-digits 99
```

批量处理可以利用退出码：

```bash
#!/usr/bin/env bash
set -u

for file in ~/documents/*.{zip,pdf,docx,xlsx,pptx}; do
    [ -f "$file" ] || continue
    printf 'Cracking %s ... ' "$file"
    password-cracking -q -f "$file" dictionary -w wordlists/ || echo "not found"
done
```

## 调优

1. 优先尝试字典。
2. 使用能满足要求的最小字符集。
3. 限制长度，每增加一位都会成倍扩大密钥空间。
4. 已知密码形状时使用 Mask。
5. 机器没有其他负载时，线程数保持默认。
6. 大型词表且重复项不多时，不要使用 `--dedup`。

## 排错

| 现象 | 原因或处理 |
|------|------------|
| 退出码为 1 | 程序已遍历完密钥空间。检查词表、字符集和长度范围。 |
| 词表中的行被跳过 | 程序把以 `#` 开头的行当作注释，并忽略空行。 |
| 只能使用 owner 密码的 PDF 未找到 | 加上 `--pdf-owner`。 |
| `Unsupported Office encryption version` | 文件使用旧版 CryptoAPI/ECMA-376 加密。 |
| `Checkpoint belongs to target ...` | 检查点属于另一个文件。使用新的检查点路径。 |
| 续跑后没有找到本应存在的密码 | 词表或攻击参数在两次运行之间发生了变化。 |
| Office 耗时很长 | 每个候选约需 10 万次哈希迭代。请使用字典或混合攻击。 |
| 没有进度条 | stdout 不是终端。 |
