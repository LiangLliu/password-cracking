# 文档密码破解工具

[English](README.md) | **中文**

用 Rust 编写的高性能文档密码破解工具。支持 ZIP、PDF 和 Office 文件，提供
字典、暴力、Mask 和混合（字典 + 规则）四种攻击模式，完全在 CPU 本地运行，
不依赖任何外部服务。

> **仅限合法用途。** 本工具用于找回你自己的加密文件，或在获得授权的安全测试中
> 使用。请勿用于你无权访问或未获授权测试的数据。

## 特性

- **格式**：ZIP（ZipCrypto、AES-128/192/256）、PDF（RC4-40/128、AES-128、
  AES-256 R5/R6，支持 user 与 owner 密码）、Office（Agile AES-128/192/256）
- **四种攻击模式**：字典、暴力、Mask（`?d?d?l?l`）、混合（字典 + 变异规则）
- **高性能**：FIFO worker 线程池 + 自适应批次，10 核笔记本 CPU 上 ZIP 可达
  1100 万+ 密码/秒
- **流式处理**：词表使用 mmap，候选按批生成，超大输入下内存保持平稳
- **断点续跑**：`--checkpoint` / `--resume`，不会重复验证已完成的前缀
- **格式识别**：基于文件魔数，不只依赖扩展名
- **两阶段验证**：ZIP 的 12 字节头检查可在完整解密前排除几乎所有错误密码
- **Linux 与 macOS**：Linux 静态 musl 构建、macOS 原生构建、man page、shell
  补全、`.deb`/`.rpm`/`.apk`、Homebrew 和 Docker

## 安装

```bash
# Homebrew（macOS / Linux）
brew install LiangLliu/tap/password-cracking

# Debian / Ubuntu / 树莓派系统（签名 APT 仓库）
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

Arch Linux（`password-cracking-bin`）和 crates.io 发布流程已就绪，但需要一次性
配置 AUR 账号或 crates.io token。当前状态、签名密钥和排错说明见
[分发渠道](docs/DISTRIBUTION.zh-CN.md)。

## 使用

```bash
# 字典攻击
password-cracking -f document.zip dictionary -w passwords.txt

# 暴力破解（4-6 位数字 PIN）
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 6

# Mask：两位数字 + 两位小写字母（92eo）
password-cracking -f document.zip mask -m "?d?d?l?l"

# 混合：字典 + 规则变异
password-cracking -f document.docx hybrid -w dict.txt --capitalize --l33t --append-digits 99

# 断点续跑
password-cracking -f document.pdf brute-force -c alnum --min-length 1 --max-length 8 \
  --checkpoint progress.ck --resume

# 静默模式（脚本使用，只输出密码）
password-cracking -q -f document.zip dictionary -w passwords.txt
```

## 性能

Apple M2 Pro（10 核 CPU）release 构建实测：

| 格式 | 加密 | 速度 |
|------|------|------|
| ZIP | ZipCrypto | 1100 万–1150 万 密码/秒 |
| PDF | RC4-128 | 51 万–54 万 密码/秒 |
| PDF（owner） | RC4-128 | 约 15 万 密码/秒（`--pdf-owner`） |
| Office | Agile AES-256 | 500–800 密码/秒 |

Office 慢是算法设计使然：每个候选密码需要约 10 万次 SHA-512 迭代。Office 文件
应优先使用字典或混合攻击。密钥空间表和调优建议见
[性能指南](docs/PERFORMANCE.zh-CN.md)。

## 工作原理

```
CLI（clap）
  └── CrackerEngine
        ├── PasswordSource   字典 | 暴力 | Mask | 混合
        │     └── fill_batch(max) ──► 有界 FIFO 通道
        └── PasswordVerifier ZIP | PDF | Office
              ├── quick_check()  低成本过滤（仅 ZIP）
              └── verify()       完整密码校验
```

- 生成线程把有界批次送入固定数量的 worker 线程。
- 批次大致按顺序完成，因此检查点前沿有意义：保存的位置是最早一个尚未完整
  验证的批次的起点。
- 批次大小在 32–4096 之间自适应，目标每批约 250 ms：慢格式能及时写检查点，
  快格式能摊薄同步开销。

## 文档

| 文档 | English | 中文 |
|------|---------|------|
| 用户指南 | [USER_GUIDE.md](docs/USER_GUIDE.md) | [USER_GUIDE.zh-CN.md](docs/USER_GUIDE.zh-CN.md) |
| 分发渠道 | [DISTRIBUTION.md](docs/DISTRIBUTION.md) | [DISTRIBUTION.zh-CN.md](docs/DISTRIBUTION.zh-CN.md) |
| 性能 | [PERFORMANCE.md](docs/PERFORMANCE.md) | [PERFORMANCE.zh-CN.md](docs/PERFORMANCE.zh-CN.md) |
| 架构 | [PROJECT_STRUCTURE.md](docs/PROJECT_STRUCTURE.md) | [PROJECT_STRUCTURE.zh-CN.md](docs/PROJECT_STRUCTURE.zh-CN.md) |
| 开发 | [DEVELOPMENT.md](docs/DEVELOPMENT.md) | [DEVELOPMENT.zh-CN.md](docs/DEVELOPMENT.zh-CN.md) |
| CI/CD | [CI-CD.md](docs/CI-CD.md) | [CI-CD.zh-CN.md](docs/CI-CD.zh-CN.md) |
| 发布工程 | [RELEASE_LINUX.md](docs/RELEASE_LINUX.md) | [RELEASE_LINUX.zh-CN.md](docs/RELEASE_LINUX.zh-CN.md) |
| ZIP 实现 | [ZIP_IMPLEMENTATION.md](docs/ZIP_IMPLEMENTATION.md) | [ZIP_IMPLEMENTATION.zh-CN.md](docs/ZIP_IMPLEMENTATION.zh-CN.md) |
| PDF 实现 | [PDF_IMPLEMENTATION.md](docs/PDF_IMPLEMENTATION.md) | [PDF_IMPLEMENTATION.zh-CN.md](docs/PDF_IMPLEMENTATION.zh-CN.md) |
| Office 实现 | [OFFICE_IMPLEMENTATION.md](docs/OFFICE_IMPLEMENTATION.md) | [OFFICE_IMPLEMENTATION.zh-CN.md](docs/OFFICE_IMPLEMENTATION.zh-CN.md) |
| 测试文件 | [examples/README.md](examples/README.md) | [examples/README.zh-CN.md](examples/README.zh-CN.md) |

## 限制

- **Office**：仅支持 Agile 加密（v4，Office 2010+）。旧版 CryptoAPI/ECMA-376
  标准加密会给出明确错误。
- **RAR5 / 7z**：暂不支持。
- **PDF**：支持 RC4/AES 的 user 与 owner 密码；空密码和证书加密不在范围内。
- **ZIP**：支持 ZipCrypto 和 AES；不支持分卷/多卷压缩包。
- **Windows**：不支持。代码本身可移植，但构建和发行只针对 Linux 与 macOS。
- **字典断点续跑**：假设两次运行之间词表内容未变。

## 许可证

MIT —— 见 [LICENSE](LICENSE)。
