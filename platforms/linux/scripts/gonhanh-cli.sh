#!/bin/bash
# Gõ Nhanh CLI
# Usage: gn [command]

VERSION=$(cat ~/.local/share/gonhanh/version 2>/dev/null || echo "1.0.0")
CONFIG_DIR="$HOME/.config/gonhanh"
METHOD_FILE="$CONFIG_DIR/method"

# Colors
G='\033[0;32m' Y='\033[0;33m' B='\033[0;34m' N='\033[0m'

# Show status: ● BẬT │ telex or ○ TẮT │ telex
show_status() {
    METHOD=$(cat "$METHOD_FILE" 2>/dev/null || echo "telex")
    STATE=$(fcitx5-remote 2>/dev/null)
    if [[ "$STATE" == "2" ]]; then
        echo -e "${G}● BẬT${N} │ $METHOD"
    else
        echo -e "${Y}○ TẮT${N} │ $METHOD"
    fi
}

case "$1" in
    telex)
        mkdir -p "$CONFIG_DIR"
        echo "telex" > "$METHOD_FILE"
        fcitx5-remote -r 2>/dev/null || fcitx5 -r 2>/dev/null
        show_status
        ;;
    vni)
        mkdir -p "$CONFIG_DIR"
        echo "vni" > "$METHOD_FILE"
        fcitx5-remote -r 2>/dev/null || fcitx5 -r 2>/dev/null
        show_status
        ;;
    on)
        fcitx5-remote -o 2>/dev/null
        show_status
        ;;
    off)
        fcitx5-remote -c 2>/dev/null
        show_status
        ;;
    toggle|"")
        fcitx5-remote -t 2>/dev/null
        show_status
        ;;
    status)
        show_status
        ;;
    version|-v|--version)
        echo "Gõ Nhanh v$VERSION"
        ;;
    update)
        echo -e "${B}[*]${N} Đang cập nhật..."
        curl -fsSL https://raw.githubusercontent.com/khaphanspace/gonhanh.org/main/scripts/install-linux.sh | bash
        ;;
    help|-h|--help|*)
        echo -e "${B}Gõ Nhanh${N} v$VERSION - Vietnamese Input Method"
        echo ""
        echo "Cách dùng: gn [lệnh]"
        echo ""
        echo "Lệnh:"
        echo "  (không có)   Toggle bật/tắt"
        echo "  on           Bật tiếng Việt"
        echo "  off          Tắt tiếng Việt"
        echo "  telex        Chuyển sang Telex"
        echo "  vni          Chuyển sang VNI"
        echo "  status       Xem trạng thái"
        echo "  update       Cập nhật phiên bản mới"
        echo "  version      Xem phiên bản"
        echo "  help         Hiển thị trợ giúp"
        ;;
esac
