#!/bin/bash

# 代码格式化和检查脚本
# 使用方法: ./scripts/fmt.sh [选项]

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
NC='\033[0m' # No Color

# 打印带颜色的消息
print_step() {
    echo -e "${BLUE}[*]${NC} $1"
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

print_header() {
    echo -e "\n${MAGENTA}=== $1 ===${NC}\n"
}

# 解析命令行参数
CHECK_ONLY=false
FIX_ERRORS=false
SKIP_TESTS=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --check)
            CHECK_ONLY=true
            shift
            ;;
        --fix)
            FIX_ERRORS=true
            shift
            ;;
        --skip-tests)
            SKIP_TESTS=true
            shift
            ;;
        -h|--help)
            echo "使用方法: $0 [选项]"
            echo ""
            echo "选项:"
            echo "  --check       只检查，不修改文件"
            echo "  --fix         自动修复可以修复的问题"
            echo "  --skip-tests  跳过测试"
            echo "  -h, --help    显示帮助信息"
            exit 0
            ;;
        *)
            echo "未知选项: $1"
            echo "使用 $0 --help 查看帮助"
            exit 1
            ;;
    esac
done

# 检查是否安装了必要的工具
check_tool() {
    if ! command -v "$1" &> /dev/null; then
        print_error "$1 未安装"
        return 1
    fi
    return 0
}

print_header "检查开发工具"
MISSING_TOOLS=false

if ! check_tool "cargo"; then
    MISSING_TOOLS=true
fi

if ! check_tool "rustfmt"; then
    print_warning "rustfmt 未安装，正在安装..."
    rustup component add rustfmt
fi

if ! check_tool "cargo-clippy"; then
    print_warning "clippy 未安装，正在安装..."
    rustup component add clippy
fi

if $MISSING_TOOLS; then
    print_error "缺少必要的工具，请先安装"
    exit 1
fi

print_success "所有必要的工具已安装"

# 运行 cargo fmt
print_header "代码格式化 (rustfmt)"

if $CHECK_ONLY; then
    print_step "检查代码格式..."
    if cargo fmt --all -- --check; then
        print_success "代码格式正确"
    else
        print_error "代码格式不正确，请运行 './scripts/fmt.sh' 修复"
        exit 1
    fi
else
    print_step "格式化代码..."
    cargo fmt --all
    print_success "代码已格式化"
fi

# 运行 clippy
print_header "代码质量检查 (clippy)"

print_step "运行 clippy..."
if $FIX_ERRORS; then
    # 尝试自动修复
    if cargo clippy --all-targets --all-features --fix --allow-dirty --allow-staged -- -D warnings 2>/dev/null; then
        print_success "Clippy 检查通过（部分问题已自动修复）"
    else
        # 如果自动修复失败，仅运行检查
        if cargo clippy --all-targets --all-features -- -D warnings; then
            print_success "Clippy 检查通过"
        else
            print_error "Clippy 发现问题，请手动修复"
            exit 1
        fi
    fi
else
    if cargo clippy --all-targets --all-features -- -D warnings; then
        print_success "Clippy 检查通过"
    else
        print_error "Clippy 发现问题，请运行 './scripts/fmt.sh --fix' 尝试自动修复"
        exit 1
    fi
fi

# 检查未使用的依赖
print_header "依赖检查"

# 检查是否安装了 cargo-machete
if command -v cargo-machete &> /dev/null; then
    print_step "检查未使用的依赖..."
    if cargo machete; then
        print_success "没有发现未使用的依赖"
    else
        print_warning "发现未使用的依赖，请考虑移除"
    fi
else
    print_warning "cargo-machete 未安装，跳过依赖检查"
    print_warning "安装方法: cargo install cargo-machete"
fi

# 运行测试
if ! $SKIP_TESTS; then
    print_header "运行测试"

    print_step "运行单元测试..."
    if cargo test --all-features; then
        print_success "所有测试通过"
    else
        print_error "测试失败"
        exit 1
    fi

    print_step "运行文档测试..."
    if cargo test --doc; then
        print_success "文档测试通过"
    else
        print_error "文档测试失败"
        exit 1
    fi
fi

# 检查 TODO 和 FIXME
print_header "代码标记检查"

print_step "检查 TODO 和 FIXME 标记..."
TODO_COUNT=$(grep -rn "TODO\|FIXME" src/ 2>/dev/null | wc -l || echo "0")
if [ "$TODO_COUNT" -gt 0 ]; then
    print_warning "发现 $TODO_COUNT 个 TODO/FIXME 标记:"
    grep -rn "TODO\|FIXME" src/ --color=always | head -10 || true
    if [ "$TODO_COUNT" -gt 10 ]; then
        echo "... 还有 $((TODO_COUNT - 10)) 个标记"
    fi
else
    print_success "没有发现 TODO/FIXME 标记"
fi

# 安全审计
print_header "安全检查"

if command -v cargo-audit &> /dev/null; then
    print_step "运行安全审计..."
    if cargo audit; then
        print_success "没有发现已知的安全漏洞"
    else
        print_warning "发现安全问题，请查看上面的输出"
    fi
else
    print_warning "cargo-audit 未安装，跳过安全检查"
    print_warning "安装方法: cargo install cargo-audit"
fi

# 最终总结
echo ""
print_header "检查完成"

if $CHECK_ONLY; then
    print_success "所有检查完成！代码符合规范。"
else
    print_success "代码格式化和检查完成！"
fi

# 如果是在 Git hook 中运行，添加修改的文件
if [ -n "${GIT_HOOK_RUNNING:-}" ] && ! $CHECK_ONLY; then
    print_step "添加格式化后的文件到 Git..."
    git add -u
fi