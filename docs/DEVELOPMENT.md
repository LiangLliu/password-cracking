# 开发规范

本文档定义了 Password Cracking 项目的开发规范和最佳实践。

## 目录

1. [环境设置](#环境设置)
2. [代码风格](#代码风格)
3. [Git 工作流](#git-工作流)
4. [测试规范](#测试规范)
5. [文档规范](#文档规范)
6. [安全规范](#安全规范)

## 环境设置

### 1. 安装开发工具

首次克隆项目后，运行以下命令设置开发环境：

```bash
# 安装 Git hooks 和开发工具
./scripts/install-hooks.sh
```

这将安装：
- **Git pre-commit hook**: 提交前自动检查代码
- **cargo-machete**: 检查未使用的依赖
- **cargo-audit**: 安全漏洞审计

### 2. 必需的 Rust 组件

确保安装以下 Rust 组件：

```bash
rustup component add rustfmt clippy
```

## 代码风格

### 1. 格式化

我们使用 `rustfmt` 进行代码格式化。配置文件位于项目根目录的 `rustfmt.toml`（如需自定义）。

**手动格式化：**
```bash
./scripts/fmt.sh
```

**仅检查格式：**
```bash
./scripts/fmt.sh --check
```

### 2. 代码质量

使用 `clippy` 进行代码质量检查：

```bash
cargo clippy -- -D warnings
```

或使用脚本自动修复：
```bash
./scripts/fmt.sh --fix
```

### 3. 命名规范

遵循 Rust 官方命名约定：

- **模块名**: `snake_case`
- **类型名**: `PascalCase`
- **函数名**: `snake_case`
- **常量**: `SCREAMING_SNAKE_CASE`
- **变量名**: `snake_case`

示例：
```rust
mod password_generator;

const MAX_PASSWORD_LENGTH: usize = 128;

struct PasswordCracker {
    thread_count: usize,
}

impl PasswordCracker {
    fn new() -> Self { ... }
    fn crack_password(&self, hash: &str) -> Option<String> { ... }
}
```

### 4. 代码组织

- 每个模块应有清晰的职责
- 使用 `mod.rs` 导出公共 API
- 将测试放在模块的 `#[cfg(test)]` 块中
- 保持函数简短，一般不超过 50 行

## Git 工作流

### 1. 提交规范

使用语义化提交信息：

```
<type>(<scope>): <subject>

<body>

<footer>
```

**类型（type）：**
- `feat`: 新功能
- `fix`: 修复 bug
- `docs`: 文档更新
- `style`: 格式调整（不影响代码运行）
- `refactor`: 重构（既不是新功能也不是修复 bug）
- `perf`: 性能优化
- `test`: 添加或修改测试
- `chore`: 构建过程或辅助工具的变动
- `ci`: CI/CD 相关的变更

**示例：**
```bash
git commit -m "feat(generator): add custom charset support for brute force"
git commit -m "fix(pdf): handle encrypted PDF v1.7 correctly"
git commit -m "docs: update installation instructions"
```

### 2. 分支管理

- `main/master`: 主分支，保持稳定
- `develop`: 开发分支（可选）
- `feature/*`: 功能分支
- `fix/*`: 修复分支
- `release/*`: 发布分支

### 3. Pull Request

提交 PR 前确保：
- [ ] 所有测试通过
- [ ] 代码已格式化
- [ ] Clippy 无警告
- [ ] 添加了必要的测试
- [ ] 更新了相关文档

## 测试规范

### 1. 单元测试

每个模块都应包含单元测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_generation() {
        // 测试代码
    }
}
```

### 2. 集成测试

在 `tests/` 目录创建集成测试：

```rust
// tests/integration_test.rs
use password_cracking::PasswordCracker;

#[test]
fn test_end_to_end_cracking() {
    // 端到端测试
}
```

### 3. 测试覆盖率

努力保持高测试覆盖率：
- 核心功能: > 90%
- 总体覆盖率: > 80%

运行覆盖率报告：
```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out html
```

## 文档规范

### 1. 代码文档

为所有公共 API 添加文档注释：

```rust
/// 生成指定长度的密码组合
///
/// # 参数
///
/// * `length` - 密码长度
/// * `charset` - 使用的字符集
///
/// # 返回值
///
/// 返回一个密码生成器迭代器
///
/// # 示例
///
/// ```
/// let generator = create_password_generator(8, "abc123");
/// ```
pub fn create_password_generator(length: usize, charset: &str) -> impl Iterator<Item = String> {
    // 实现
}
```

### 2. 模块文档

在每个模块顶部添加模块级文档：

```rust
//! # 密码生成器模块
//!
//! 该模块提供多种密码生成策略：
//! - 暴力破解生成器
//! - 字典攻击生成器
//! - 混合模式生成器
```

### 3. README 和其他文档

- 保持 README.md 更新
- 在 `docs/` 目录添加详细文档
- 使用 Markdown 格式

## 安全规范

### 1. 依赖管理

- 定期运行 `cargo audit` 检查安全漏洞
- 及时更新有安全问题的依赖
- 审查新添加的依赖

### 2. 敏感信息

- 不要在代码中硬编码密码、密钥或其他敏感信息
- 使用环境变量或配置文件（并添加到 `.gitignore`）
- 示例配置使用 `.example` 后缀

### 3. 错误处理

- 不要在错误信息中泄露敏感信息
- 使用 `anyhow` 或 `thiserror` 进行错误处理
- 记录足够的信息用于调试，但不包含敏感数据

## 持续改进

### 1. 代码审查

- 所有代码必须经过审查才能合并
- 使用 GitHub PR 进行代码审查
- 关注代码质量、性能和安全性

### 2. 性能优化

- 使用 `cargo bench` 进行性能测试
- 在优化前先进行基准测试
- 记录性能改进

### 3. 反馈和建议

欢迎提出改进建议：
- 通过 Issue 报告问题
- 通过 PR 贡献代码
- 参与讨论和设计决策

## 快速参考

### 常用命令

```bash
# 格式化和检查
./scripts/fmt.sh              # 格式化代码
./scripts/fmt.sh --check      # 只检查不修改
./scripts/fmt.sh --fix        # 自动修复问题

# 测试
cargo test                    # 运行所有测试
cargo test --doc             # 运行文档测试
cargo bench                  # 运行基准测试

# 构建
cargo build                  # 调试构建
cargo build --release        # 发布构建

# 发布
./scripts/release.sh patch   # 发布补丁版本
./scripts/release.sh minor   # 发布次版本
./scripts/release.sh major   # 发布主版本
```

### 故障排除

1. **Git hook 没有运行**
   ```bash
   ./scripts/install-hooks.sh
   ```

2. **跳过 Git hook（紧急情况）**
   ```bash
   git commit --no-verify
   ```

3. **清理构建缓存**
   ```bash
   cargo clean
   ```

---

遵循这些规范将帮助我们维护高质量、一致的代码库。如有疑问，请查阅 Rust 官方文档或在项目 Issue 中讨论。