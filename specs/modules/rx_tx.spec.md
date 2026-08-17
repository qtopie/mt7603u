# Module Spec: RX Frame Parsing & TX Frame Transcoding

## 1. Overview
规范定义 802.11 / RxWI 数据包解析与 TxWI 封装模块（`src/rust/src/rx.rs` & `src/rust/src/tx.rs`）。

## 2. Interface / API Contract

### RX Parsing
- **Inputs:** `data: *const u8`, `len: usize`, `rssi_offset: i8`（EEPROM 0x46 有符号校准偏移，已按 -10..10 校验）, `out: *mut RxInfo`
- **Outputs:** 解析得出的 RSSI（dBm，`0`=未知）、帧长度、帧类型（Beacon/Data）、CRC 状态
- **Errors:** `0` (Success), `-22` (`EINVAL`), `-14` (`EFAULT`)

#### RSSI 提取契约（厂商 `ParseRxVPacket` + `ConvertToRssi`）
- 仅当 `grp_vld & 0x04`（Group 3 / RxVector 存在）且 `pkt_type == RX_NORMAL(0x02)` 时提取：
  - Group3 起始偏移 = `16 + (G4?16:0) + (G1?16:0) + (G2?8:0)`（组序 G4→G1→G2→G3，厂商 `mt_rx_info_2_blk`，`cmm_data.c:426-448`）
  - `IBRssi0` = Group3+12 处有符号字节（厂商 `RXV1_4TH_CYCLE->IBRssi0`，`cmm_data.c:373`）
  - **dBm 换算：** `rssi_dbm = IBRssi0 + rssi_offset`（厂商 MT7603 `ConvertToRssi`：`rssi + BGRssiOffset - LNAGain`，`cmm_sync.c:502`；MT7603 `lan_gain` 恒为 0，无赋值路径）
- `IBRssi0 == 0`（厂商映射为 -99）或 Group3 缺失 → `rssi = 0`（未知，cfg80211 按无信号处理）
- C 侧上报：`ieee80211_rx_status.signal = rssi * 100`（MBM，`wiphy->signal_type = CFG80211_SIGNAL_TYPE_MBM`）

### TX Transcoding
- **Inputs:** `params: *const TxParams`, `txwi_buf: *mut u8`, `txwi_len: usize`
- **Outputs:** 构建完成的 **32-byte TMAC_TXD_L（长 TXD）** header（厂商 `TXWISize = sizeof(TMAC_TXD_L) = 32`，`chips/mt7603.c:1199`；MT_MAC 下 `tx_hw_hdr_len = 32`，`mac_info.Length = SrcBufLen - 32`，`tmac_info = pSrcBufVA`，`common/cmm_data.c:1969-1972`）
- **Errors:** `0` (Success), `-22` (`EINVAL`), `-28` (`ENOSPC`)

### TXD 字段契约（LE 位域，厂商 `write_tmac_info`，`mac/mt_mac.c:840-1150`，位域定义 `include/mac/mac_mt/mt_mac.h:60-470`）
Probe Request（STA 主动扫描，ch≤14）场景取值：

| DW | 位域 | 字段 | 值 | 依据 |
|---|---|---|---|---|
| DW0 | [15:0] | tx_byte_cnt | txd_size(32)+Length（初值），USB 层按路径重写（数据路径=4 对齐 `USBDMApktLen`；mgmt 路径保持 SrcBufLen） | mt_mac.c:1139、cmm_data_usb.c:1544 |
| DW0 | [30:27] | q_idx | Q_IDX_AC4(0x04) | cmm_data.c:2079 (MT_MAC mgmt) |
| DW0 | [31] | p_idx | P_IDX_LMAC(0) | mt_mac.c:896 |
| DW1 | [7:0] | wlan_idx | 0（未关联广播） | mt_mac.c:908 |
| DW1 | [12:8] | hdr_info | hdr_len>>1（Probe Req=12） | TMI_HDR_INFO_2_VAL，hdr_len 必须偶数 |
| DW1 | [14:13] | hdr_format | TMI_HDR_FT_NOR_80211(2) | mt_mac.c:910 |
| DW1 | [15] | ft | TMI_FT_LONG(1) | mt_mac.c:909 |
| DW1 | [18:16] | hdr_pad | 0 | info->hdr_pad |
| DW1 | [19] | no_ack | 1（Probe Req 无 ACK） | mt_mac.c:915，info->Ack=0 |
| DW1 | [22:20] | tid | 0 | mac_info.TID=0 |
| DW1 | [23] | protect_frm | 0 | info->prot=0 |
| DW1 | [31:26] | own_mac | 0 | mt_mac.c:917 |
| DW2 | [3:0] | sub_type | 从 802.11 FC 提取（Probe Req=4） | mt_mac.c:966 |
| DW2 | [5:4] | frm_type | 从 802.11 FC 提取（mgmt=0） | mt_mac.c:967 |
| DW2 | [10] | bc_mc_pkt | BM（广播=1） | mt_mac.c:951 |
| DW2 | [12] | duration | 0 | mt_mac.c:953 |
| DW2 | [13] | htc_vld | 0 | mt_mac.c:954 |
| DW2 | [15:14] | frag | 0 | info->FRAG |
| DW2 | [23:16] | max_tx_time | 0 | mt_mac.c:950 |
| DW2 | [28:24] | pwr_offset | 0 | - |
| DW2 | [29] | ba_disable | 1 | mt_mac.c:955 |
| DW2 | [30] | timing_measure | 0 | mt_mac.c:956 |
| DW2 | [31] | fix_rate | 1 | mt_mac.c:952 |
| DW3 | [10:6] | tx_cnt | 0 | - |
| DW3 | [15:11] | remain_tx_cnt | MT_TX_SHORT_RETRY(0x0f) | mt_mac.c:998-1005 |
| DW3 | [27:16] | sn | 0（mac80211 已填 SEQ，TXD 无需重复） | 见下注 |
| DW3 | [30] | pn_vld | 0 | - |
| DW3 | [31] | sn_vld | 0 | - |
| DW4 | [31:0] | pn_low | 0 | - |
| DW5 | [7:0] | pid | 0（无 TxS 需求） | mt_mac.c:1045 |
| DW5 | [8:10] | tx_status_* | 0 | mt_mac.c:1046-1080 |
| DW5 | [11] | da_select | TMI_DAS_FROM_MPDU(0) | - |
| DW5 | [12] | bar_sn_ctrl | TMI_BSN_CFG_BY_SW(1) | mt_mac.c:1093 |
| DW5 | [13] | pwr_mgmt | TMI_PM_BIT_CFG_BY_HW(0) | mt_mac.c:1094-1097 |
| DW5 | [31:16] | pn_high | 0 | - |
| DW6 | [0] | fix_rate_mode | TMI_FIX_RATE_BY_TXD(0) | mt_mac.c:1021 |
| DW6 | [7:2] | ant_id | 0 | - |
| DW6 | [10:8] | bw | (1<<2)|BW_20 = 0x4 | mt_mac.c:1027 |
| DW6 | [11] | spe_en | 1 | mt_mac.c:1026 |
| DW6 | [14:12] | ant_pri | 0 | info->AntPri |
| DW6 | [29:18] | tx_rate | tx_rate_to_tmi_rate(MODE_CCK,0,1,0,LONG_PREAMBLE)=0 | mt_mac.c:1032-1042 |
| DW6 | [31:30] | ldpc/gi | 0 | - |
| DW7 | [15:0] | sch_tx_time | 0 | - |
| DW7 | [31:16] | sw_field | 0 | - |

> **sn 说明:** 802.11 帧头内的 SEQ 由 mac80211 提供（`mgmt->seq_ctrl`），TXD 的 `sn` 字段置 0 由 HW 忽略（`txd_3->sn_vld=0`），与厂商 STA 扫描行为一致。

### TX 帧长度与 USB 提交契约（`common/cmm_data_usb.c:1712-1805` RtmpUSBMgmtKickOut）
- **mgmt 路径（EP 0x08）:** `BulkOutSize = (SrcBufLen + 3) & ~3`，随后 `+= 4`（USB_END_PADDING 尾部 4 零字节）；`padLen = BulkOutSize - SrcBufLen` 清零；TXD 的 `tx_byte_cnt` 保持 `SrcBufLen`（32+帧长）。
- **数据路径（EP 0x05）:** `tx_byte_cnt` 重写为 4 对齐 `USBDMApktLen`；批量打包按 `tx_byte_cnt + padding` 叠加（`padding=(4-tx_byte_cnt%4)&3`，`rtusb_bulk.c:673-674`）。
- **端点路由（`chips/mt7603.c:1249-1256` + `rtusb_bulk.c:198-204`）:**
  - `CommandBulkOutAddr = 0x8`: MT7603 下 **所有 mgmt 帧（MgmtRing/BcnRing/MGMTPIPEIDX）与 MCU 命令**共用
  - `WMM0ACBulkOutAddr = {0x5, 0x4, 0x6, 0x7, 0x8}`: 数据帧按 AC 路由（AC0=0x5）
  - `DataBulkInAddr = 0x84`、`CommandRspBulkInAddr = 0x85`

## 3. Acceptance Criteria (BDD)

### Feature: RX Frame Parsing

#### Scenario 1: [SPEC-RXTX-001] Valid MT-MAC RMAC_RXD RX Frame Parse
- **Given** 一个从 Bulk IN 端点接收到的 MT7603 RMAC_RXD 帧（含 16 字节 Base + Grp1~3 扩展头，共 64 字节 header，总长 128 字节）
- **When** 调用 `mt7603_rust_parse_rx_frame(data, 128, out)`
- **Then** 函数返回 `0`
- **And** `out->hdr_len` 为 `64`
- **And** `out->pkt_len` 为 `64` (128 - 64)
- **And** `out->is_crc_error` 正确反映 FCS 状态
- **Mapped Test:** `src/rust/src/rx.rs:test_parse_valid_rx_frame`

#### Scenario 2: [SPEC-RXTX-002] Truncated RX Packet Guard
- **Given** 一个被截断的 RX 帧（长度小于 RMAC_RXD 头部最小尺寸 16 字节）
- **When** 调用 `mt7603_rust_parse_rx_frame(data, 8, out)`
- **Then** 函数返回 `-22` (`EINVAL`)
- **Mapped Test:** `src/rust/src/rx.rs:test_parse_truncated_rx_frame`

#### Scenario 4: [SPEC-RXTX-004] RSSI Extraction from Group 3 RxVector
- **Given** 一个含 Group1/2/3 的 RMAC_RXD 帧（`grp_vld = 0b0111`，64 字节 header，Group3 起始偏移 40），`IBRssi0`（偏移 52）为 `0x9A` (-102)
- **When** 调用 `mt7603_rust_parse_rx_frame(data, 128, 0, out)`
- **Then** 函数返回 `0`
- **And** `out->rssi` 等于 `-102`
- **And** 以 `rssi_offset = 2` 调用时 `out->rssi` 等于 `-100`
- **Mapped Test:** `src/rust/src/rx.rs:test_parse_rx_rssi`

#### Scenario 5: [SPEC-RXTX-005] RSSI Unknown Fallback
- **Given** 一个仅含 Group1/2（`grp_vld = 0b0011`，无 Group3）的 RMAC_RXD 帧
- **When** 调用 `mt7603_rust_parse_rx_frame(data, len, 0, out)`
- **Then** `out->rssi` 等于 `0`（未知）
- **And** Group3 存在但 `IBRssi0 == 0` 时 `out->rssi` 同样为 `0`
- **Mapped Test:** `src/rust/src/rx.rs:test_parse_rx_rssi_unknown`

### Feature: TX Frame Transcoding

#### Scenario 3: [SPEC-RXTX-003] 32-byte Long TXD (TMAC_TXD_L) Construction
- **Given** `TxParams` 结构体包含 Probe Request 场景字段（hdr_len=24, fc_type=0, fc_subtype=4, no_ack=1, is_bm=1, rate_mode=MODE_CCK, rate_mcs=0, bw=BW_20, queue=Q_IDX_AC4）
- **When** 调用 `mt7603_rust_build_txwi(params, buf, 32)`
- **Then** 函数返回 `0`
- **And** `buf[0..32]` 满足 TMAC_TXD_L 全字段契约（见 §2 字段表）
- **And** `buf[0:4]` DW0 = tx_byte_cnt(32+pkt_len) | q_idx(0x04)<<27 | p_idx(0)<<31
- **And** `buf[4:8]` DW1 = wlan_idx(0) | hdr_info(12)<<8 | hdr_format(2)<<13 | ft(1)<<15 | no_ack(1)<<19
- **And** `buf[8:12]` DW2 = sub_type(4) | frm_type(0)<<4 | bc_mc_pkt(1)<<10 | ba_disable(1)<<29 | fix_rate(1)<<31
- **And** `buf[12:16]` DW3 = remain_tx_cnt(0x0f)<<11
- **And** `buf[24:28]` DW6 = fix_rate_mode(0) | bw(0x4)<<8 | spe_en(1)<<11 | tx_rate(0)<<18
- **And** 返回写入长度 `32`
- **Mapped Test:** `src/rust/src/tx.rs:test_build_txwi`
