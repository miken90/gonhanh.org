#!/bin/bash
# Generate release notes using AI (opencode CLI)
# Usage: ./generate-release-notes.sh [version] [from-commit]
# Examples:
#   ./generate-release-notes.sh                    # từ last release đến HEAD
#   ./generate-release-notes.sh v1.0.18            # từ last release đến HEAD, version v1.0.18
#   ./generate-release-notes.sh v1.0.18 abc123     # từ commit abc123 đến HEAD

VERSION="${1:-next}"
FROM_REF="$2"

# Xác định điểm bắt đầu
if [ -z "$FROM_REF" ]; then
    FROM_REF=$(gh release view --json tagName -q .tagName 2>/dev/null || echo "")
fi

# Fallback nếu không có release
if [ -z "$FROM_REF" ]; then
    FROM_REF="HEAD~20"
fi

# Lấy danh sách commits
COMMITS=$(git log "$FROM_REF"..HEAD --pretty=format:"- %s" 2>/dev/null)

# Lấy diff summary (files changed + stats)
DIFF_STAT=$(git diff "$FROM_REF"..HEAD --stat 2>/dev/null)

# Lấy diff chi tiết (giới hạn để không quá dài)
DIFF_CONTENT=$(git diff "$FROM_REF"..HEAD --no-color 2>/dev/null | head -500)

if [ -z "$COMMITS" ] && [ -z "$DIFF_STAT" ]; then
    echo "Không tìm thấy thay đổi từ $FROM_REF đến HEAD"
    exit 1
fi

opencode run --format json "Tạo release notes cho version $VERSION của 'Gõ Nhanh' (Vietnamese IME for macOS).

## Commits:
$COMMITS

## Files changed:
$DIFF_STAT

## Code changes (snippet):
$DIFF_CONTENT

Quy tắc:
- Phân tích code changes để hiểu thay đổi thực sự, không chỉ dựa vào commit message
- Nhóm theo: ✨ Tính năng mới, 🐛 Sửa lỗi, ⚡ Cải thiện, 🔧 Khác
- Bỏ qua section rỗng
- Mỗi item: 1 dòng, súc tích, mô tả user-facing changes
- Viết tiếng Việt (có thể dùng keywords tiếng Anh như build, config, API...)
- Chỉ output markdown, không giải thích thêm" 2>/dev/null | jq -r 'select(.type == "text") | .part.text'
