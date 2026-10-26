# 开发指南

[English](DEVELOPMENT.md) | **中文**

## 环境

需要 Rust 1.88 或更高版本，以及 `rustfmt` 和 `clippy`：

```bash
rustup component add rustfmt clippy
git clone https://github.com/LiangLliu/password-cracking
cd password-cracking
cargo build --release
cargo test
```

`./scripts/install-hooks.sh` 会安装可选的 pre-commit 钩子，只检查暂存的 Rust
改动。如果钩子的 stash 操作与文档提交冲突，可以使用
`git commit --no-verify`。

## 目录

```text
src/
  main.rs          程序入口和退出码
  cli/mod.rs       clap 定义和 run()
  engine/mod.rs    FIFO worker 池、自适应批次、检查点
  formats/         ZIP、PDF 和 Office 验证器
  generators/      字典、暴力、Mask 和混合候选源
  utils/           字符集和格式化
tests/e2e.rs       examples/ 的端到端测试
examples/          加密测试文件和 Python 生成脚本
docs/              使用和开发文档
packaging/         Homebrew、AUR、APT 和 nfpm 模板
scripts/           发布、安装、manifest 和 APT 脚本
```

## 检查

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo audit
cargo machete
bash scripts/test-package-manifests.sh
cargo publish --dry-run --allow-dirty
```

`./scripts/fmt.sh` 会运行格式化、clippy 和测试。加上 `--check` 只检查，不修改
文件。

## 架构

- `PasswordSource` 生成候选。`fill_batch` 每次最多返回一批，不会把整个密钥
  空间放进内存。
- `PasswordVerifier` 校验候选。`quick_check` 是低成本过滤，`verify` 给出最终
  结果。
- 生成线程通过有界 FIFO 通道把批次交给固定数量的 worker 线程。FIFO 顺序让
  检查点可以保存最早一个未完成批次的起点。
- 引擎只在批次完成后写检查点。保存的是已验证前沿，不是生成器的当前位置。
- 批次大小从 32 开始，按约 250 毫秒的目标调整，上限为 4096。
- `quick_check` 只对 ZIP ZipCrypto 有效。PDF 和 Office 直接返回 `true`，避免
  重复执行昂贵的校验。

## 格式验证

- ZIP：读取所有加密条目。ZipCrypto 使用 12 字节头检查，AES 交给 `zip` crate。
  密码必须能打开所有加密条目。
- PDF：解析 trailer、`/Encrypt` 字典和 `/ID`，再检查 `/U` 哈希。
  `--pdf-owner` 还会检查 `/O`，耗时约为原来的三倍。
- Office：从 OLE2 容器读取 Agile `EncryptionInfo`，执行迭代哈希校验。候选按
  UTF-16LE 编码，因此支持 Unicode 密码。旧版 CryptoAPI/ECMA-376 文件会被拒绝。

## 添加格式

1. 在 `src/formats/` 中实现 `PasswordVerifier`。
2. 在 `src/formats/mod.rs` 中加入魔数识别和构造逻辑。
3. 在 `examples/` 添加测试文件，在 `tests/e2e.rs` 添加端到端测试。
4. 更新使用指南的中英文版本。

```rust
impl PasswordVerifier for MyVerifier {
    fn quick_check(&self, password: &[u8]) -> bool { true }
    fn verify(&self, password: &[u8]) -> bool { /* 最终校验 */ }
    fn format_name(&self) -> &'static str { "MyFormat" }
    fn encryption_info(&self) -> &str { "..." }
}
```

## 添加候选源

1. 实现 `PasswordSource`。
2. 在 `src/generators/mod.rs` 中添加 `GeneratorMode` 变体和工厂分支。
3. 在 `src/cli/mod.rs` 中添加子命令或选项。
4. 为顺序、边界和检查点往返添加单元测试。
5. 添加端到端测试。

`checkpoint()` 和 `restore()` 使用不透明字符串。引擎还会保存攻击模式名称和
规范化后的目标路径，续跑时如果不一致就拒绝启动。

## 测试

单元测试放在代码旁边的 `#[cfg(test)] mod tests` 中。端到端测试位于
`tests/e2e.rs`，使用 `examples/` 中的文件。

除 `test_unicode.docx` 使用 `密码92eo` 外，其他测试文件的密码都是 `92eo`。
只接受 owner 密码的 PDF 需要 `VerifyOptions { try_pdf_owner: true }` 或
`--pdf-owner`。

```bash
cargo test
cargo test --test e2e
cargo test --lib generators::mask
```

## 发布

1. 修改 `Cargo.toml` 中的版本号，运行 `cargo check` 更新 `Cargo.lock`。
2. 同步更新面向用户的中英文文档。
3. 运行上面的检查。
4. 提交版本变更。
5. 运行 `./scripts/release.sh patch`、`minor`、`major` 或 `vX.Y.Z`。

推送标签后会启动 8 个构建：6 个 Linux 目标和 2 个 macOS 目标。CI 创建 GitHub
Release，然后运行 `publish.yml` 更新各个软件包渠道。

仅修改文档时不要发布新版本。推送到 `master` 只运行检查，不会创建标签，因此
不会触发发布或更新软件包。

| 渠道 | 状态 |
|------|------|
| Homebrew | 已上线 |
| GitHub Pages 上的 APT 仓库 | 已上线，已签名 |
| GHCR 多架构镜像 | 已上线 |
| `install.sh` | 已上线 |
| GitHub Release 压缩包和安装包 | 已上线 |
| AUR `password-cracking-bin` | 已生成，待提交 |
| crates.io | 流水线就绪，需要 token |

不重新构建，直接重跑现有 release 的发布流程：

```bash
gh workflow run publish.yml -f version=0.2.1
```

发布任务从 GitHub Release 下载产物。未配置凭据的渠道会跳过，不会让整个发布
失败。

### Secrets

| Secret | 用途 |
|--------|------|
| `APT_GPG_PRIVATE_KEY` | 签名 APT 仓库元数据 |
| `HOMEBREW_TAP_DEPLOY_KEY` | 推送 formula 到 `homebrew-tap` |
| `AUR_SSH_PRIVATE_KEY` | 推送 AUR 包（可选） |
| `CARGO_REGISTRY_TOKEN` | 发布到 crates.io（可选） |

APT 公钥地址为
`https://lianglliu.github.io/password-cracking/key.gpg`。轮换密钥会破坏现有用户
配置，因此需要提前公告新密钥，并在过渡期保留旧密钥。

## 风格

- 提交前运行 `cargo fmt` 和 `cargo clippy --all-targets -- -D warnings`。
- 遵循 Rust 命名约定，热循环中不要为每个候选分配内存。
- 使用 `anyhow::Result` 和 `.context(...)` 返回用户能处理的错误。
- 使用语义化提交信息，例如 `fix(pdf): parse /EncryptMetadata` 或
  `docs: trim the usage guide`。
- 同一次修改中同时更新中英文文档。
- 优先写实测数字和具体命令，不写宽泛的结论。

## 安全

本项目仅用于授权的密码恢复和安全测试。不要提交凭据、私钥、token 或真实用户
数据。发现漏洞时请私下报告，不要在公开 issue 中附带利用代码。
