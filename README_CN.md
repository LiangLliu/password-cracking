# 密码破解工具

[中文文档](./README_CN.md) | [English](./README.md)

一个使用 Rust 编写的高性能文档密码破解工具，支持多种文件格式和攻击模式。

⚠️ **警告**: 此工具仅用于合法目的，如恢复自己忘记密码的文件或在获得授权的安全测试中使用。请勿用于非法目的。

## 特性

- 🚀 **高性能**: 使用 Rust 编写，多线程并行处理
- 📄 **多格式支持**: ZIP、PDF、Office (Word、Excel、PowerPoint)
- 🔧 **攻击模式**: 字典、暴力破解、混合
- 📊 **实时进度**: 显示速度、百分比和预计剩余时间
- 🎯 **智能优化**: 自动检测 CPU 核心数并调整批次大小
- ⚡ **性能模式**: 激进和平衡模式
- 🌍 **跨平台**: 支持 Windows、macOS 和 Linux

## 安装

### 从源码安装

```bash
# 克隆仓库
git clone https://github.com/LiangLliu/password-cracking
cd password-cracking

# 构建发布版本
cargo build --release

# 二进制文件位置: target/release/password-cracking
```

## 使用方法

### 基本语法

```bash
password-cracking -f <文件> <命令> [选项]
```

### 全局选项

- `-f, --file <文件>`: 目标文件（必需）
- `-t, --threads <线程数>`: 线程数（默认：自动检测）
- `-p, --performance <模式>`: 性能模式：balanced、aggressive（默认：aggressive）

### 命令

#### 1. 字典攻击

从字典文件或目录尝试密码：

```bash
# 单个文件
password-cracking -f document.zip dictionary -w passwords.txt

# 目录（递归加载所有 .txt 文件）
password-cracking -f document.zip dictionary -w wordlists/
```

选项：
- `-w, --wordlist <路径>`: 字典文件或目录路径（必需）

功能：
- 单文件模式：从一个文件加载密码
- 目录模式：递归扫描所有 `.txt` 文件
- 自动去重
- 跳过空行和注释（以 # 开头的行）

#### 2. 暴力破解

尝试所有可能的组合：

```bash
# 4-6 位数字 PIN
password-cracking -f document.pdf brute-force -c digits --min-length 4 --max-length 6

# 自定义字符集
password-cracking -f document.zip brute-force -c "abc123" --max-length 8
```

选项：
- `-c, --charset <字符集>`: 使用的字符集
- `--min-length <N>`: 最小密码长度（默认：1）
- `--max-length <N>`: 最大密码长度（必需）

可用字符集：
- `digits`: 0-9
- `lowercase`: a-z
- `uppercase`: A-Z
- `letters`: a-zA-Z
- `alnum`: a-zA-Z0-9
- `special`: !@#$%^&*()_+-=[]{}|;:,.<>?
- `all`: 所有可打印 ASCII 字符
- 自定义: 任意字符串（例如："abc123!@#"）

#### 3. 混合攻击

字典词汇加变体：

```bash
password-cracking -f document.xlsx hybrid -w base_words.txt
```

选项：
- `-w, --wordlist <文件>`: 基础字典（必需）

## 示例

### 常见用例

```bash
# 使用常见密码破解 ZIP
password-cracking -f archive.zip dictionary -w common-passwords.txt

# 破解 4 位数字 PIN 的 PDF
password-cracking -f invoice.pdf brute-force -c digits --min-length 4 --max-length 4

# 破解 6-8 位字母数字密码的 Excel
password-cracking -f data.xlsx brute-force -c alnum --min-length 6 --max-length 8

# 激进模式使用所有 CPU 核心
password-cracking -f file.zip -p aggressive dictionary -w wordlist.txt

# 平衡模式（保留 1 个核心）
password-cracking -f file.pdf -p balanced brute-force -c lowercase --max-length 6
```

## 性能提示

1. **字符集**: 更小的集合 = 更快的破解
   - 10 个字符（数字）: 10^n 种组合
   - 26 个字符（小写）: 26^n 种组合
   - 62 个字符（字母数字）: 62^n 种组合

2. **密码长度**: 时间呈指数增长
   - 4 个字符: 秒级
   - 6 个字符: 分钟到小时
   - 8+ 个字符: 天到年

3. **文件格式**（从快到慢）:
   - ZIP（传统）: 非常快
   - ZIP（AES）: 中等
   - PDF: 慢
   - Office: 非常慢

4. **CPU 使用**:
   - 激进模式: 使用所有核心
   - 平衡模式: 为系统保留 1 个核心

## 支持的格式

| 格式 | 扩展名 | 加密类型 | 速度 |
|------|--------|----------|------|
| ZIP | .zip | 传统 (ZipCrypto)、AES | 快/中等 |
| PDF | .pdf | RC4、AES-128、AES-256 | 慢 |
| Office | .docx、.xlsx、.pptx | Office 2007+ (AES) | 非常慢 |
| RAR | .rar | RAR3、RAR5（计划中） | - |

## 项目结构

```
password-cracking/
├── src/
│   ├── main.rs           # CLI 入口点
│   ├── cracker/          # 核心破解引擎
│   ├── formats/          # 文件格式处理器
│   │   ├── zip.rs        # ZIP 支持
│   │   ├── pdf.rs        # PDF 支持
│   │   └── office.rs     # Office 支持
│   ├── generator/        # 密码生成器
│   │   ├── brute_force.rs
│   │   └── dictionary.rs
│   └── utils/           # 工具函数
├── examples/            # 测试文件和脚本
│   ├── create_test_files.py  # 创建测试文件的脚本
│   ├── pyproject.toml        # UV 项目配置
│   └── test.zip              # 示例加密文件
├── wordlists/           # 密码字典
├── docs/               # 文档
└── Cargo.toml          # 依赖项
```

## 测试文件

`examples/` 目录包含用于验证工具的测试文件：

### 快速测试

```bash
cd examples

# 如需要，安装 UV（Python 包管理器）
curl -LsSf https://astral.sh/uv/install.sh | sh

# 创建测试文件（PDF、Word、Excel、PowerPoint）
uv sync
uv run python create_test_files.py

# 使用密码破解工具测试
cd ..
./target/release/password-cracking -f examples/test.zip dictionary -w wordlists/common-passwords.txt
```

所有测试文件使用密码：**92eo**

## 技术细节

- **语言**: Rust（内存安全、零成本抽象）
- **并行化**: Rayon 实现工作窃取并行
- **进度显示**: indicatif 实时进度条
- **ZIP**: 正确的 CRC32 验证以确保准确结果
- **跨平台**: 纯 Rust 实现，无平台特定代码

## 文档

- [性能分析](./docs/PERFORMANCE.md)
- [项目结构](./docs/PROJECT_STRUCTURE.md)
- [ZIP 实现详解](./docs/ZIP_IMPLEMENTATION.md)

## 贡献

欢迎贡献代码！请随时提交 Pull Request。

## 法律声明

本工具仅供合法用途使用。用户需承担使用本工具的所有法律责任。作者不对任何非法使用负责。

## 许可证

MIT 许可证 - 详见 LICENSE 文件