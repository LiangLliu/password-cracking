# 开发脚本说明

本目录包含项目开发和维护所需的各种脚本。

## 📋 脚本列表

### 1. fmt.sh - 代码格式化和检查
**用途**：一键运行所有代码质量检查，包括格式化、lint、测试等。

```bash
# 格式化并检查代码
./scripts/fmt.sh

# 仅检查，不修改
./scripts/fmt.sh --check

# 自动修复可修复的问题
./scripts/fmt.sh --fix

# 跳过测试
./scripts/fmt.sh --skip-tests
```

**功能**：
- ✅ 运行 rustfmt 格式化代码
- ✅ 运行 clippy 代码质量检查
- ✅ 检查未使用的依赖（需要 cargo-machete）
- ✅ 运行所有测试
- ✅ 检查 TODO/FIXME 标记
- ✅ 运行安全审计（需要 cargo-audit）

### 2. release.sh - 版本发布
**用途**：创建新版本并推送标签，触发自动发布流程。

```bash
# 发布补丁版本 (0.1.0 -> 0.1.1)
./scripts/release.sh patch

# 发布次版本 (0.1.0 -> 0.2.0)
./scripts/release.sh minor

# 发布主版本 (1.0.0 -> 2.0.0)
./scripts/release.sh major

# 发布指定版本
./scripts/release.sh v1.2.3
```

### 3. install-hooks.sh - Git Hooks 安装
**用途**：安装 Git hooks，启用提交前的自动代码检查。

```bash
# 安装 Git hooks
./scripts/install-hooks.sh
```

**安装的 hooks**：
- `pre-commit`: 提交前自动运行代码格式化和检查

### 4. check-large-files.sh - 大文件检查
**用途**：检查暂存区中是否有大文件（> 5MB）。

```bash
# 检查大文件
./scripts/check-large-files.sh
```

## 🛠 开发工具安装

某些脚本依赖额外的开发工具，可通过以下命令安装：

```bash
# 安装 cargo-machete（检查未使用的依赖）
cargo install cargo-machete

# 安装 cargo-audit（安全审计）
cargo install cargo-audit --locked
```

## 💡 使用建议

1. **日常开发**：定期运行 `./scripts/fmt.sh` 保持代码质量
2. **提交前**：Git hooks 会自动检查，但手动运行更快
3. **发布版本**：使用 `./scripts/release.sh` 确保版本号正确更新
4. **首次使用**：运行 `./scripts/install-hooks.sh` 安装 Git hooks