# 文档

[English](README.md) | **中文**

所有维护中的文档都提供英文和中文版本。英文文件名为 `NAME.md`，中文文件名为
`NAME.zh-CN.md`。

| 文档 | 内容 |
|------|------|
| [用户指南](USER_GUIDE.zh-CN.md) | 安装、攻击模式、选项、示例、排错 |
| [分发渠道](DISTRIBUTION.zh-CN.md) | Homebrew、APT、AUR、Docker、crates.io、签名密钥、验证 |
| [性能指南](PERFORMANCE.zh-CN.md) | 基准测试、密钥空间、引擎设计、调优 |
| [项目结构](PROJECT_STRUCTURE.zh-CN.md) | 模块布局、trait、数据流、依赖 |
| [开发指南](DEVELOPMENT.zh-CN.md) | 环境、代码风格、测试、Git 工作流、发布流程 |
| [CI/CD](CI-CD.zh-CN.md) | 工作流、构建矩阵、发布 job、secrets |
| [发布工程](RELEASE_LINUX.zh-CN.md) | Linux/macOS 产物、打包、检查清单、风险 |
| [ZIP 实现](ZIP_IMPLEMENTATION.zh-CN.md) | ZipCrypto/AES 验证、12 字节头检查 |
| [PDF 实现](PDF_IMPLEMENTATION.zh-CN.md) | RC4/AES、R5/R6 Algorithm 2.B、owner 密码、xref stream |
| [Office 实现](OFFICE_IMPLEMENTATION.zh-CN.md) | Agile 加密、哈希迭代、OLE2/XML 解析 |
| [测试文件](../examples/README.zh-CN.md) | fixture 列表、运行方式、重新生成 |

## 版本

本文档对应 **v0.2.1**。命令行选项以 `password-cracking --help` 和生成的 man
page 为准。

## 历史资料

`docs/superpowers/` 保存了早期迭代的设计与规划记录，仅用于追溯历史，**不再维护
也不翻译**。当前行为以源代码和上面的文档为准。
