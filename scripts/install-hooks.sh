#!/bin/bash

# Git hooks 安装脚本
# 使用方法: ./scripts/install-hooks.sh

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[✓]${NC} $1"
}

print_error() {
    echo -e "${RED}[✗]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[!]${NC} $1"
}

# 获取脚本所在目录
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"

print_info "安装 Git hooks..."

# 检查是否在 Git 仓库中
if [ ! -d "$PROJECT_ROOT/.git" ]; then
    print_error "当前目录不是 Git 仓库根目录"
    exit 1
fi

# 定义要安装的 hooks
HOOKS=(
    "pre-commit"
    # 可以在这里添加更多 hooks，如 "pre-push", "commit-msg" 等
)

# 创建 .git/hooks 目录（如果不存在）
mkdir -p "$PROJECT_ROOT/.git/hooks"

# 安装 hooks
INSTALLED=0
SKIPPED=0

for hook in "${HOOKS[@]}"; do
    SOURCE_HOOK="$PROJECT_ROOT/hooks/$hook"
    TARGET_HOOK="$PROJECT_ROOT/.git/hooks/$hook"

    if [ ! -f "$SOURCE_HOOK" ]; then
        print_warning "源 hook 文件不存在: $SOURCE_HOOK"
        continue
    fi

    if [ -f "$TARGET_HOOK" ] || [ -L "$TARGET_HOOK" ]; then
        # 检查是否已经是我们的 hook
        if [ -L "$TARGET_HOOK" ] && [ "$(readlink "$TARGET_HOOK")" = "$SOURCE_HOOK" ]; then
            print_info "$hook 已经安装"
            SKIPPED=$((SKIPPED + 1))
            continue
        fi

        print_warning "$hook 已存在"
        read -p "是否覆盖？(y/N) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            SKIPPED=$((SKIPPED + 1))
            continue
        fi

        # 备份现有的 hook
        BACKUP_FILE="$TARGET_HOOK.backup.$(date +%Y%m%d_%H%M%S)"
        mv "$TARGET_HOOK" "$BACKUP_FILE"
        print_info "已备份到: $BACKUP_FILE"
    fi

    # 创建符号链接
    ln -s "$SOURCE_HOOK" "$TARGET_HOOK"
    print_success "已安装 $hook"
    INSTALLED=$((INSTALLED + 1))
done

# 安装额外的开发工具（可选）
print_info "\n检查可选的开发工具..."

# cargo-machete - 检查未使用的依赖
if ! command -v cargo-machete &> /dev/null; then
    print_warning "cargo-machete 未安装（用于检查未使用的依赖）"
    read -p "是否安装 cargo-machete？(y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        print_info "正在安装 cargo-machete..."
        cargo install cargo-machete
        print_success "cargo-machete 已安装"
    fi
else
    print_success "cargo-machete 已安装"
fi

# cargo-audit - 安全审计
if ! command -v cargo-audit &> /dev/null; then
    print_warning "cargo-audit 未安装（用于安全审计）"
    read -p "是否安装 cargo-audit？(y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        print_info "正在安装 cargo-audit..."
        cargo install cargo-audit --locked
        print_success "cargo-audit 已安装"
    fi
else
    print_success "cargo-audit 已安装"
fi

# 总结
echo ""
print_info "安装完成！"
print_info "- 已安装: $INSTALLED 个 hooks"
print_info "- 已跳过: $SKIPPED 个 hooks"

echo ""
print_info "Git hooks 已启用，将在以下时机自动运行："
print_info "- pre-commit: 提交前自动检查和格式化代码"

echo ""
print_info "其他有用的命令："
print_info "- 手动运行格式化: ./scripts/fmt.sh"
print_info "- 只检查不修改: ./scripts/fmt.sh --check"
print_info "- 自动修复问题: ./scripts/fmt.sh --fix"
print_info "- 跳过 hook 提交: git commit --no-verify"

echo ""
print_success "开始享受自动化的代码规范检查吧！"