# 文档密码破解工具

[English](README.md) | **中文**

`password-cracking` 用于找回 ZIP、PDF 和 Office 文件的密码，支持 Linux 和
macOS。程序在本地 CPU 上运行，不访问网络，提供字典、暴力、Mask 和混合四种
攻击模式。

> 请仅对自己拥有或已获授权测试的文件使用本工具。

## 功能

- ZIP：ZipCrypto、AES-128/192/256。
- PDF：RC4-40/128、AES-128、AES-256 R5/R6，支持 user 和 owner 密码。
- Office：Agile AES-128/192/256（Office 2010 及更高版本）。
- 字典、暴力、Mask 和混合四种攻击模式。
- 词表使用 mmap，候选按批生成，内存占用保持平稳。
- 检查点可续跑长时间任务，不重复验证已完成的候选。
- 通过文件魔数识别格式，无法判断时回退到扩展名。
- 提供 Linux 安装包、Homebrew、Docker 和静态二进制安装脚本。

## 安装

```bash
# Homebrew（macOS / Linux）
brew install LiangLliu/tap/password-cracking

# Debian、Ubuntu、树莓派系统（签名 APT 仓库）
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://lianglliu.github.io/password-cracking/key.gpg \
  | sudo tee /etc/apt/keyrings/password-cracking.gpg > /dev/null
echo "deb [signed-by=/etc/apt/keyrings/password-cracking.gpg] https://lianglliu.github.io/password-cracking stable main" \
  | sudo tee /etc/apt/sources.list.d/password-cracking.list
sudo apt update && sudo apt install password-cracking

# Docker
docker run --rm -v "$PWD:/data" ghcr.io/lianglliu/password-cracking --version

# 静态二进制安装脚本（Linux / macOS）
curl -fsSL https://raw.githubusercontent.com/LiangLliu/password-cracking/master/scripts/install.sh | sh

# 从源码安装
cargo install --git https://github.com/LiangLliu/password-cracking
```

每个 release 还附带 `.deb`、`.rpm`、`.apk` 包和 Arch Linux 的 `PKGBUILD`。
AUR 与 crates.io 发布需要一次性配置账号，步骤见[开发指南](docs/DEVELOPMENT.zh-CN.md)。

## 使用

```bash
# 字典攻击
password-cracking -f document.zip dictionary -w passwords.txt

# 暴力破解 4 到 6 位数字 PIN
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 6

# Mask：两位数字加两位小写字母
password-cracking -f document.zip mask -m "?d?d?l?l"

# 混合攻击：字典加变异规则
password-cracking -f document.docx hybrid -w dict.txt --capitalize --l33t --append-digits 99

# 断点续跑
password-cracking -f document.pdf brute-force -c alnum --min-length 1 --max-length 8 \
  --checkpoint progress.ck --resume

# 只输出密码，便于脚本调用
password-cracking -q -f document.zip dictionary -w passwords.txt
```

运行 `password-cracking --help` 或 `password-cracking man` 可查看完整命令参考。
攻击模式、退出码和检查点规则见[使用指南](docs/USAGE.zh-CN.md)。

## 性能

在 Apple M2 Pro（10 核）上使用 release 构建实测：

| 格式 | 加密方式 | 速度 |
|------|----------|------|
| ZIP | ZipCrypto | 约 1.78 亿密码/秒 |
| ZIP | AES-256 | 约 9.4 万密码/秒 |
| PDF | RC4-128 | 约 68.8 万密码/秒 |
| PDF | AES-256 R6 | 约 1.66 万密码/秒 |
| PDF（owner） | RC4-128 | 约 22 万密码/秒（`--pdf-owner`） |
| Office | Agile AES-256 | 600 到 800 密码/秒 |

Office 对每个候选执行约 10 万次 SHA-512 迭代。破解 Office 文件应优先使用字典
或混合攻击。加密参数和 CPU 型号都会改变实际速度。

## 工作原理

```text
CLI（clap）
  └── CrackerEngine
        ├── PasswordSource   字典 | 暴力 | Mask | 混合
        │     └── next_shard(max) / fill_batch(max) ──► 有界 FIFO 通道
        └── PasswordVerifier ZIP | PDF | Office
              ├── quick_check()          低成本过滤（ZIP ZipCrypto 与 AES）
              ├── verify_batch()         批量 / 4 路并行校验
              ├── verify_prefix_group()  共享前缀组校验
              └── verify()               完整密码校验
```

生成线程将紧凑分片（`WorkShard`）或扁平字节批次（`CandidateBatch`）通过有界
FIFO 通道交给固定数量的 worker 线程。FIFO 顺序让检查点有明确含义：文件保存的
是最早一个尚未完成批次的起点。批次大小在 32 到 65,536 之间自适应调整，目标是
每批约 250 毫秒。

`quick_check` 在完整解压前快速过滤 ZIP 候选：ZipCrypto 检查加密条目的 12 字节
加密头，WinZip AES 仅计算包含 2 字节校验值的单个 PBKDF2-HMAC-SHA1 块。PDF 和
Office 直接在 `verify_batch` / `verify_prefix_group` / `verify` 中执行批量与向量化校验。

## 文档

- [使用指南](docs/USAGE.zh-CN.md) / [English](docs/USAGE.md)
- [开发与发布](docs/DEVELOPMENT.zh-CN.md) / [English](docs/DEVELOPMENT.md)
- [测试文件](examples/README.zh-CN.md) / [English](examples/README.md)

## 限制

- Office：仅支持 Agile 加密。旧版 CryptoAPI/ECMA-376 文件会被拒绝。
- 不支持 RAR5 和 7z。
- PDF：空密码和证书加密不在支持范围内。
- ZIP：不支持分卷和多卷压缩包。
- 不支持 Windows。
- 字典检查点假设两次运行之间词表内容未变。

## 许可证

MIT，见 [LICENSE](LICENSE)。
