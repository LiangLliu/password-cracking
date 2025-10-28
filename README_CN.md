# 密码破解工具

[English](./README.md) | [中文文档](./README_CN.md)

一个用 Rust 编写的高性能文档密码破解工具，支持多种文件格式和攻击模式。

⚠️ **警告**：此工具仅供合法用途，如恢复您自己的密码保护文件或授权的安全测试。请勿用于非法目的。

## ✨ 特性

- 🚀 **高性能**：多线程并行处理
- 📄 **多格式支持**：ZIP、PDF、Office 文档
- 🔧 **攻击模式**：字典、暴力破解、混合
- 📊 **实时进度**：速度、百分比和预计时间
- 🌍 **跨平台**：Windows、macOS 和 Linux

## 🚀 快速开始

### 安装

```bash
# 克隆并构建
git clone https://github.com/LiangLliu/password-cracking
cd password-cracking
cargo build --release

# 安装开发钩子（可选）
./scripts/install-hooks.sh
```

### 基本用法

```bash
# 字典攻击
password-cracking -f document.zip dictionary -w passwords.txt

# 暴力破解（4-6位数字密码）
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 6

# 混合攻击
password-cracking -f document.docx hybrid -w dictionary.txt -m append-digits
```

## 📚 文档

- **[用户指南](docs/USER_GUIDE_CN.md)** - 详细使用说明（中文）
- **[开发指南](docs/DEVELOPMENT.md)** - 设置和编码规范
- **[项目结构](docs/PROJECT_STRUCTURE.md)** - 架构概述
- **[CI/CD 指南](docs/CI-CD.md)** - 自动化工作流
- **[所有文档](docs/)** - 完整文档

## 🛠 开发

```bash
# 运行代码检查
./scripts/fmt.sh

# 运行测试
cargo test

# 创建发布
./scripts/release.sh patch
```

查看 [scripts/](scripts/) 了解所有可用的开发工具。

## 📦 示例文件

包含密码为 `92eo` 的测试文件：

```bash
cd examples
python create_test_files.py  # 生成测试文件
cd ..
./target/release/password-cracking -f examples/test.zip dictionary -w wordlists/common-passwords.txt
```

## ⚡ 性能

在 8 核 CPU 上的典型速度：
- 字典攻击：100K-500K 密码/秒
- 暴力破解（数字）：10M+ 密码/秒
- 混合攻击：10K-100K 密码/秒

## 🤝 贡献

欢迎贡献！请先阅读我们的[开发指南](docs/DEVELOPMENT.md)。

## 📄 许可证

MIT 许可证 - 详见 [LICENSE](LICENSE) 文件。

## ⚖️ 法律声明

此工具仅供合法用途使用。用户有责任遵守所有适用法律。作者不对任何滥用行为负责。