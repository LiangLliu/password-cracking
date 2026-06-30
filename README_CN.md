# 密码破解工具

[English](./README.md) | [中文文档](./README_CN.md)

用 Rust 编写的高性能文档密码破解工具，支持 ZIP、PDF、Office 文档的字典、暴力和混合攻击。

> **警告**：此工具仅供合法用途——恢复您自己的密码保护文件或授权的安全测试。请勿用于非法目的。

## 特性

- **多格式**：ZIP (ZipCrypto + AES)、PDF (RC4-40/128, AES-128/256)、Office (Agile AES-256)
- **多线程**：Rayon 工作窃取并行，利用所有 CPU 核心
- **三种攻击模式**：字典、暴力、混合（字典 + 规则变异）
- **自动检测**：通过文件魔数识别格式，不依赖扩展名
- **两阶段验证**：快速头部检查先排除 ~99% 错误密码，再做完整解密确认
- **跨平台**：Linux、macOS、Windows

## 快速开始

```bash
# 编译
git clone https://github.com/LiangLliu/password-cracking
cd password-cracking
cargo build --release

# 字典攻击
./target/release/password-cracking -f document.zip dictionary -w passwords.txt

# 暴力破解（4-6 位数字 PIN）
./target/release/password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 6

# 混合攻击（字典 + 规则变异）
./target/release/password-cracking -f document.docx hybrid -w dict.txt --capitalize --l33t --append-digits 99

# 静默模式（脚本中使用，只输出密码）
./target/release/password-cracking -q -f document.zip dictionary -w passwords.txt
```

## 性能

| 格式 | 加密类型 | 速度（密码/秒） |
|------|---------|---------------|
| ZIP | ZipCrypto | 7,000,000+ |
| PDF | RC4-128 | 500,000+ |
| Office | Agile AES-256 | 400+ |

性能取决于 CPU 核心数、加密类型和密码复杂度。

## 文档

- **[用户指南](docs/USER_GUIDE.md)** — 完整使用说明和示例
- **[项目结构](docs/PROJECT_STRUCTURE.md)** — 架构概述
- **[性能指南](docs/PERFORMANCE.md)** — 性能基准和优化建议
- **[ZIP 实现](docs/ZIP_IMPLEMENTATION.md)** — ZipCrypto 加密实现细节
- **[开发规范](docs/DEVELOPMENT.md)** — 编码规范和工作流

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

## 许可证

MIT — 详见 [LICENSE](LICENSE)

## 法律声明

此工具仅供合法用途使用。用户有责任遵守所有适用法律。作者不对任何滥用行为负责。
