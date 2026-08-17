#!/usr/bin/env bash
set -euo pipefail

IFNAME="wlxb0d59d8c8c7a"

echo "=== [Hardware Scan Test] 检查 MT7603U 物理网卡 WiFi 扫描能力 ==="

if ! ip link show "$IFNAME" >/dev/null 2>&1; then
    echo "❌ 错误: 未检测到物理接口 $IFNAME，请确认驱动已加载！"
    exit 1
fi

echo "--> 1. 确保接口 $IFNAME 已拉起 (UP)..."
if [ -f ~/.pass ]; then
    cat ~/.pass | sudo -S ip link set "$IFNAME" up || true
else
    sudo ip link set "$IFNAME" up || true
fi
sleep 1

echo "--> 2. 触发 802.11 主动/被动扫描 (iw dev $IFNAME scan)..."
if [ -f ~/.pass ]; then
    cat ~/.pass | sudo -S iw dev "$IFNAME" scan >/dev/null 2>&1 || true
else
    sudo iw dev "$IFNAME" scan >/dev/null 2>&1 || true
fi
sleep 1

echo "--> 3. 收集扫描到的 BSSID / SSID 列表..."
SCAN_OUTPUT=$(iw dev "$IFNAME" scan dump 2>/dev/null || true)

SSID_COUNT=$(echo "$SCAN_OUTPUT" | grep -c "SSID:" || true)

if [ "$SSID_COUNT" -gt 0 ]; then
    echo "✅ 成功扫描到 $SSID_COUNT 个 WiFi 网络:"
    echo "$SCAN_OUTPUT" | grep -E "(BSS |SSID:|signal:)" | head -n 30
    echo "=================================================="
    echo "🎉 [Hardware Scan Test] 测试通过！已成功发现周边真实 WiFi AP！"
    exit 0
else
    echo "❌ [Hardware Scan Test] 测试失败: 未扫描到任何 WiFi SSID！"
    echo "当前 scan dump 输出为空。芯片接收链路 (RX) 仍需进一步调通。"
    exit 1
fi
