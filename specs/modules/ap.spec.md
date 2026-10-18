# Module Spec: Access Point (AP) Hotspot Mode Operations

## 1. Overview
规范定义 802.11 AP (Access Point) 热点模式核心处理逻辑（Beacon 广播帧组装、客户端 Assoc Request 帧解析、Assoc Response 响应帧构造与 mac80211 / hostapd 的接入控制集成）。

## 2. Interface / API Contract

### Beacon Frame Construction
- **Inputs:** `ssid: *const u8`, `ssid_len: usize`, `bssid: *const u8`, `channel: u8`, `out_buf: *mut u8`, `max_out_len: usize`, `out_written: *mut usize`
- **Outputs:** 标准 802.11 Management Beacon 广播帧 (Frame Control `0x0080` + Timestamp + Beacon Interval + Capability + SSID IE + Supported Rates IE + DS Channel IE)
- **Errors:** `0` (Success), `-22` (`EINVAL`), `-28` (`ENOSPC`)

### Association Response Framing
- **Inputs:** `sta_mac: *const u8`, `bssid: *const u8`, `aid: u16`, `status_code: u16`, `out_buf: *mut u8`, `max_out_len: usize`, `out_written: *mut usize`
- **Outputs:** 标准 802.11 Association Response 帧 (Frame Control `0x0010` + Capability + Status Code + AID + Supported Rates IE)
- **Errors:** `0` (Success), `-22` (`EINVAL`), `-28` (`ENOSPC`)

### Association Request Parsing
- **Inputs:** `frame_buf: *const u8`, `frame_len: usize`, `out_sta_mac: *mut u8`, `out_capability: *mut u16`, `out_listen_interval: *mut u16`
- **Outputs:** 解析 STA 发送的 802.11 Association Request 帧，提取客户端 MAC 地址、Capability 及 Listen Interval
- **Errors:** `0` (Success), `-22` (`EINVAL` - 非 Assoc Req 帧或长度不足)

### AP BssSync & Beacon Queue Register Sequence (`mt7603_rust_build_ap_bss_sync_sequence`)
- **Inputs:** `beacon_period: u16`, `enable: bool`, `out_ops: *mut RegWriteOp`, `max_ops: usize`, `out_written: *mut usize`
- **Outputs:** 对齐厂商 `AsicEnableBssSync` / `AsicDisableSync` (`hw_ctrl/cmm_asic_mt.c:1180-1305`) 的寄存器写序列：
  - 当 `enable == true` 时（6 条写操作）：
    1. `LPON_T0TPCR (0x0002_4034) = (beacon_period as u32) | 0x8000_0000` (`BEACONPERIODn | TBTTn_CAL_EN`)
    2. `LPON_MPTCR0 (0x0002_405C) = 0x0000_0099` (启用 `TBTT0` 与 `PRETBTT0` 周期定时器)
    3. `LPON_PISR   (0x0002_4030) = 0x0000_0050` (BSS0 Pre-TBTT 间隔 = `0x50` × 64us)
    4. `LPON_T0CR   (0x0002_4010) = 0x0000_000C` (启用硬件本地 TSF 自由运行计时器，供 TMAC 自动写入 Beacon/ProbeResp 时间戳)
    5. `ARB_SCR     (0x0002_1480) = 0x0000_0001` (`BCNQ_OP_MODE_AP = 0x1`，并保持 TX/RX 开启)
    6. `ARB_BCNQCR0 (0x0002_1518) = 0x0000_0001` (启动 BSS0 Beacon 队列)
  - 当 `enable == false` 时（4 条写操作）：
    1. `LPON_T0TPCR (0x0002_4034) = 0x0000_0000` (关闭 TBTT 计算)
    2. `LPON_MPTCR0 (0x0002_405C) = 0x0000_0000` (停止 TBTT0 定时器)
    3. `ARB_SCR     (0x0002_1480) = 0x0000_0000` (`BCNQ_OP_MODE_STA = 0x0`)
    4. `ARB_BCNQCR1 (0x0002_151C) = 0x0000_0001` (清空 BSS0 Beacon 队列)
- **Errors:** `0` (Success), `-22` (`EINVAL`), `-28` (`ENOSPC`)

### mac80211 / hostapd AP Mode Integration Contract (`src/c/mac80211.c`)
1. **AP BSSID 设定与保持**：
   - 当 `vif->type == NL80211_IFTYPE_AP` 且 `add_interface` 或 `bss_info_changed` (`BSS_CHANGED_BEACON_ENABLED | BSS_CHANGED_BEACON`) 触发时，将 AP 接口 MAC (`vif->addr`) 同时写入 `RMAC_OMA0R0/1` (`0x0002_1824/28`) 与 `RMAC_CB0R0/1` (`0x0002_1804/08`)，并配置 `WTBL1` `WCID 0` 广播表项。
   - 在 `sta_add` / `sta_remove` 中，若 `vif->type == NL80211_IFTYPE_AP`，**严禁**覆盖或清零 `RMAC_CB0R0/1`（`sta->addr` 为接入客户端 MAC，仅更新 `WTBL1` `WCID 1` 单播对端表项）。
2. **周期性 Beacon 广播 (`delayed_work`)**：
   - 当 `BSS_CHANGED_BEACON_ENABLED` 置位 (`info->enable_beacon == true`) 时，执行 `mt7603_rust_build_ap_bss_sync_sequence(beacon_int, true, ...)` 初始化硬件 TSF/TBTT，并启动内核 `delayed_work` (`beacon_work`)，按 `beacon_int` TU 周期调用 `ieee80211_beacon_get(hw, vif, 0)` 获取最新 Beacon 帧（含 TIM IE）通过 `mt7603_mac80211_tx` (`EP 0x08`) 广播。
   - 当 `enable_beacon == false`、`remove_interface` 或 `stop` 时，`cancel_delayed_work_sync(&dev->beacon_work)` 并执行 `mt7603_rust_build_ap_bss_sync_sequence(0, false, ...)`。
3. **TX 完成状态回报 (`ieee80211_tx_status_irqsafe`)**：
   - `hostapd` 通过 `NL80211_CMD_FRAME` 发送 `AUTH` / `ASSOC_RESP` 后，依赖内核 `NL80211_CMD_FRAME_TX_STATUS` (`IEEE80211_TX_STAT_ACK`) 回调触发 `handle_assoc_cb` 启动 WPA2 4-Way 握手（发送 EAPOL-Key M1）。
   - `mt7603_tx_complete` 在非 Beacon 帧 URB 发送完成时，必须剥离前置 32B `TMAC_TXD_L`、移除 `hdr_pad` 与尾部 `pad_len` 恢复原始 802.11 帧，置位 `IEEE80211_TX_STAT_ACK` 并调用 `ieee80211_tx_status_irqsafe(dev->hw, skb)`。

## 3. Acceptance Criteria (BDD)

### Feature: 802.11 AP Beacon Generation

#### Scenario 1: [SPEC-AP-001] Construct AP Beacon Frame for SSID Broadcasting
- **Given** BSSID `00:0C:43:76:03:01`、热点 SSID `"MT7603U-Hotspot"` 以及 2.4G 信道 `6`
- **When** 调用 `mt7603_rust_build_beacon(ssid, 15, bssid, 6, out_buf, 128, out_written)`
- **Then** 函数返回 `0`
- **And** `out_written` 大于等于 48 字节
- **And** `out_buf[0..2]` Frame Control 为 `0x0080` (Beacon)
- **And** `out_buf[10..16]` 与 `out_buf[16..22]` 地址包含 BSSID `00:0C:43:76:03:01`
- **And** SSID Element 包含 `"MT7603U-Hotspot"`，信道 Element 包含信道 `6`
- **Mapped Test:** `src/rust/src/ap.rs:test_build_beacon_frame`

### Feature: 802.11 Association Response Framing

#### Scenario 2: [SPEC-AP-002] Construct Association Response for STA Client
- **Given** 客户端 MAC `AA:BB:CC:DD:EE:FF`、BSSID `00:0C:43:76:03:01` 以及 AID `1` (状态码 `0` 成功)
- **When** 调用 `mt7603_rust_build_assoc_resp(sta_mac, bssid, 1, 0, out_buf, 128, out_written)`
- **Then** 函数返回 `0`
- **And** `out_buf[0..2]` Frame Control 为 `0x0010` (Association Response)
- **And** `out_buf[4..10]` 目的地址匹配客户端 MAC `AA:BB:CC:DD:EE:FF`
- **And** Status Code 匹配 `0` (Success)，AID 匹配 `1`
- **Mapped Test:** `src/rust/src/ap.rs:test_build_assoc_resp`

### Feature: 802.11 Association Request Parsing

#### Scenario 3: [SPEC-AP-003] Parse Association Request from Client
- **Given** 一个包含客户端 MAC `11:22:33:44:55:66`、Capability `0x0001` 及 Listen Interval `10` 的 Association Request 帧
- **When** 调用 `mt7603_rust_parse_assoc_req(frame_buf, len, out_sta_mac, out_cap, out_listen)`
- **Then** 函数返回 `0`
- **And** `out_sta_mac` 等于 `11:22:33:44:55:66`
- **And** `out_cap` 为 `0x0001`
- **And** `out_listen` 为 `10`
- **Mapped Test:** `src/rust/src/ap.rs:test_parse_assoc_req`

### Feature: AP BssSync & Beacon Queue Hardware Configuration

#### Scenario 4: [SPEC-AP-004] Build AP BssSync Enable Register Sequence
- **Given** Beacon 间隔 `100` TU (`0x0064`) 且 `enable = true`
- **When** 调用 `build_ap_bss_sync_sequence(100, true, &mut ops)`
- **Then** 返回 `Ok(6)`
- **And** 包含 `LPON_T0TPCR (0x0002_4034) = 0x8000_0064`、`LPON_MPTCR0 (0x0002_405C) = 0x0000_0099`、`LPON_PISR (0x0002_4030) = 0x0000_0050`、`LPON_T0CR (0x0002_4010) = 0x0000_000C`、`ARB_SCR (0x0002_1480) = 0x0000_0001`、`ARB_BCNQCR0 (0x0002_1518) = 0x0000_0001`
- **Mapped Test:** `src/rust/src/ap.rs:test_build_ap_bss_sync_enable`

#### Scenario 5: [SPEC-AP-005] Build AP BssSync Disable Register Sequence
- **Given** `enable = false`
- **When** 调用 `build_ap_bss_sync_sequence(100, false, &mut ops)`
- **Then** 返回 `Ok(4)`
- **And** 包含 `LPON_T0TPCR (0x0002_4034) = 0`、`LPON_MPTCR0 (0x0002_405C) = 0`、`ARB_SCR (0x0002_1480) = 0`、`ARB_BCNQCR1 (0x0002_151C) = 1`
- **Mapped Test:** `src/rust/src/ap.rs:test_build_ap_bss_sync_disable`


