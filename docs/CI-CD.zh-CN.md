[English](CI-CD.md) | **中文**

# CI/CD

该项目使用三个 GitHub Actions 工作流。所有发布工作都由单次 tag 推送驱动。

## 工作流

### 1. `ci.yml` — CI 与发布

触发条件：

- 推送到 `main`、`master` 或 `develop`
- 匹配 `v*.*.*` 的 tag
- 以这些分支为目标的 pull request
- 手动 `workflow_dispatch`（无需 tag 即可测试构建任务）

任务：

| 任务 | 运行于 | 作用 |
|-----|---------|--------------|
| `quality-check` | 每次触发 | `cargo fmt --check`、`cargo clippy -D warnings`、manifest 模板冒烟测试 |
| `security-audit` | 每次触发 | `rustsec/audit-check` |
| `test` | 每次触发 | Ubuntu stable + beta、macOS stable |
| `build` | tag 和手动运行 | 8 目标矩阵（6 个 Linux + 2 个 macOS） |
| `release` | 仅 tag | 创建 GitHub Release 并上传产物 |
| `publish` | 仅 tag | 发布后调用 `publish.yml` |

构建矩阵：

| 平台 | 目标 | 产物 | 包 |
|----------|--------|----------|----------|
| Linux | `x86_64-unknown-linux-gnu` | `linux-amd64` | |
| Linux | `x86_64-unknown-linux-musl` | `linux-amd64-musl` | deb/rpm/apk |
| Linux | `aarch64-unknown-linux-gnu` | `linux-arm64` | |
| Linux | `aarch64-unknown-linux-musl` | `linux-arm64-musl` | deb/rpm/apk |
| Linux | `armv7-unknown-linux-gnueabihf` | `linux-armv7` | |
| Linux | `armv7-unknown-linux-musleabihf` | `linux-armv7-musl` | deb(armhf)/rpm(armv7hl)/apk(armv7) |
| macOS | `x86_64-apple-darwin` | `macos-amd64` | |
| macOS | `aarch64-apple-darwin` | `macos-arm64` | |

包仅从 musl 二进制构建，因此三种格式不会在同一架构名上发生冲突。

### 2. `publish.yml` — 分发

由 `ci.yml` 调用的可复用工作流，也可手动运行：

```bash
gh workflow run publish.yml -f version=0.2.1
```

| 任务 | 目标 |
|-----|--------|
| `prepare` | 规范化 `v0.2.1` → `0.2.1` |
| `apt` | 重建已签名的 APT 仓库并推送 `gh-pages` |
| `homebrew` | 渲染 formula 并推送 `homebrew-tap` |
| `aur` | 渲染 `PKGBUILD` / `PKGBUILD.SRCINFO`；若已配置则推送 |
| `docker` | 构建 `linux/amd64` + `linux/arm64` 并推送 GHCR |
| `crates` | 配置了 token 时执行 `cargo publish` |

所有任务都从 GitHub Release 下载产物，因此重新运行不会重新构建二进制文件。缺失凭据只会产生提示，不会导致失败。

### 3. `release-drafter.yml`

用已合并 PR 的标题和标签更新草稿发布。它不会发布任何内容。

## 发布流程

```bash
# 推荐：更新 Cargo.toml、提交、打 tag 并推送
./scripts/release.sh patch     # 0.2.1 -> 0.2.1
./scripts/release.sh minor     # 0.2.1 -> 0.3.0
./scripts/release.sh major     # 0.3.0 -> 1.0.0
./scripts/release.sh v0.4.0    # 显式指定版本
```

手动等价操作：

```bash
# 更新 Cargo.toml，运行 cargo check，然后提交
git tag -a v0.2.1 -m "Release v0.2.1"
git push origin v0.2.1
```

推送 tag 会触发 `ci.yml` → `build` → `release` → `publish`。可使用以下命令观察：

```bash
gh run list --workflow=ci.yml --limit 5
gh run watch
```

无需重新构建即可重新发布已有版本：

```bash
gh workflow run publish.yml -f version=0.2.1
```

## Secrets

| Secret | 是否必需 | 用途 |
|--------|----------|---------|
| `GITHUB_TOKEN` | 自动 | 发布产物、`gh-pages`、GHCR |
| `APT_GPG_PRIVATE_KEY` | 用于签名 | 签名 `InRelease` 和 `Release.gpg` |
| `HOMEBREW_TAP_DEPLOY_KEY` | 用于 Homebrew | 对 `homebrew-tap` 的写权限 |
| `AUR_SSH_PRIVATE_KEY` | 可选 | 将 `password-cracking-bin` 推送到 AUR |
| `CARGO_REGISTRY_TOKEN` | 可选 | 发布到 crates.io |

`publish.yml` 使用的 `GITHUB_TOKEN` 权限：

```yaml
contents: write   # 发布产物与 gh-pages
packages: write   # GHCR
pages: write      # 启用/刷新 Pages
```

## 缓存与性能

- Cargo registry 和 `target/` 按 OS/target 缓存，缓存键为 `Cargo.lock`。
- `cross` 镜像会为 Linux 交叉目标按需拉取。
- 在 GitHub 托管的 runner 上，完整矩阵大约需要 5–8 分钟。
- musl 任务最慢，因为它们运行在 cross 容器内。

## Dependabot

Dependabot 每周检查 Cargo 和 GitHub Actions 并提交 PR。当有多个更新待处理时，在 `master` 上以单个提交应用这些更新，然后引用该提交关闭这些 PR。

## 故障排查

| 现象 | 修复 |
|---------|-----|
| `Quality Check` 因格式检查失败 | `cargo fmt --all` |
| `Quality Check` 因 clippy 失败 | `cargo clippy --all-targets -- -D warnings` |
| Manifest 冒烟测试失败 | 检查 `scripts/gen-package-manifests.py` 的输出 |
| 交叉构建失败 | 检查该目标对应的 `cross` 镜像；重新运行该任务 |
| 发布任务找不到产物 | 确保每个构建任务都上传了产物 |
| 发布任务无法下载 GitHub Release | 该发布必须存在且不是草稿 |
| APT 任务推送失败 | 检查 `pages: write` 权限和分支保护 |
| Homebrew 任务克隆失败 | 检查 `HOMEBREW_TAP_DEPLOY_KEY` 和部署密钥 |
| GHCR 推送未授权 | 检查 `packages: write` 和仓库设置 |

## 徽章

```markdown
[![CI](https://github.com/LiangLliu/password-cracking/actions/workflows/ci.yml/badge.svg)](https://github.com/LiangLliu/password-cracking/actions/workflows/ci.yml)
```

## 说明

- 不构建也不测试 Windows。工作流有意不包含 `windows-latest` runner。
- 发布任务由 tag 门控；分支推送只运行质量检查、审计和测试。
- `workflow_dispatch` 可以在不创建发布的情况下运行构建矩阵，这对于验证打包变更很有用。
