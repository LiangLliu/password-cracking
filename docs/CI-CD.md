# CI/CD 使用说明

## 概述

本项目使用 GitHub Actions 实现完整的 CI/CD 流程，包括：

- 持续集成（CI）：自动测试、代码检查
- 多平台构建：支持 Linux、macOS 的多种架构
- 自动发布：创建 GitHub Release 并上传构建产物

## 工作流说明

### 1. CI 工作流 (`.github/workflows/ci.yml`)

在推送、PR 和 tag 时运行：

- **Quality Check**：`cargo fmt --check`、`cargo clippy -D warnings`、包清单模板校验
- **Security Audit**：`rustsec/audit-check`
- **Test**：Ubuntu / macOS × stable（Ubuntu 额外跑 beta）
- **Build**：8 个目标三元组（见下表），交叉编译使用 `cross`
- **Create Release**：仅在 `v*` tag 时创建 GitHub Release
- **Publish packages**：调用 `publish.yml` 发布到各包管理器

构建矩阵：

| 平台 | 目标三元组 | 产物 | 发行包 |
|------|-----------|------|--------|
| Linux | `x86_64-unknown-linux-gnu` | `linux-amd64` | |
| Linux | `x86_64-unknown-linux-musl` | `linux-amd64-musl` | deb/rpm/apk |
| Linux | `aarch64-unknown-linux-gnu` | `linux-arm64` | |
| Linux | `aarch64-unknown-linux-musl` | `linux-arm64-musl` | deb/rpm/apk |
| Linux | `armv7-unknown-linux-gnueabihf` | `linux-armv7` | |
| Linux | `armv7-unknown-linux-musleabihf` | `linux-armv7-musl` | deb(armhf)/rpm(armv7hl)/apk(armv7) |
| macOS | `x86_64-apple-darwin` | `macos-amd64` | |
| macOS | `aarch64-apple-darwin` | `macos-arm64` | |

发行包只从 musl 构建生成：静态二进制不依赖发行版的 glibc 版本，
amd64/arm64/armv7 都能直接运行。

### 2. 发布工作流 (`.github/workflows/publish.yml`)

由 `ci.yml` 在 Release 创建后调用，也可手动运行：

```bash
gh workflow run publish.yml -f version=0.2.0
```

| Job | 目标 |
|-----|------|
| `apt` | 从 `.deb` 重建 APT 仓库，签名后推送到 `gh-pages` |
| `homebrew` | 渲染 formula，推送到 `homebrew-tap` |
| `aur` | 生成 PKGBUILD/.SRCINFO；配置密钥后推送到 AUR |
| `docker` | 构建 `linux/amd64` + `linux/arm64` 镜像并推送到 GHCR |
| `crates` | 发布到 crates.io（需要 `CARGO_REGISTRY_TOKEN`） |

所有 job 都从 GitHub Release 下载资产，因此可以安全重跑，不会重新编译。
未配置的渠道会跳过并给出提示，不会让整个发布失败。

渠道细节、签名密钥和一次性配置见 [DISTRIBUTION.md](DISTRIBUTION.md)。

### 3. Release Drafter (`.github/workflows/release-drafter.yml`)

自动生成 Release 草稿和更新日志。

## 使用方法

### 日常开发

1. **提交代码**：正常提交代码到分支
2. **创建 PR**：CI 会自动运行测试和检查
3. **合并代码**：确保所有检查通过后合并

### 发布新版本

#### 方法一：使用发布脚本（推荐）

```bash
# 发布补丁版本 (0.0.1 -> 0.0.2)
./scripts/release.sh patch

# 发布次版本 (0.1.0 -> 0.2.0)
./scripts/release.sh minor

# 发布主版本 (1.0.0 -> 2.0.0)
./scripts/release.sh major

# 发布指定版本
./scripts/release.sh v1.2.3
```

#### 方法二：手动创建标签

```bash
# 创建标签
git tag -a v1.0.0 -m "Release v1.0.0"

# 推送标签
git push origin v1.0.0
```

### 查看构建状态

在 README.md 中添加状态徽章：

```markdown
[![CI](https://github.com/YOUR_USERNAME/password-cracking/actions/workflows/ci.yml/badge.svg)](https://github.com/YOUR_USERNAME/password-cracking/actions/workflows/ci.yml)
[![Release](https://github.com/YOUR_USERNAME/password-cracking/actions/workflows/release.yml/badge.svg)](https://github.com/YOUR_USERNAME/password-cracking/actions/workflows/release.yml)
```

## 配置要求

### Secrets

大部分功能使用默认的 `GITHUB_TOKEN`，无需额外配置。

可选的 Secrets：
- `CODECOV_TOKEN`：用于上传代码覆盖率到 Codecov

### 权限

确保 GitHub Actions 有以下权限：
- 创建 Release
- 上传 Release 资产
- 读写仓库内容

## 维护

### 更新依赖

Dependabot 会自动创建 PR 更新依赖：
- Rust 依赖：每周一检查
- GitHub Actions：每周一检查

### 添加新平台

编辑 `.github/workflows/build.yml` 和 `.github/workflows/release.yml`，在 `matrix` 中添加新的目标平台。

## 故障排除

### CI 失败

1. **格式检查失败**：运行 `cargo fmt`
2. **Clippy 失败**：运行 `cargo clippy --fix`
3. **测试失败**：检查测试日志，修复失败的测试

### 构建失败

1. **交叉编译失败**：确保 `cross` 工具正确安装
2. **缓存问题**：可能需要清理 GitHub Actions 缓存

### 发布失败

1. **权限问题**：检查 GitHub token 权限
2. **标签冲突**：确保标签不存在

## 最佳实践

1. **版本号**：遵循语义化版本 (SemVer)
2. **提交信息**：使用规范的提交信息格式
3. **分支保护**：启用主分支保护，要求 PR 和 CI 通过
4. **定期更新**：及时处理 Dependabot 的更新 PR

## 参考资源

### GitHub Actions 官方文档
- [GitHub Actions 文档](https://docs.github.com/en/actions)
- [工作流语法参考](https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions)
- [使用矩阵策略](https://docs.github.com/en/actions/using-jobs/using-a-matrix-for-your-jobs)

### Rust 特定资源
- [dtolnay/rust-toolchain](https://github.com/dtolnay/rust-toolchain) - Rust 工具链安装
- [cross-rs/cross](https://github.com/cross-rs/cross) - 交叉编译工具
- [cargo-audit](https://github.com/rustsec/audit-check) - 安全审计

### 参考项目
- [ripgrep](https://github.com/BurntSushi/ripgrep) - 多平台构建的优秀示例
- [tokio](https://github.com/tokio-rs/tokio) - 复杂的测试矩阵配置
- [bat](https://github.com/sharkdp/bat) - 使用 cross 进行交叉编译