# 密码破解工具

[English](./README.md) | [中文文档](./README_CN.md)

用 Rust 编写的高性能文档密码破解工具，支持 ZIP、PDF、Office 文档的字典、暴力和混合攻击。

> **警告**：此工具仅供合法用途——恢复您自己的密码保护文件或授权的安全测试。请勿用于非法目的。

## 特性

- **多格式**：ZIP (ZipCrypto + AES)、PDF (RC4-40/128, AES-128/256，支持 owner 密码)、Office (Agile AES-256)
- **四种攻击模式**：字典、暴力、Mask（`?d?d?l?l`）、混合（字典 + 规则变异）
- **高性能**：FIFO worker 线程池 + 自适应批次，ZIP 1100 万+ 密码/秒
- **自动检测**：通过文件魔数识别格式，不依赖扩展名
- **两阶段验证**：快速头部检查先排除 ~99% 错误密码，再做完整解密确认
- **断点续跑**：`--checkpoint`/`--resume`，中断后可继续
- **流式词表**：mmap 映射，超大词表内存平稳
- **Unix 友好**：man page、shell 补全、`.deb`/`.rpm`/`.apk`、安装脚本
- **跨平台**：Linux、macOS、Windows

## 快速开始

### 安装

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

# Arch Linux（AUR；包由 CI 生成，见 docs/DISTRIBUTION.md）
yay -S password-cracking-bin

# Windows（Scoop）
scoop bucket add lianglliu https://github.com/LiangLliu/scoop-bucket
scoop install password-cracking

# Docker
docker run --rm -v "$PWD:/data" ghcr.io/lianglliu/password-cracking --version

# 任意 Rust 平台（crates.io 尚未发布，目前从 git 安装）
cargo install --git https://github.com/LiangLliu/password-cracking

# 静态二进制脚本（Linux / macOS）
curl -fsSL https://raw.githubusercontent.com/LiangLliu/password-cracking/master/scripts/install.sh | sh
```

签名密钥、版本固定和问题排查见 [分发渠道](docs/DISTRIBUTION.md)。

### 使用

```bash
# 字典攻击
password-cracking -f document.zip dictionary -w passwords.txt

# 暴力破解（4-6 位数字 PIN）
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 6

# Mask 攻击（逐位字符集，92eo = 数字数字小写小写）
password-cracking -f document.zip mask -m "?d?d?l?l"

# 混合攻击（字典 + 规则变异）
password-cracking -f document.docx hybrid -w dict.txt --capitalize --l33t --append-digits 99

# 断点续跑
password-cracking -f document.pdf brute-force -c alnum --min-length 1 --max-length 8 \
  --checkpoint progress.ck --resume

# 静默模式（脚本中使用，只输出密码）
password-cracking -q -f document.zip dictionary -w passwords.txt
```

## 性能

| 格式 | 加密类型 | 速度（密码/秒） |
|------|---------|---------------|
| ZIP | ZipCrypto | 11,000,000+ |
| PDF | RC4-128 | 530,000+ |
| Office | Agile AES-256 | 500+ |

性能取决于 CPU 核心数、加密类型和密码复杂度。

## 文档

- **[用户指南](docs/USER_GUIDE.md)** — 完整使用说明和示例
- **[项目结构](docs/PROJECT_STRUCTURE.md)** — 架构概述
- **[性能指南](docs/PERFORMANCE.md)** — 性能基准和优化建议
- **[ZIP 实现](docs/ZIP_IMPLEMENTATION.md)** — ZipCrypto 加密实现细节
- **[分发渠道](docs/DISTRIBUTION.md)** — Homebrew、APT、AUR、Scoop、Docker、crates.io
- **[开发规范](docs/DEVELOPMENT.md)** — 编码规范和工作流
- **[发布工程](docs/RELEASE_LINUX.md)** — 构建矩阵、打包与发布流程

## 示例文件

`examples/` 目录包含密码为 `92eo` 的测试文件（ZIP、PDF、DOCX、XLSX、PPTX）：

```bash
# 生成测试文件（需要 Python + UV）
cd examples && uv run python create_test_files.py && cd ..

# 测试所有格式
./target/release/password-cracking -f examples/test.zip dictionary -w wordlists/common-passwords.txt
```

## 开发

```bash
# 代码检查
./scripts/fmt.sh

# 运行测试
cargo test

# 发布
./scripts/release.sh patch
```

## 限制

- **Office**：仅支持 Agile 加密（v4，Office 2010+）。旧版
  CryptoAPI/ECMA-376 标准加密（老 `.doc`/`.xls`/`.ppt`）暂不支持，会给出明确错误。
- **RAR5 / 7z**：暂未支持（已列入路线图）。
- **PDF**：支持 RC4/AES 的 user 与 owner 密码；空密码与证书加密不在范围内。
- **ZIP**：支持 AES 与 ZipCrypto；不支持分卷/多卷压缩包。
- **断点续跑**：字典攻击恢复时假设词表内容未变。

## 许可证

MIT — 详见 [LICENSE](LICENSE)

## 法律声明

此工具仅供合法用途使用。用户有责任遵守所有适用法律。作者不对任何滥用行为负责。
