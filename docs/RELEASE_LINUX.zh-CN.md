[English](RELEASE_LINUX.md) | **中文**

# 发布工程

介绍 Linux 和 macOS 产物如何构建、打包与发布。分发渠道和签名密钥设置见 [DISTRIBUTION.md](DISTRIBUTION.md)。

## 构建矩阵

`v*` tag（或手动 `workflow_dispatch` 运行）会构建 **8 个目标**：

| 产物 | 目标三元组 | 说明 |
|----------|---------------|-------|
| `password-cracking-linux-amd64.tar.gz` | `x86_64-unknown-linux-gnu` | glibc 2.39+ |
| `password-cracking-linux-amd64-musl.tar.gz` | `x86_64-unknown-linux-musl` | 静态，推荐 |
| `password-cracking-linux-arm64.tar.gz` | `aarch64-unknown-linux-gnu` | 交叉编译 |
| `password-cracking-linux-arm64-musl.tar.gz` | `aarch64-unknown-linux-musl` | 交叉编译，静态 |
| `password-cracking-linux-armv7.tar.gz` | `armv7-unknown-linux-gnueabihf` | Raspberry Pi 2+ |
| `password-cracking-linux-armv7-musl.tar.gz` | `armv7-unknown-linux-musleabihf` | 静态 |
| `password-cracking-macos-amd64.tar.gz` | `x86_64-apple-darwin` | Intel Mac |
| `password-cracking-macos-arm64.tar.gz` | `aarch64-apple-darwin` | Apple Silicon |

每个压缩包都包含位于根目录的 `password-cracking` 二进制文件，以及 `README.md` 和 `LICENSE`。压缩包在原生 runner（macOS）上构建，或使用 `cross` 构建（Linux 交叉目标）。

## 原生包

使用 [nfpm](https://nfpm.goreleaser.com/) 从 **musl** 二进制构建：

| 格式 | 架构 | 说明 |
|--------|---------------|-------|
| `.deb` | `amd64`、`arm64`、`armhf` | Debian、Ubuntu、Raspberry Pi OS |
| `.rpm` | `x86_64`、`aarch64`、`armv7hl` | Fedora、RHEL、openSUSE |
| `.apk` | `x86_64`、`aarch64`、`armv7` | Alpine |

ARMv7 在每种格式中名称不同；CI 会按打包器分别映射：

```text
deb: armv7 -> armhf
rpm: armv7 -> armv7hl
apk: armv7 -> armv7
```

包中包含二进制文件、man 手册页、bash/zsh/fish 补全以及许可证。没有维护者脚本；musl 构建的二进制是静态的。

## 为什么 musl 是主要的 Linux 二进制

- `x86_64-unknown-linux-gnu` 二进制链接到 runner 的 glibc。较旧的发行版可能因 `GLIBC_2.xx not found` 而失败。
- musl 构建完全静态（`file` 报告 “statically linked”），可在任何内核兼容的 x86_64/aarch64/armv7 Linux 上运行。
- 发行版包使用 musl 二进制，因此在 Debian 12 上安装的 `.deb` 与在 Fedora 上安装的 `.rpm` 行为完全一致。
- 静态二进制 stripped 后约 1.1 MB；scratch Docker 镜像约 1.7 MB。

GNU tarball 仍会发布，供偏好动态链接二进制的用户以及 AUR 包使用（Arch 提供较新的 glibc）。

## 发布流水线

```text
tag v0.2.1
  │
  ├── quality-check（fmt、clippy、清单模板）
  ├── security-audit（RustSec）
  ├── test（Ubuntu stable/beta、macOS stable）
  │
  ├── build × 8 ──► tarball + deb/rpm/apk + 每个文件的 .sha256
  │
  ├── GitHub Release ──► 39 个产物 + SHA256SUMS + install.sh
  │
  └── publish.yml
        ├── apt       gh-pages 上的签名仓库
        ├── homebrew  homebrew-tap 中的 formula
        ├── aur       PKGBUILD + PKGBUILD.SRCINFO（可选推送）
        ├── docker    GHCR 上的 linux/amd64 + linux/arm64
        └── crates    cargo publish（可选）
```

发布任务是可复用工作流（`workflow_call`），可通过 `gh workflow run publish.yml -f version=0.2.1` 手动运行。

## 验证

已针对 v0.2.1 执行：

1. **校验和**：所有发布产物均通过 `sha256sum -c SHA256SUMS`。
2. **APT**：Debian 容器，使用已发布的密钥验证签名，`apt install password-cracking` → `0.2.1`。
3. **Homebrew**：`brew install`、`brew test`、补全脚本和 man 手册页，在 macOS arm64 上使用真实 fixture 进行破解。
4. **Docker**：对 `0.2.1` 和 `latest` 执行 `docker run … --version`；多架构 manifest 列出 `linux/amd64` 和 `linux/arm64`。
5. **安装器**：`install.sh --prefix …` 校验 SHA-256 并安装 0.2.1。
6. **AUR**：`PKGBUILD` 通过 `bash -n`；校验和与发布 tarball 匹配。
7. **清单**：Homebrew 和 AUR 的哈希与已发布的产物匹配。

自动化容器测试尚未纳入 CI；参见[待完成工作](#remaining-work)。

## 发布检查清单

1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test`（撰写时 25 个单元测试 + 19 个 e2e 测试）
4. `cargo audit && cargo machete`
5. `bash scripts/test-package-manifests.sh`
6. `cargo publish --dry-run --allow-dirty`
7. 针对用户可见的变更更新英文和中文文档。
8. 提升 `Cargo.toml` 版本；运行 `cargo check` 更新 `Cargo.lock`。
9. 提交并运行 `./scripts/release.sh patch|minor|major|vX.Y.Z`。
10. 观察该 tag 的 CI 运行；确认 8 个构建任务、发布产物和发布任务。
11. 执行上面的验证步骤。
12. 宣布发布。

<a id="remaining-work"></a>
## 待完成工作

| 优先级 | 任务 | 说明 |
|----------|------|-------|
| P0 | CI 中的容器冒烟测试 | 每次发布时安装 `.deb`/`.rpm`/`.apk` |
| P1 | 产物签名 / 来源证明 | `actions/attest-build-provenance` 或 Sigstore |
| P1 | APT 验证任务 | 在 CI 中运行签名仓库安装 |
| P2 | AUR 提交 | `PKGBUILD` 已生成；提交一次后设置 `AUR_SSH_PRIVATE_KEY` |
| P2 | crates.io token | 设置 `CARGO_REGISTRY_TOKEN` |
| P3 | 可复现构建 | 固定工具链、`SOURCE_DATE_EPOCH` |
| P3 | 减小二进制体积 | 保持 `opt-level=3`；静态 musl 二进制约 1.1 MB |

已完成：8 目标矩阵、musl 包、Homebrew tap、GitHub Pages 上的 APT 仓库、GHCR 多架构镜像、crates.io 元数据与发布任务、按格式映射 ARMv7 架构、双语文档。

## 说明与风险

- `cross` 需要在 runner 上安装 Docker；GitHub 托管的 Ubuntu runner 已提供。版本由 `taiki-e/install-action` 固定。
- armv7 构建假定硬件浮点（Raspberry Pi 2+）。Raspberry Pi Zero/1（armv6）不在覆盖范围内。
- GNU Linux 二进制要求 glibc >= runner 的 glibc（当前为 2.39）。musl 构建避免了此问题。
- nfpm 版本传入时不带 `v` 前缀；工作流会将其去除。
- APT 元数据已签名（`InRelease`、`Release.gpg`）。单个 `.deb`/`.rpm` 文件未签名；仓库元数据是信任锚点。
- GitHub Release 是二进制的唯一来源。发布任务从其中下载，因此重新运行发布不会重新构建。
- GitHub Pages 提供 APT 仓库；一次构建通常在一分钟内完成。

## 备选方案：cargo-dist

`cargo-dist` 可以从 `[workspace.metadata.dist]` 生成压缩包、shell 安装器、Homebrew formula 和来源证明。

- **优点**：更少的定制 YAML，安装器一致。
- **缺点**：不支持原生 deb/rpm/apk 包（仍需要 nfpm），并且它会重写发布工作流。

建议：保留当前工作流和 nfpm。只有在发布工作流需要简化或需要 Homebrew-core formula 时，才重新考虑 `cargo-dist`。
