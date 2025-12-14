#!/bin/bash
# Gõ Nhanh CLI - Simple commands for Vietnamese input
# Usage: gn [telex|vni|on|off|toggle|status]

CONFIG_DIR="$HOME/.config/gonhanh"
METHOD_FILE="$CONFIG_DIR/method"

case "$1" in
    telex)
        mkdir -p "$CONFIG_DIR"
        echo "telex" > "$METHOD_FILE"
        fcitx5-remote -r 2>/dev/null || fcitx5 -r 2>/dev/null
        echo "✓ Đã chuyển sang Telex"
        ;;
    vni)
        mkdir -p "$CONFIG_DIR"
        echo "vni" > "$METHOD_FILE"
        fcitx5-remote -r 2>/dev/null || fcitx5 -r 2>/dev/null
        echo "✓ Đã chuyển sang VNI"
        ;;
    on)
        fcitx5-remote -o 2>/dev/null
        echo "✓ Đã bật tiếng Việt"
        ;;
    off)
        fcitx5-remote -c 2>/dev/null
        echo "✓ Đã tắt tiếng Việt"
        ;;
    toggle|"")
        fcitx5-remote -t 2>/dev/null
        ;;
    status)
        # Show current method
        if [[ -f "$METHOD_FILE" ]]; then
            METHOD=$(cat "$METHOD_FILE")
        else
            METHOD="telex"
        fi
        # Check if Vietnamese is active
        STATE=$(fcitx5-remote 2>/dev/null)
        if [[ "$STATE" == "2" ]]; then
            echo "Tiếng Việt: BẬT ($METHOD)"
        else
            echo "Tiếng Việt: TẮT ($METHOD)"
        fi
        ;;
    *)
        echo "Gõ Nhanh - Vietnamese Input Method"
        echo ""
        echo "Cách dùng:"
        echo "  gn          Toggle bật/tắt tiếng Việt"
        echo "  gn on       Bật tiếng Việt"
        echo "  gn off      Tắt tiếng Việt"
        echo "  gn telex    Chuyển sang Telex"
        echo "  gn vni      Chuyển sang VNI"
        echo "  gn status   Xem trạng thái"
        ;;
esac
