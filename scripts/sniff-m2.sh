#!/usr/bin/env bash
# ============================================================================
# sniff-m2.sh — 用 MT7601U 抓包判定 M2(EAPOL-Key) 是否真正上 air
#
# 只操作 MT7601U 嗅探网卡，绝不改动 MT7603U(DUT) 的任何配置。
# 判定树:
#   STA->AP 的 EAPOL-Key(即 M2/M4) 在 air 上出现  => M2 已发出, AP 拒收=MIC 失败(查 M1 RX 污染)
#   STA->AP 的 EAPOL-Key 完全不出现              => M2 被 LMAC 丢弃(数据队列/SCH 或 WCID1 表项)
#
# 用法:
#   ./scripts/sniff-m2.sh [CHANNEL] [DUT_IFACE] [DURATION]
#     CHANNEL    嗅探信道(1-13)。省略则自动从 DUT link 读取(见下)
#     DUT_IFACE  MT7603U 接口名(默认 wlxb0d59d8c8c7a)，仅用于只读读取信道/MAC 做分析过滤
#     DURATION   抓包秒数(默认 25)
#   环境变量:
#     WAIT_CH    未立即拿到信道时, 轮询等待 DUT 关联的最长秒数(默认 15)。期间你可在另一终端触发 wpa_supplicant。
#
# 信道自动识别: 省略 CHANNEL 时, 脚本只读读取 `iw dev <DUT> link` 的 freq 行换算信道;
#   若 DUT 尚未关联(无信道), 会轮询等待至多 WAIT_CH 秒, 直到 STA 关联上露出信道再锁定嗅探器。
# ⚠️ 抓 4-Way 握手务必显式传 CHANNEL: 握手发生在关联瞬间, 若等信道自动浮现再开抓, M2 早已发完。
#   正确时序: `./scripts/sniff-m2.sh <CH> ...` 先开抓, 随后在另一终端启动 wpa_supplicant 触发握手。
# ============================================================================
set -uo pipefail

# In this sandbox `sudo` cannot prompt on a tty; feed the password via stdin so
# the internal `sudo iw`/`sudo ip` calls (monitor iface setup, channel lock) work.
sudo() { cat ~/.pass 2>/dev/null | command sudo -S "$@"; }

CHANNEL="${1:-}"
DUT_IFACE="${2:-wlxb0d59d8c8c7a}"
DURATION="${3:-25}"
WAIT_CH="${WAIT_CH:-15}"
MON="mon0"
PCAP="/tmp/m2_capture.pcap"

# ---- 1. 自动发现 MT7601U 嗅探网卡(驱动名为 mt7601u) -------------------------
SNIFFER=""
for d in /sys/class/net/*; do
    iface=$(basename "$d")
    drv=$(readlink "$d/device/driver" 2>/dev/null | xargs basename 2>/dev/null || true)
    if [ "$drv" = "mt7601u" ]; then
        SNIFFER="$iface"
        break
    fi
done

if [ -z "$SNIFFER" ]; then
    echo "❌ 未找到驱动为 mt7601u 的嗅探网卡。请确认 MT7601U 已插入并加载 mt7601u 驱动。"
    echo "   可用网卡: $(ls /sys/class/net 2>/dev/null | tr '\n' ' ')"
    exit 1
fi
echo "✅ 嗅探网卡: $SNIFFER (driver=mt7601u)  [DUT=$DUT_IFACE 不会被改动]"

# ---- 2. 确定信道(优先参数，否则从 DUT link 只读读取；未关联则轮询等待) ----
# 从 `iw dev <DUT> link` 解析信道: 优先 freq 行(更可靠)换算, 退而求 channel 行。
get_channel_from_dut() {
    local freq ch
    freq=$(iw dev "$DUT_IFACE" link 2>/dev/null | awk -F'freq: | ' '/freq:/ {print $2; exit}')
    if [[ "$freq" =~ ^[0-9]+$ ]] && [ "$freq" -ge 2412 ]; then
        # 2.4GHz: ch = (freq - 2407) / 5
        ch=$(( (freq - 2407) / 5 ))
        [ "$ch" -ge 1 ] && [ "$ch" -le 13 ] && { echo "$ch"; return 0; }
    fi
    ch=$(iw dev "$DUT_IFACE" link 2>/dev/null | awk '/channel/ {print $2; exit}')
    [[ "$ch" =~ ^[0-9]+$ ]] && { echo "$ch"; return 0; }
    return 1
}

if [ -z "$CHANNEL" ]; then
    if ip link show "$DUT_IFACE" >/dev/null 2>&1; then
        CHANNEL=$(get_channel_from_dut)
    fi
    if [ -z "$CHANNEL" ]; then
        echo "⏳ 未立即拿到信道, 轮询等待 DUT($DUT_IFACE) 关联 (最多 ${WAIT_CH}s)..."
        echo "   (此期间可在另一终端触发 wpa_supplicant 关联)"
        for ((i=0; i<WAIT_CH; i++)); do
            CHANNEL=$(get_channel_from_dut)
            [ -n "$CHANNEL" ] && break
            sleep 1
        done
    fi
fi
if [ -z "$CHANNEL" ]; then
    echo "❌ 仍无法确定信道。请显式传 CHANNEL 参数, 或确保 DUT($DUT_IFACE) 已关联。"
    exit 1
fi
echo "📡 嗅探信道: $CHANNEL"

# ---- 3. 只读获取 DUT MAC(用于分析时过滤 STA->AP 方向) -----------------------
DUT_MAC=""
if ip link show "$DUT_IFACE" >/dev/null 2>&1; then
    DUT_MAC=$(iw dev "$DUT_IFACE" link 2>/dev/null | awk '/addr/ {print tolower($2)}' | head -n1)
fi
[ -n "$DUT_MAC" ] && echo "🖥️  DUT MAC (过滤用): $DUT_MAC"

# ---- 4. 建 monitor 接口并锁信道 ------------------------------------------
cleanup() {
    echo "--> 清理 monitor 接口 $MON ..."
    sudo iw dev "$MON" del >/dev/null 2>&1 || true
}
trap cleanup EXIT

# 关键: monitor 与 managed 接口同 phy, 必须把 managed 接口 down 掉, 否则其信道会覆盖 mon0 锁定。
sudo iw dev "$MON" del >/dev/null 2>&1 || true
sudo ip link set "$SNIFFER" down 2>/dev/null || true
sudo iw dev "$SNIFFER" interface add "$MON" type monitor 2>/dev/null || {
    sudo iw dev "$SNIFFER" interface add "$MON" type monitor
}
sudo ip link set "$SNIFFER" down 2>/dev/null || true
# 稳健锁信道: MT7601U monitor 设信道偶发不生效, 循环校验 `iw info` 直到命中目标, 否则告警。
LOCKED=""
attempt=0
for attempt in 1 2 3 4 5 6; do
    sudo iw dev "$MON" set channel "$CHANNEL" 2>/dev/null || sudo iw dev "$MON" set freq "$((2407 + CHANNEL*5))" 2>/dev/null
    sudo ip link set "$MON" up 2>/dev/null || true
    sleep 0.3
    LOCKED=$(sudo iw dev "$MON" info 2>/dev/null | awk '/channel/ {print $2; exit}')
    [ "$LOCKED" = "$CHANNEL" ] && break
    sudo ip link set "$MON" down 2>/dev/null || true
    sleep 0.5
done
echo "✅ monitor 接口 $MON 已起, 锁定信道: ${LOCKED:-?} (目标 $CHANNEL) [尝试 $attempt]"
if [ "$LOCKED" != "$CHANNEL" ]; then
    echo "⚠️  信道锁定失败(实际 $LOCKED != 目标 $CHANNEL): 抓包可能落在错误信道, M2 可能漏抓。请显式指定正确 CHANNEL 或换用更稳的嗅探器。"
fi

# ---- 5. 抓包 --------------------------------------------------------------
if [ -f ~/.pass ]; then
    cat ~/.pass | sudo -S tcpdump -i "$MON" -s0 -w "$PCAP" -G "$DURATION" \
        'type mgt or type data' >/dev/null 2>&1 &
else
    sudo tcpdump -i "$MON" -s0 -w "$PCAP" -G "$DURATION" \
        'type mgt or type data' >/dev/null 2>&1 &
fi
TCPDUMP_PID=$!
sleep "$DURATION"
# 确保抓包结束
kill "$TCPDUMP_PID" >/dev/null 2>&1 || true
sleep 1
sudo chmod 644 "$PCAP" 2>/dev/null || true
echo "✅ 抓包完成 -> $PCAP"

# ---- 6. 分析 --------------------------------------------------------------
if ! [ -s "$PCAP" ]; then
    echo "❌ pcap 为空, 抓包可能失败(检查 $MON 是否成功 up / tcpdump 权限)。"
    exit 1
fi

# DUT MAC 用接口固资地址(与关联状态无关, 始终可拿), 保证方向过滤可用。
if [ -f "/sys/class/net/$DUT_IFACE/address" ]; then
    DUT_MAC=$(cat "/sys/class/net/$DUT_IFACE/address" 2>/dev/null | tr 'A-F' 'a-f')
    [ -n "$DUT_MAC" ] && echo "🖥️  DUT MAC (接口固资): $DUT_MAC"
fi

echo ""
echo "=================================================================="
echo " 📊 EAPOL-Key 帧分析 (4-way 握手消息均为 eapol.type==3, ethertype 0x888E)"
echo "=================================================================="

AP_CNT=0; STA_CNT=0; NEED_MANUAL=0
EAPOL_FILTER='ether proto 0x888e'

if [ -z "$DUT_MAC" ]; then
    # 拿不到 DUT MAC, 无法做方向过滤, 只能给出原始 EAPOL 计数并请人工看
    echo "⚠️  未取到 DUT MAC, 无法做方向区分。原始 EAPOL-Key 帧(全部方向):"
    tcpdump -r "$PCAP" -enn "$EAPOL_FILTER" 2>/dev/null | head -30
    EAPOL_TOTAL=$(tcpdump -r "$PCAP" -enn "$EAPOL_FILTER" 2>/dev/null | wc -l)
    echo "   原始 EAPOL 帧总数: $EAPOL_TOTAL"
    echo "   建议: 在 Wireshark 打开 $PCAP, 过滤 'eapol && wlan.sa==<STA MAC>' 看 M2 是否出现。"
    NEED_MANUAL=1
elif command -v tshark >/dev/null 2>&1; then
    echo "--> AP -> STA (M1/M3) 方向:"
    tshark -r "$PCAP" -Y "eapol.type==3 && wlan.da==$DUT_MAC" -T fields \
        -e frame.time_relative -e wlan.sa -e wlan.da -e eapol.type 2>/dev/null \
        | awk '{printf "   t=%-7s AP=%s -> STA=%s  eapol=%s\n",$1,$2,$3,$4}' | head -20
    echo "--> STA -> AP (M2/M4) 方向 [关键]:"
    tshark -r "$PCAP" -Y "eapol.type==3 && wlan.sa==$DUT_MAC" -T fields \
        -e frame.time_relative -e wlan.sa -e wlan.da -e eapol.type 2>/dev/null \
        | awk '{printf "   t=%-7s STA=%s -> AP=%s  eapol=%s\n",$1,$2,$3,$4}' | head -20
    AP_CNT=$(tshark -r "$PCAP" -Y "eapol.type==3 && wlan.da==$DUT_MAC" 2>/dev/null | wc -l)
    STA_CNT=$(tshark -r "$PCAP" -Y "eapol.type==3 && wlan.sa==$DUT_MAC" 2>/dev/null | wc -l)
    # 🔬 M2 原始字节: 关键判定 hdr_pad 是否被 HW 剥离
    #   802.11 QoS-Data 头 = 26 字节。若 HW 剥离了 2 字节 pad, LLC/SNAP(AA AA 03 00 00 00 88 8E)
    #   应紧跟在 offset 26。若 offset 26/27 = 00 00, 说明 2 字节 pad 仍残留在 air 上, EAPOL 整体错位 2 字节。
    if [ "$STA_CNT" -gt 0 ]; then
        echo ""
        echo "🔬 M2 原始字节 (STA->AP 首个 EAPOL-Key; 重点看 offset 26/27):"
        echo "   offset 26/27 == AA AA => LLC/SNAP 紧贴 26B 头, pad 已被 HW 剥离 ✅"
        echo "   offset 26/27 == 00 00 => 2B pad 残留在 air, EAPOL 错位 ❌ (hdr_pad 未剥离)"
        tshark -r "$PCAP" -Y "eapol.type==3 && wlan.sa==$DUT_MAC" -x 2>/dev/null | head -10
    fi
else
    echo "⚠️  未安装 tshark, 用 tcpdump 做方向区分(降级模式, 以 802.11 SA/DA 文本过滤):"
    echo "   全部 EAPOL-Key 帧:"
    tcpdump -r "$PCAP" -enn "$EAPOL_FILTER" 2>/dev/null | head -30
    # tcpdump -e 对 802.11 monitor 帧输出 "DA:<mac> BSSID:<mac> SA:<mac>", 非 "src > dst"
    #   STA->AP (M2/M4): STA 是源 => 匹配 "SA:DUT_MAC"
    #   AP->STA (M1/M3): STA 是目的 => 匹配 "DA:DUT_MAC"
    STA_CNT=$(tcpdump -r "$PCAP" -enn "$EAPOL_FILTER" 2>/dev/null | grep -c "SA:$DUT_MAC" || true)
    AP_CNT=$(tcpdump -r "$PCAP" -enn "$EAPOL_FILTER" 2>/dev/null | grep -c "DA:$DUT_MAC" || true)
    echo "   建议: 安装 tshark 可得更可靠判定 (sudo apt-get install tshark)。"
fi

echo ""
echo "------------------------------------------------------------------"
echo "  AP->STA EAPOL-Key 帧数: $AP_CNT  (反复出现=AP 在重传 M1)"
echo "  STA->AP EAPOL-Key 帧数: $STA_CNT  (>0 即 M2 已上 air)"
echo "------------------------------------------------------------------"

if [ "$NEED_MANUAL" -eq 1 ]; then
    echo "⚠️  无法自动判定方向(缺 DUT MAC)。请在 Wireshark 打开 $PCAP, 过滤 'eapol && wlan.sa==<STA MAC>' 看 M2 是否出现。"
elif [ "$STA_CNT" -gt 0 ]; then
    echo "✅ 判定: M2 已上 air (STA->AP EAPOL-Key 可见)。"
    echo "   => AP 收到却重传 M1, 说明 M2 被拒 = MIC 失败。"
    echo "   => 根因在 RX: M1 被污染导致 wpa_supplicant 算出错误 M2。"
    echo "   => 下一步查 rx.rs:89 pkt_len 未剥离 4 字节 FCS 的影响。"
else
    echo "❌ 判定: M2 从未上 air (STA->AP EAPOL-Key 为 0)。"
    echo "   => LMAC 把帧丢弃了, 根本没发出。"
    echo "   => 根因在 TX: 数据队列/调度器(EP 0x05 / AC0)未真正使能, 或 WCID1 表项导致丢弃。"
    echo "   => 下一步查 SCH/ARB 数据队列使能与 build_wtbl_sta_sequence(WCID1)。"
fi
