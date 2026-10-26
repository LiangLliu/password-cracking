[English](DEVELOPMENT.md) | **中文**

# 开发指南

如何构建、测试、扩展和发布 `password-cracking`。

## 环境要求

- Rust **1.88+**（edition 2024，let-chains）
- `rustfmt` 和 `clippy`（`rustup component add rustfmt clippy`）
- Git
- 可选：`cargo-audit`、`cargo-machete`、Docker（交叉构建和打包）

## 环境搭建

```bash
git clone https://github.com/LiangLliu/password-cracking
cd password-cracking
./scripts/install-hooks.sh   # optional pre-commit hook
cargo build --release
cargo test
```

pre-commit 钩子会对已暂存的 Rust 变更运行格式化、clippy、测试、`cargo-audit` 和 `cargo-machete`。它使用 `git stash --keep-index`；如果出现 stash/pop 冲突，请解决冲突后手动运行检查。对于仅涉及文档的提交，可以使用 `git commit --no-verify`。

## 仓库结构

```
src/
  main.rs          process entry point, exit codes
  lib.rs           public modules
  cli/mod.rs       clap definitions and run()
  engine/mod.rs    worker pool, adaptive batches, checkpoints
  formats/         PasswordVerifier: mod.rs, zip.rs, pdf.rs, office.rs
  generators/      PasswordSource: dictionary, brute_force, mask, rules
  utils/mod.rs     charsets, formatting, validation
tests/e2e.rs       end-to-end tests over examples/
examples/          encrypted fixtures (password: 92eo) + generator
docs/              bilingual documentation
packaging/         Homebrew, AUR, APT, nfpm templates
scripts/           release, install, manifest and APT repo scripts
```

详细的模块地图见 [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md)。

## 常用命令

```bash
cargo build --release                 # optimized binary
cargo test                            # unit + e2e tests
cargo test --lib dictionary           # filter by name
cargo test --test e2e                 # e2e only
cargo fmt --all -- --check            # formatting
cargo clippy --all-targets -- -D warnings
cargo audit                           # RustSec advisories
cargo machete                         # unused dependencies
cargo publish --dry-run --allow-dirty # crate package verification
bash scripts/test-package-manifests.sh
actionlint .github/workflows/*.yml    # if installed
```

`./scripts/fmt.sh` 运行格式化 + clippy（除非指定 `--skip-tests`，否则也运行测试）；`./scripts/fmt.sh --check` 是只读检查。

## 架构不变量

这些不是风格偏好；改动它们可能破坏正确性，或影响性能。

1. **FIFO 批次顺序。** worker 线程从有界 FIFO 通道中取出批次。检查点前沿是尚未完成的最早批次的起点。LIFO 调度器（例如 Rayon 的工作窃取队列）会导致早期批次仍在等待而后续批次已完成，使前沿失去意义。
2. **以已验证前沿为准，而非已生成前沿。** 生成器在每个批次开始时记录数据源位置。只有完全验证的批次才推进检查点。绝不要写入生成器的当前位置：它可能远远超前于 worker 线程已检查的位置。
3. **自适应批次大小。** 共享目标值从 32 开始，根据批次耗时翻倍/减半，目标约为 250 ms，上限为 4096。慢速验证器（Office）需要小批次以及时写入检查点；快速验证器（ZIP）则受益于更大的批次。
4. **流式数据源。** `PasswordSource::fill_batch(batch, max)` 只可持有当前批次。不要收集整个密钥空间。
5. **仅在划算时使用两阶段验证。** `quick_check` 仅对 ZIP ZipCrypto 是真正的过滤器。PDF 和 Office 返回 `true`，因此引擎只运行一次 `verify`。

## 新增格式

1. 在 `src/formats/` 中实现 `PasswordVerifier`：

```rust
impl PasswordVerifier for MyVerifier {
    fn quick_check(&self, password: &[u8]) -> bool { /* cheap filter or true */ }
    fn verify(&self, password: &[u8]) -> bool { /* authoritative check */ }
    fn format_name(&self) -> &'static str { "MyFormat" }
    fn encryption_info(&self) -> &str { "..." }
}
```

2. 在 `src/formats/mod.rs` 中添加魔数字节检测和构造函数。
3. 在 `examples/` 中添加测试样本（除非另有理由，密码使用 `92eo`）。
4. 在 `tests/e2e.rs` 中添加端到端测试。
5. 更新 `README.md`、`USER_GUIDE.md` 及其中文版本。

## 新增生成器

1. 实现 `PasswordSource`：

```rust
impl PasswordSource for MySource {
    fn fill_batch(&mut self, batch: &mut Vec<Box<[u8]>>, max: usize) -> bool;
    fn estimated_total(&self) -> Option<u64>;
    fn checkpoint(&self) -> Option<String>;       // None if not resumable
    fn restore(&mut self, state: &str) -> Result<()>;
    fn name(&self) -> &'static str;
}
```

2. 在 `src/generators/mod.rs` 中为 `GeneratorMode` 添加变体并更新工厂。
3. 在 `src/cli/mod.rs` 中添加 CLI 子命令或标志。
4. 为顺序、边界、检查点往返和边界情况添加单元测试。
5. 使用测试样本添加端到端测试。

只要 `checkpoint()` 和 `restore()` 彼此一致，检查点格式可自由定义。引擎存储 `source_name`、状态以及规范化后的目标路径。

## 测试

- **单元测试**位于代码旁的 `#[cfg(test)] mod tests` 中。
- **端到端测试**位于 `tests/e2e.rs`，通过 `examples/` 中的测试样本驱动公共 API。
- 除仅 owner 密码的 PDF 测试样本外，每个测试样本都使用密码 **`92eo`**；后者需要 `VerifyOptions { try_pdf_owner: true }`（CLI：`--pdf-owner`）。
- 测试辅助函数：`wordlist_with_password()` 会写入一个临时字典文件。
- 任何落盘测试输入都应使用 `tempfile`。

```bash
cargo test
cargo test --test e2e
cargo test --lib generators::mask
```

## 代码风格

- 提交前运行 `cargo fmt` 和 `cargo clippy --all-targets -- -D warnings`。
- 命名遵循 Rust 约定：函数/模块使用 `snake_case`，类型使用 `PascalCase`，常量使用 `SCREAMING_SNAKE_CASE`。
- 应用代码中优先使用 `anyhow::Result`，面向用户的错误使用 `.context(...)`。
- 行为不明显时，用 `///` 为公共项保留文档注释。
- 不要静默吞掉错误。`let _ =` 仅可用于失败无碍的清理操作。
- 保持热点循环无内存分配。复用缓冲区；除非 API 要求，否则避免为每个候选分配 `Vec`。

## Git 工作流

使用语义化提交信息：

```
feat(formats): support AES-256 R6 PDFs
fix(engine): keep the checkpoint frontier on FIFO order
docs: rewrite the bilingual user guide
```

常见类型：`feat`、`fix`、`perf`、`refactor`、`test`、`docs`、`build`、`ci`、`chore`。

提交 PR 前：

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] 新行为有测试覆盖
- [ ] 英文和中文文档均已更新
- [ ] `cargo audit` / `cargo machete` 无告警

## 发布流程

1. 更新 `Cargo.toml` 中的版本（并通过 `cargo check` 更新 `Cargo.lock`）。
2. 在发布草稿或提交信息中更新 `CHANGELOG` 风格的说明。
3. 运行完整的本地检查套件。
4. 提交版本号变更。
5. `./scripts/release.sh patch|minor|major|vX.Y.Z` —— 创建并推送标签。
6. CI 构建 8 个目标，创建 GitHub Release，然后运行 `publish.yml`。
7. 验证：
   - `SHA256SUMS` 覆盖每个产物；
   - Homebrew、APT 和 Docker 能安装新版本；
   - AUR 产物和清单已附加。
8. 发布公告。

发布工作流可以重新运行，而无需重新构建：

```bash
gh workflow run publish.yml -f version=0.2.1
```

## 文档政策

- 每个维护中的文档都有英文 `NAME.md` 和中文 `NAME.zh-CN.md`。
- 两种版本必须在同一次变更中更新。不要让某一语言描述过时的 CLI 或已移除的平台。
- README 对（`README.md`、`README_CN.md`）遵循相同规则。
- `docs/superpowers/` 是早期设计工作的存档，有意不翻译，也不保持更新。
- 优先使用具体命令和实测数字，而非营销话术。

## 安全

- 本项目仅用于经授权的密码恢复和测试。
- 切勿提交凭据、私钥、令牌或真实用户数据。
- 发布所用的 CI 密钥：`APT_GPG_PRIVATE_KEY`、`HOMEBREW_TAP_DEPLOY_KEY`、可选的 `AUR_SSH_PRIVATE_KEY`、`CARGO_REGISTRY_TOKEN`。
- 私下报告漏洞；不要公开提交带有利用代码的 issue。
