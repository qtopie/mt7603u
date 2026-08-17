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

