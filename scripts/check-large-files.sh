#!/bin/bash

# 独立的大文件检查脚本
# 用于手动检查即将提交的大文件

set -e

# 颜色定义
YELLOW='\033[0;33m'
GREEN='\033[0;32m'
NC='\033[0m'

echo "检查暂存区中的大文件..."

LARGE_FILES=""
for file in $(git diff --cached --name-only); do
    if [ -f "$file" ]; then
        size=$(stat -f%z "$file" 2>/dev/null || stat -c%s "$file" 2>/dev/null || echo 0)
        if [ "$size" -gt 5242880 ]; then  # 5MB
            LARGE_FILES="${LARGE_FILES}${size} ${file}\n"
        fi
    fi
done

if [ -n "$LARGE_FILES" ]; then
    echo -e "${YELLOW}发现以下大文件（> 5MB）：${NC}"
    echo -e "$LARGE_FILES" | sort -nr | while read -r size file; do
        if [ -n "$size" ] && [ -n "$file" ]; then
            size_mb=$((size / 1048576))
            echo "  - $file (${size_mb}MB)"
        fi
    done
else
    echo -e "${GREEN}没有发现大文件${NC}"
fi