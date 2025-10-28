#!/bin/bash

# 发布脚本 - 用于创建新版本并推送 tag
# 使用方法: ./scripts/release.sh [major|minor|patch|version]

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 打印带颜色的消息
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# 检查是否在 git 仓库中
if ! git rev-parse --git-dir > /dev/null 2>&1; then
    print_error "当前目录不是一个 Git 仓库"
    exit 1
fi

# 检查是否有未提交的更改
if ! git diff --quiet || ! git diff --cached --quiet; then
    print_error "存在未提交的更改，请先提交或暂存"
    exit 1
fi

# 获取当前版本
current_version=$(git describe --tags --abbrev=0 2>/dev/null || echo "v0.0.0")
print_info "当前版本: $current_version"

# 解析版本号
version_without_v=${current_version#v}
IFS='.' read -ra VERSION_PARTS <<< "$version_without_v"
major="${VERSION_PARTS[0]:-0}"
minor="${VERSION_PARTS[1]:-0}"
patch="${VERSION_PARTS[2]:-0}"

# 根据参数确定新版本
case "$1" in
    major)
        major=$((major + 1))
        minor=0
        patch=0
        ;;
    minor)
        minor=$((minor + 1))
        patch=0
        ;;
    patch)
        patch=$((patch + 1))
        ;;
    v*)
        # 直接指定版本号
        new_version="$1"
        ;;
    *)
        print_error "无效的参数: $1"
        echo "使用方法: $0 [major|minor|patch|v1.2.3]"
        exit 1
        ;;
esac

# 如果没有直接指定版本，则构建新版本号
if [ -z "$new_version" ]; then
    new_version="v${major}.${minor}.${patch}"
fi

print_info "新版本: $new_version"

# 确认发布
read -p "确定要发布版本 $new_version 吗? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    print_warning "发布已取消"
    exit 0
fi

# 拉取最新代码
print_info "拉取最新代码..."
git pull origin main || git pull origin master

# 更新 Cargo.toml 中的版本号（如果需要）
if [ -f "Cargo.toml" ]; then
    print_info "更新 Cargo.toml 版本号..."
    # 提取版本号（去掉 v 前缀）
    cargo_version=${new_version#v}
    # 使用 sed 更新版本号
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        sed -i '' "s/^version = \".*\"/version = \"$cargo_version\"/" Cargo.toml
    else
        # Linux
        sed -i "s/^version = \".*\"/version = \"$cargo_version\"/" Cargo.toml
    fi

    # 运行 cargo check 确保版本更新正确
    print_info "检查 Cargo.toml..."
    cargo check

    # 提交版本更新
    git add Cargo.toml Cargo.lock
    git commit -m "chore: bump version to $new_version" || true
fi

# 创建并推送 tag
print_info "创建 tag: $new_version"
git tag -a "$new_version" -m "Release $new_version"

print_info "推送 tag 到远程仓库..."
git push origin "$new_version"

# 推送代码更改（如果有）
if git status --porcelain | grep -q "^"; then
    git push origin main || git push origin master
fi

print_success "版本 $new_version 已成功发布！"
print_info "GitHub Actions 将自动构建并创建 Release"
print_info "您可以在以下地址查看进度："
print_info "https://github.com/$(git remote get-url origin | sed 's/.*github.com[:\/]\(.*\)\.git/\1/')/actions"