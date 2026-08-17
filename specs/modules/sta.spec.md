# Module Spec: Station (STA) Mode Operations

## 1. Overview
规范定义 802.11 STA (Station) 客户端模式核心处理逻辑与数据通路交互契约（扫描与 Probe Request 构造、Beacon/Probe Response 解析、信道切换配置序列、RX 接收解析与 RxWI 剥离、TX 发送与 TxWI 封装、BSS 过滤配置与安全协议能力边界）。

### 1.1 安全协议支持范围 (Security Scope)
- **Supported (支持):** Open (无加密), WPA-PSK (TKIP / CCMP), WPA2-PSK (AES-CCMP).


## 2. Interface / API Contract

### 2.1 Probe Request Construction
- **Inputs:** `ssid: *const u8`, `ssid_len: usize`, `src_mac: *const u8`, `out_buf: *mut u8`, `max_out_len: usize`, `out_written: *mut usize`
- **Outputs:** 标准 802.11 Management Probe Request 帧 (含 Header + SSID IE + Supported Rates IE)
- **Errors:** `0` (Success), `-22` (`EINVAL`), `-28` (`ENOSPC`)

### 2.2 Beacon / Probe Response Parsing
- **Inputs:** `frame_buf: *const u8`, `frame_len: usize`, `out_info: *mut StaBssInfo`
- **Outputs:** BSSID、SSID、Channel、RSSI、Capability 信息
- **Errors:** `0` (Success), `-22` (`EINVAL`)

### 2.3 Channel Switch Sequence Generation
- **Inputs:** `channel: u8` (1..=13), `ops: *mut RegWriteOp`, `max_ops: usize`, `ops_written: *mut usize`
- **Outputs:** MT7603U 对应信道的 BBP/RF 频点与射频配置寄存器写操作序列
- **Errors:** `0` (Success), `-22` (`EINVAL` - 无效信道号), `-28` (`ENOSPC`)

### 2.4 RX Frame RxWI Header Demux
- **Inputs:** `buf: *const u8`, `buf_len: usize`, `out_info: *mut RxInfo`
- **Outputs:** 解析后的 `RxInfo`（包含 `pkt_len`, `rssi`, `channel`, `rate`, `is_beacon`, `is_data`），驱动随后剥离 12/16 字节 RxWI 硬件头并提交给 `ieee80211_rx_skb`
- **Errors:** `0` (Success), `-22` (`EINVAL` - 长度不足 12 字节或截断)

### 2.5 TX Frame TxWI Header Construction
- **Inputs:** `params: *const TxParams`, `out_buf: *mut u8`, `max_out_len: usize`, `out_written: *mut usize`
- **Outputs:** 16 字节 TxWI 发送描述符（包含 MPDU 长度、队列、PID、速率索引），驱动置于 skb 头部并通过 EP4 Bulk OUT 异步发送
- **Errors:** `0` (Success), `-22` (`EINVAL`), `-28` (`ENOSPC` - 缓冲区小于 16 字节)

### 2.6 Current BSSID Programming (STA 关联)
- **Inputs:** AP BSSID (来自 `BSS_CHANGED_BSSID` 的 `info->bssid`)
- **Outputs:** 将 Current BSSID 写入 RMAC_CB0R0/R1（HIF 0x21804/0x21808）
  - `CB0R0` = `bssid[0] | bssid[1]<<8 | bssid[2]<<16 | bssid[3]<<24`
  - `CB0R1` = `bssid[4] | bssid[5]<<8 | BIT(16)`（bit16 使能）
- **Rationale:** 固件依赖 Current BSSID 过滤并转发关联 AP 的单播 data 帧（如 EAPOL M1）到 EP 0x84。缺失时 STA 关联成功但 4-way 握手收不到 M1，AP 以 `DISASSOC_DUE_TO_INACTIVITY` (reason=4) 踢出。
- **Mapped Spec:** 厂商 `AsicSetBssid` (hw_ctrl/cmm_asic_mt.c:574)，寄存器定义 `include/mac/mac_mt/wf_rmac.h:68`

### 2.7 WTBL1 Table & BSSID Sequence Generation (STA 硬件单播接收表项)
- **Inputs:** `bssid: *const u8`, `ops_buf: *mut RegWriteOp`, `max_ops: usize`, `out_count: *mut usize`
- **Outputs:** 生成 WTBL1 Entry 0 (广播/通配默认条目, 0x28000) 与 Entry 1 (AP 专用单播条目, 0x28014) 的寄存器配置序列
  - Entry 0 (0x28000): DW0=`0x304EFF_FF` (`rv=1, rc_a2=1, rc_a1=1, muar_idx=0x0e`), DW1=`0xFFFFFFFF`, DW2=`0x00000000` (Cipher None)
  - Entry 1 (0x28014): DW0=`(1<<28)|(1<<29)|(1<<22)|(bssid[5]<<8)|bssid[4]`, DW1=`bssid[0..3]`, DW2=`0x00000000` (Cipher None)
- **Rationale:** MT7603 硬件要求单播数据帧（包括未加密的 EAPOL 帧）在 WTBL1 中存在有效条目 (`rv=1`) 且 Cipher Suite 匹配（明文阶段设为 NONE），否则硬件直接丢弃单播数据帧导致 4-Way 握手超时断开。
- **Mapped Spec:** 厂商 `AsicUpdateRxWCIDTable` (hw_ctrl/cmm_asic_mt.c:2874) 与 `mt_hw_tb_init` (mac/mt_mac.c:1845)

## 3. Acceptance Criteria (BDD)

### Feature: 802.11 Probe Request Frame Building

#### Scenario 1: [SPEC-STA-001] Construct Probe Request Frame for Scanning
- **Given** 源 MAC 地址 `00:0C:43:76:03:01` 和目标广播 SSID `"WiFi-Test"`
- **When** 调用 `mt7603_rust_build_probe_req(ssid, 9, src_mac, out_buf, 128, out_written)`
- **Then** 函数返回 `0`
- **And** `out_written` 大于等于 36 字节
- **And** `out_buf[0..2]` Frame Control 为 `0x0040` (Probe Request)
- **And** `out_buf[4..10]` 目的 MAC 为广播地址 `FF:FF:FF:FF:FF:FF`
- **And** SSID Element ID 为 `0` 且包含 `"WiFi-Test"` 内容
- **Mapped Test:** `src/rust/src/sta.rs:test_build_probe_request`

### Feature: Beacon & Probe Response Parsing

#### Scenario 2: [SPEC-STA-002] Parse Valid Beacon Frame Information
- **Given** 一个包含 BSSID `12:34:56:78:9A:BC` 和 SSID `"Home-AP"` 的 802.11 Beacon 帧
- **When** 调用 `mt7603_rust_parse_beacon(frame_buf, len, out_info)`
- **Then** 函数返回 `0`
- **And** `out_info->bssid` 等于 `12:34:56:78:9A:BC`
- **And** `out_info->ssid` 匹配 `"Home-AP"`
- **Mapped Test:** `src/rust/src/sta.rs:test_parse_beacon_frame`

### Feature: Channel Switch Configuration

#### Scenario 3: [SPEC-STA-003] Channel Switch Register Sequence
- **Given** 目标信道 `channel = 6` (2437 MHz)
- **When** 调用 `mt7603_rust_get_channel_sequence(6, ops_buf, 16, ops_written)`
- **Then** 函数返回 `0`
- **And** `ops_written` 大于等于 1
- **And** 寄存器操作包含针对信道 6 的 `BBP_R105` 或射频调谐配置
- **Mapped Test:** `src/rust/src/mac.rs:test_channel_switch_sequence`

### Feature: RX Frame RxWI Demux

#### Scenario 4: [SPEC-STA-004] Demux Valid RxWI Header
- **Given** 包含 12 字节 RxWI 头的有效 802.11 数据包（`pkt_len=32`, `RSSI=-64`, `Channel=6`）
- **When** 调用 `mt7603_rust_parse_rx_frame(buf, len, out_info)`
- **Then** 函数返回 `0`
- **And** `out_info->pkt_len` 等于 `32`
- **And** `out_info->rssi` 等于 `-64`
- **And** `out_info->channel` 等于 `6`
- **Mapped Test:** `src/rust/src/rx.rs:test_parse_valid_rx_frame`

### Feature: TX Frame TxWI Encapsulation

#### Scenario 5: [SPEC-STA-005] Build 16-byte TxWI Header
- **Given** 发送参数 `pkt_len=256`, `rate_idx=7`, `queue=0`, `pid=1`
- **When** 调用 `mt7603_rust_build_txwi(params, out_buf, max_len, out_written)`
- **Then** 函数返回 `0`
- **And** `out_written` 等于 `16`
- **And** `out_buf[0..2]` 等于小端序 `256`
- **And** `out_buf[4]` 为 `pid = 1`
- **And** `out_buf[8]` 为 `rate_idx = 7`
- **Mapped Test:** `src/rust/src/tx.rs:test_build_txwi`

### Feature: WTBL1 Table & BSSID Sequence Generation

#### Scenario 6: [SPEC-STA-006] Build WTBL1 Sequence for Associated AP
- **Given** 目标 AP BSSID `fc:34:97:19:0e:01`
- **When** 调用 `build_wtbl_sta_sequence(&bssid, &mut ops)`
- **Then** 函数返回 `0` 且 `out_written` 大于等于 6
- **And** `ops[0]` 写入 `0x00028000` 包含 `(1<<28)|(1<<29)|(1<<22)|0x000E0000|0xFFFF`
- **And** `ops[3]` 写入 `0x00028014` 包含 `(1<<28)|(1<<29)|(1<<22)|(0x01<<8)|0x0E`
- **And** `ops[4]` 写入 `0x00028018` 值为 `0x199734fc`
- **And** `ops[5]` 写入 `0x0002801C` 值为 `0x00000000` (Cipher None)
- **Mapped Test:** `src/rust/src/sta.rs:test_build_wtbl_sta_sequence`


