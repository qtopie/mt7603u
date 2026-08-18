# Module Spec: MCU Firmware Command Packet Construction

## 1. Overview
规范定义 Andes MCU 固件与命令数据包格式构造模块（`src/rust/src/mcu.rs`）。MT7603U 依赖 MCU 命令包（通过 EP6 Bulk OUT）下载 `mt7603u_e2.bin` 固件与下发硬件控制与校准数据。本规范基于厂商驱动真实数据包格式（`mcu/andes_mt.c`、`include/mcu/andes_mt.h`），不再使用虚构的 4 字节头。

## 2. FW_TXD 命令头格式（12 字节下载期 / 32 字节运行期）

每个发送到 EP6 的命令帧 = `FW_TXD` 头 + payload（可选） + 4 字节零 padding（`USB_END_PADDING`）。所有 dword 以小端序写入。

**头的尺寸由命令所处阶段决定（双实现证据，见 §2.1）：**
- **下载期**（`FW_NO_INIT / FW_DOWNLOAD / ROM_PATCH_DOWNLOAD`）：**12 字节短头**（`fw_txd_0/1/2`）。
- **运行期**（`FW_RUN_TIME`，固件启动完成后）：**32 字节完整 `FW_TXD`**（`fw_txd_0/1/2` + `au4D3toD7rev[5]`，GNU_PACKED → sizeof=32B），其中 `au4D3toD7rev[5]` 为 **20 字节保留区，必须显式清零**（mainline `__mt76_mcu_msg_alloc` 对整个 skb head 做 `memset(skb->head, 0, len)`）。

| Offset | 位域 | 说明 |
|---|---|---|
| `0..1` | `length[15:0]` | **整个帧总长度**（头 + payload，不含 padding；下载期 `12+payload`，运行期 `32+payload`） |
| `2..3` | `pq_id[15:0]` | 优先级队列 ID（如 `P1_Q0=0x8000`、固件 scatter `0xC000`） |
| `4` | `cid` | 命令 ID |
| `5` | `pkt_type_id` | `0xA0` (`PKT_ID_CMD`) |
| `6` | `set_query` | `CMD_SET=1` / `CMD_QUERY=0` / `CMD_NA=3` |
| `7` | `seq_num` | 序列号（`need_wait` 命令从 1 起递增，vendor `AndesGetCmdMsgSeq`；无响应命令为 0） |
| `8` | `0x00` | 保留 (`ucD2B0Rev`) |
| `9` | `ext_cid` | 扩展命令类型 (`ExtCmdType`)，普通命令为 `EXT_CMD_NA(0)` |
| `10` | `0x00` | 保留 (`ucD2B2Rev`) |
| `11` | `ext_cid_option` | **仅当 `cid==EXT_CID(0xED)` 且 `set_query∈{SET,QUERY}` 且 `need_rsp` 时为 `1`（NEED_ACK），否则 `0`** |
| `12..31` | `au4D3toD7rev[5]` | **仅运行期存在**：20 字节保留区，**必须显式清零**（厂商未显式清零但分配自 `dev_alloc_skb`；mainline 显式 `memset` 清零，本驱动对齐 mainline 显式置 0） |

来源：`mcu/andes_mt.c:AndesMTFillCmdHeader`、`include/mcu/andes_mt.h` FW_TXD 联合体（little-endian 位域：DW2 的 `ucD2B0Rev | ext_cid | ucD2B2Rev | ext_cid_option` 四字节布局）。`length` 为整个帧长度，因为填充 header 时 header 已并入 net_pkt。

### 2.1 头尺寸的阶段依据（双实现交叉验证）

**厂商 SDK（`mcu/andes_mt.c:AndesMTFillCmdHeader`，365-410 行）**按 `Ctl->Stage` 选择头尺寸：
- 下载阶段（`FW_NO_INIT` / `FW_DOWNLOAD` / `ROM_PATCH_DOWNLOAD`）：`OS_PKT_HEAD_BUF_EXTEND(net_pkt, 12)` → **12 字节短头**；
- 运行期（`FW_RUN_TIME`）：`OS_PKT_HEAD_BUF_EXTEND(net_pkt, sizeof(*fw_txd))` → **32 字节完整 `FW_TXD`**。

`fw_txd_0.field.length = GET_OS_PKT_LEN(net_pkt)`（andes_mt.c:410）——`GET_OS_PKT_LEN`=skb->len，即**含头的整帧长度**。`AndesMTUSBKickOutCmdMsg`（andes_mt.c:220-268）在尾部追加 4 字节 `USB_END_PADDING` 零填充后提交 URB，URB 长度=`GET_OS_PKT_LEN`（头+payload+pad）。

芯片能力表 `chips/mt7603.c:1243`：`.cmd_header_len = sizeof(FW_TXD)`（=32B）；`:1248`：`.cmd_padding_len = 4`。

**主线上游 mt76（openwrt/mt76，PCIe mt7603e 已知可用）**在 `mt7603_mcu_skb_send_msg`（mt7603/mcu.c:42）中同样按阶段选择：
```c
int hdrlen = dev->mcu_running ? sizeof(struct mt7603_mcu_txd) : 12;
```
`dev->mcu_running = true` 在固件启动完成、`mt76_poll_msec(MT_TOP_MISC2, BIT(1))` 通过后设置（mt7603/mcu.c:206）。`struct mt7603_mcu_txd`（mt7603/mcu.h:6-19）字段布局与厂商一致并含 `au4_d3_to_d7_rev[5]`，`__packed __aligned(4)` → **32B**。`__mt76_mcu_msg_alloc` 分配时对整个 skb head 做 `memset(skb->head, 0, len)`，保留字段被清零。

**结论（本驱动契约）：** 下载期命令（`CmdAddressLenReq`/`CmdFwStartReq`/`CmdRestartDLReq`/`CmdFwScatter`）用 **12B** 头；运行期命令（`CmdChannelSwitch`/`CmdSetTxPowerCtrl`/`CmdRadioOnOffCtrl`/`CmdEfusBufferModeSet`/`CmdChPrivilege`/`CmdEdcaParameterSet`）用 **32B** 头（12B 字段 + 20B 保留清零）。

**长度账目（dmesg `bulk-out send len` 观测点）：**
- channel switch：`32 + 36 + 4 = 72`（错误 12B 时为 52）
- TX power：`32 + 44 + 4 = 80`（错误 12B 时为 60）

## 3. 固件下载命令序列（AndesMTLoadFwMethod1）

**下载期命令（步骤 0-3）使用 12 字节 `FW_TXD` 短头；运行期命令（步骤 4-5）使用 32 字节完整 `FW_TXD` 头（见 §2.1）。**

| 步骤 | 命令 | cid | pq_id | payload | need_rsp | 头尺寸 |
|---|---|---|---|---|---|---|
| 0* | `CmdRestartDLReq` | `0xEF` (`MT_RESTART_DL_REQ`) | `0x8000` | （无 payload） | 是（seq 动态） | 12B |
| 1 | `CmdAddressLenReq` | `0x01` (`MT_TARGET_ADDRESS_LEN_REQ`) | `0x8000` | `[le32 0x100000, le32 dl_len, le32 0x8000_0000]` | 是（seq 动态） | 12B |
| 2 | `CmdFwScatter` ×N | `0xEE` (`MT_FW_SCATTER`) | `0xC000` | 固件块，每片 ≤ `4096 - 32 = 4064` 字节 | 否 (seq=0) | 12B |
| 3 | `CmdFwStartReq` | `0x02` (`MT_FW_START_REQ`) | `0x8000` | `[le32 0x0000_0001(override), le32 0x100000]` | 是（seq 动态） | 12B |
| 4 | `CmdRadioOnOffCtrl` | `0xED` (`EXT_CID`, ext=0x05) | `0x8000` | 4 字节 `EXT_CMD_RADIO_ON_OFF_CTRL_T` | 是（seq 动态） | **32B** |
| 5 | `CmdChannelSwitch` | `0xED` (`EXT_CID`, ext=0x08) | `0x8000` | 36 字节 `EXT_CMD_CHAN_SWITCH_T` | 是（seq 动态） | **32B** |
| 6 | `CmdEdcaParameterSet` | `0xED` (`EXT_CID`, ext=0x27) | `0x8000` | 36 字节 `CMD_EDCA_SET_T`（4 字节头 + 4×`TX_AC_PARAM_T`） | 是（seq 动态） | **32B** |

- `*` 步骤 0 仅当检测到 RAM 固件已在运行（`TOP_MISC2 & 0x02 == 0x02`，即 re-probe/reconnect）时发送，用于让 MCU 跳回 ROM 代码再重新下载（vendor `AndesMTLoadFwMethod1`）。
- `seq` 由 `AndesGetCmdMsgSeq` 语义分配：`cmd_seq >= 0xf ? cmd_seq = 1 : cmd_seq++`，`cmd_seq` 初始为 0，故首个 `need_wait` 命令 `seq=1`。
  - 冷探测（RAM 未运行）：`CmdAddressLenReq` seq=1、`CmdFwStartReq` seq=2。
  - 热探测（RAM 运行）：`CmdRestartDLReq` seq=1、`CmdAddressLenReq` seq=2、`CmdFwStartReq` seq=3。
- `seq=0` 专用于 no-wait 命令（`CmdFwScatter`）。
- `dl_len = le32(fw[fw_len-4..fw_len]) + 4`（尾部 4 字节为下载长度，另含 4 字节 CRC）。
- 每片 USB 帧总长 = `12 + payload + 4`（含 padding），须 ≤ `MT_UPLOAD_FW_UNIT=4096`。

## 3.1 USB 端点路由契约 (Endpoint Routing Contract)

- **命令发送端点:** 所有 MCU 命令（下载期与运行期）均通过 **EP 0x06 (Bulk OUT)** 下发。
- **下载期（Bypass 模式）响应端点:** 处于 ROM bypass 阶段时，ROM 固件将 `EVENT_RXD` 命令响应发送到 **EP 0x84 (DataBulkInAddr, RX_RING0)**。驱动必须在 EP 0x84 上提交 Bulk-IN URB 来等待与接收 `need_wait` ACK（包括 `CmdRestartDLReq`、`CmdAddressLenReq`、`CmdFwStartReq`）。
- **运行期（Normal 模式）响应端点:** 固件启动后（Normal 模式），命令响应切换至 **EP 0x85 (CommandRspBulkInAddr, RX_RING1)**。

### 热探测 restart-dl 等待语义（非阻塞）

热探测（RAM 固件已在运行）时，驱动发送 `CmdRestartDLReq`（cid=0xEF, EP 0x08）后：

- **ACK 不作为失败判据**：RAM 固件可能不响应 restart ACK（例如固件刚下载运行 <~17s 即被重新探测，或 MCU 忙）。ACK 超时仅记录日志，不返回错误。
- **成功判据为轮询 TOP_MISC2**：发送 restart 后，驱动轮询 `MT7603_TOP_MISC2` 等待 ROM ready（`bit0=1 && bit1=0`，即 `(val & 0x03) == 0x01`，上限 500 次 × 1ms）。这与厂商 `AndesMTLoadFwMethod1`（andes_mt.c:2440-2470）一致：`CmdRestartDLReq` 后不等待 ACK，直接轮询 ROM ready。
- **仅轮询超时才失败**：TOP_MISC2 在 500ms 内未出现 ROM ready 状态 → 下载失败返回 `-ETIMEDOUT`。

## 3.2 EVENT_RXD 响应头结构

ROM/Firmware 在 Bulk IN 返回的 `EVENT_RXD` 为 12/16 字节头：
- `DW0 (0..3)`: `FW_RXD_0 { length: u16 = 12, pkt_type_id: u16 = 0xE000 }`
- `DW1 (4..7)`: `FW_RXD_1 { eid: u8, seq_num: u8, rsv: u16 }`
  - `CmdRestartDLReq` 响应: `eid = 0xEF (MT_RESTART_DL_RSP)`, `seq_num` 匹配发送时的 `seq`
  - `CmdAddressLenReq` 响应: `eid = 0x01 (MT_TARGET_ADDRESS_LEN_RSP)`, `seq_num` 匹配发送时的 `seq`
  - `CmdFwStartReq` 响应: `eid = 0x01 (MT_TARGET_ADDRESS_LEN_RSP)`, `seq_num` 匹配发送时的 `seq`
- `DW2 (8..11)`: `FW_RXD_2 { ext_eid: u8, rsv: u24 }`

## 4. Interface / API Contract

- **Inputs:** `cmd_type: u8`, `seq: u8`, `payload: *const u8`, `payload_len: usize`, `out_buf: *mut u8`, `max_out_len: usize`, `out_written: *mut usize`
- **Outputs:** 带有 `FW_TXD` command header（下载期 12 字节，运行期 32 字节，见 §2）的完整 Bulk OUT 数据包（不含 4 字节 padding，padding 由 C 发送层追加）
- **Errors:**
  - `0`: 成功
  - `-22` (`EINVAL`): 入参空指针
  - `-28` (`ENOSPC`): `max_out_len` 小于 `payload_len + header_size`

## 5. Acceptance Criteria (BDD)

### Feature: MCU Command Packet Framing

#### Scenario 1: [SPEC-MCU-001] Construct FW_TXD Command Header
- **Given** 命令字 `cmd_type = 0x01` (AddressLenReq), 序列号 `seq = 1`, payload 为 12 字节地址/长度载荷，`need_rsp = true`（但非 EXT 命令）
- **When** 调用 `mt7603_rust_build_fw_txd_frame(cid=0x01, pq_id=0x8000, set_query=CMD_NA, ext_cid=0, seq=1, need_rsp=true, payload, out_buf, out_written)`
- **Then** 函数返回 `0`
- **And** `out_written` 等于 `24` (12 字节 Header + 12 字节 Payload)
- **And** `out[0..1]` 包含帧总长度 `24` (小端序)
- **And** `out[2..3]` 包含 `pq_id = 0x8000` (小端序)
- **And** `out[4]` 包含 `cmd_type = 0x01`
- **And** `out[5]` 包含 `pkt_type_id = 0xA0`
- **And** `out[7]` 包含 `seq = 1`
- **And** `out[9]` 包含 `ext_cid = 0x00`
- **And** `out[11]` 包含 `ext_cid_option = 0`（非 EXT 命令，即使 need_rsp=true）
- **And** `out[12..24]` 等于 payload
- **Mapped Test:** `src/rust/src/mcu.rs:test_build_fw_txd_frame`

#### Scenario 2: [SPEC-MCU-002] Output Buffer Overflow Protection
- **Given** payload 长度为 64 字节，但 `max_out_len` 只有 32 字节
- **When** 调用 `mt7603_rust_build_fw_txd_frame(cid=0x01, pq_id=0x8000, set_query=CMD_NA, ext_cid=0, seq=1, need_rsp=true, payload_64B, out_buf=32B, out_written)`
- **Then** 函数返回 `-28` (`ENOSPC`)
- **Mapped Test:** `src/rust/src/mcu.rs:test_build_fw_txd_frame_overflow`

### Feature: Firmware Download Command Construction

#### Scenario 3: [SPEC-MCU-003] Build AddressLenReq Command Frame
- **Given** 固件加载起始地址 `0x100000`、下载长度 `0x0000_0004`（`dl_len=4`）、数据模式 `0x8000_0000`、seq=`1`（首个 need_wait 命令）
- **When** 调用 `mt7603_rust_build_addr_len_req(0x100000, 4, 1, out_buf, out_written)`
- **Then** 函数返回 `0`
- **And** `out_written` 等于 `24`
- **And** `out[4]` 为 `cid = 0x01`
- **And** `out[7]` 为 `seq = 1`
- **And** `out[12..16]` 为 `0x100000` 小端序
- **And** `out[16..20]` 为 `4` 小端序
- **And** `out[20..24]` 为 `0x8000_0000` 小端序
- **Mapped Test:** `src/rust/src/mcu.rs:test_build_addr_len_req`

#### Scenario 4: [SPEC-MCU-004] Build Firmware Start Req Command Frame
- **Given** override=`1`、入口地址 `0x100000`、seq=`2`（第二个 need_wait 命令）
- **When** 调用 `mt7603_rust_build_fw_start_req(1, 0x100000, 2, out_buf, out_written)`
- **Then** 函数返回 `0`
- **And** `out[4]` 为 `cid = 0x02`
- **And** `out[7]` 为 `seq = 2`
- **And** `out[12..16]` 为 `1` 小端序（override）
- **And** `out[16..20]` 为 `0x100000` 小端序（入口地址）
- **Mapped Test:** `src/rust/src/mcu.rs:test_build_fw_start_req`

#### Scenario 4b: [SPEC-MCU-004b] Build Restart Download Req Command Frame
- **Given** RAM 固件已在运行（热探测）、seq=`1`（首个 need_wait 命令）
- **When** 调用 `mt7603_rust_build_restart_dl_req(1, out_buf, out_written)`
- **Then** 函数返回 `0`
- **And** `out_written` 等于 `32`（32 字节完整运行时头，无 payload）
- **And** `out[4]` 为 `cid = 0xEF`
- **And** `out[5]` 为 `pkt_type_id = 0xA0`
- **And** `out[6]` 为 `set_query = CMD_NA(3)`
- **And** `out[7]` 为 `seq = 1`
- **And** `out[9]` 为 `ext_cid = 0x00`
- **And** `out[11]` 为 `ext_cid_option = 0`（非 EXT 命令）
- **Mapped Test:** `src/rust/src/mcu.rs:test_build_restart_dl_req`

#### Scenario 5: [SPEC-MCU-005] Build Firmware Scatter Frame (chunk ≤ 4064B)
- **Given** 一个 32 字节固件块（含开头 `0x46 0x00 0x01 0x00`）
- **When** 调用 `mt7603_rust_build_fw_scatter_frame(chunk, 0, out_buf, out_written)`
- **Then** 函数返回 `0`
- **And** `out[4]` 为 `cid = 0xEE`
- **And** `out[2..3]` 为 `pq_id = 0xC000`
- **And** `out[7]` 为 `seq = 0`
- **And** `out_written` 等于 `12 + 32`
- **And** `out[12..]` 等于原始固件块
- **Mapped Test:** `src/rust/src/mcu.rs:test_build_fw_scatter_frame`

#### Scenario 6: [SPEC-MCU-006] Firmware Scatter Payload Length Cap
- **Given** payload 为 `4064 + 1` 字节
- **When** 调用 `mt7603_rust_build_fw_scatter_frame(...)`
- **Then** 函数返回 `-22` (`EINVAL`)
- **Mapped Test:** `src/rust/src/mcu.rs:test_fw_scatter_payload_cap`

### Feature: Firmware Image Verification

#### Scenario 3: [SPEC-MCU-003] Firmware Image Integrity & E2 Compatibility Check
- **Given** 从 `harness/fixtures/mt7603u_e2.bin` 加载的 74,372 字节固件二进制镜像
- **When** 调用 `mt7603_rust_verify_firmware(fw_buf, fw_len)`
- **Then** 函数返回 `0` (确认属于合法的 MT7603U E2 固件)
- **And** 不合法或损坏的二进制镜像（长度小于 1024 字节或校验不匹配）返回 `-22` (`EINVAL`)
#### Scenario 7: [SPEC-MCU-007] Build Channel Switch Command Frame (运行期 32B 头)
- **Given** 控制信道 `6`、中心信道 `6`、带宽 `0` (BW_20)、TxStream `2`、RxStream `2`、seq `1`（固件已运行，FW_RUN_TIME）
- **When** 调用 `mt7603_rust_build_chan_switch_cmd(6, 6, 0, 2, 2, 1, out_buf, out_written)`
- **Then** 函数返回 `0`
- **And** `out_written` 等于 `68` (32 字节 Header + 36 字节 Payload)
- **And** `out[0..1]` 包含帧总长度 `68` (小端序)
- **And** `out[4]` 为 `cid = 0xED` (`EXT_CID`)
- **And** `out[6]` 为 `set_query = 1` (`CMD_SET`)
- **And** `out[9]` 为 `ext_cid = 0x08` (`EXT_CMD_CHANNEL_SWITCH`)
- **And** `out[11]` 为 `ext_cid_option = 1` (`NEED_ACK`)
- **And** `out[12..32]` 为 20 字节保留区 `au4D3toD7rev[5]`，全部为 `0`
- **And** `out[32]` 为 `ucCtrlCh = 6`
- **And** `out[33]` 为 `ucCentralCh = 6`
- **Mapped Test:** `src/rust/src/mcu.rs:test_build_channel_switch_cmd`
 
#### Scenario 8: [SPEC-MCU-008] Build Radio On/Off Control Command Frame (运行期 32B 头)
- **Given** Radio 状态为 ON (`1`)、seq `1`（固件已运行，FW_RUN_TIME）
- **When** 调用 `mt7603_rust_build_radio_on_off_cmd(true, 1, out_buf, out_written)`
- **Then** 函数返回 `0`
- **And** `out_written` 等于 `36` (32 字节 Header + 4 字节 Payload)
- **And** `out[4]` 为 `cid = 0xED` (`EXT_CID`)
- **And** `out[6]` 为 `set_query = 1` (`CMD_SET`)
- **And** `out[9]` 为 `ext_cid = 0x05` (`EXT_CMD_RADIO_ON_OFF_CTRL`)
- **And** `out[11]` 为 `ext_cid_option = 1` (`NEED_ACK`)
- **And** `out[12..32]` 为 20 字节保留区，全部为 `0`
- **And** `out[32]` 为 `ucWiFiRadioCtrl = 1`
- **Mapped Test:** `src/rust/src/mcu.rs:test_build_radio_on_off_cmd`

#### Scenario 9: [SPEC-MCU-009] Build Channel Privilege Command Frame (运行期 32B 头)
- **Given** 目标信道为 `6`、seq `0`（固件已运行，FW_RUN_TIME）
- **When** 调用 `mt7603_rust_build_ch_privilege_cmd(6, 0, out_buf, out_written)`
- **Then** 函数返回 `0`
- **And** `out_written` 等于 `48` (32 字节 Header + 16 字节 Payload)
- **And** `out[4]` 为 `cid = 0x20` (`CMD_CH_PRIVILEGE`)
- **And** `out[6]` 为 `set_query = 1` (`CMD_SET`)
- **And** `out[12..32]` 为 20 字节保留区，全部为 `0`
- **And** `out[35]` 为 `ucPrimaryChannel = 6`（payload 偏移 3）
- **Mapped Test:** `src/rust/src/mcu.rs:test_build_ch_privilege_cmd`

#### Scenario 10: [SPEC-MCU-010] Build Efuse Buffer Mode Command Frame (运行期 32B 头)
- **Given** eFuse 芯片（EEPROM_EFUSE）、seq `3`（固件已运行，FW_RUN_TIME）
- **When** 调用 `mt7603_rust_build_efuse_buffer_mode_cmd(eeprom, 3, out_buf, out_written)`
- **Then** 函数返回 `0`
- **And** `out_written` 等于 `996` (32 字节 Header + 964 字节 `EXT_CMD_EFUSE_BUFFER_MODE_T`)
- **And** `out[4]` 为 `cid = 0xED` (`EXT_CID`)
- **And** `out[9]` 为 `ext_cid = 0x21` (`EXT_CMD_EFUSE_BUFFER_MODE`)
- **And** `out[11]` 为 `ext_cid_option = 1` (`NEED_ACK`)
- **And** `out[12..32]` 为 20 字节保留区，全部为 `0`
- **And** `out[32]` 为 `ucSourceMode = 0` (`EEPROM_MODE_EFUSE`)
- **And** `out[33]` 为 `ucCount = 0`（固件自读片上 eFuse）
- **And** payload 中 240 个 `BIN_CONTENT_T` 条目（`out[36..996]`）全部为 `0`
- **Mapped Test:** `src/rust/src/mcu.rs:test_build_efuse_buffer_mode_cmd`

#### Scenario 11: [SPEC-MCU-011] Build TX Power Control Command Frame (运行期 32B 头)
- **Given** EEPROM 镜像（字段值见测试）、中心信道 `6`、seq `4`（固件已运行，FW_RUN_TIME）
- **When** 调用 `mt7603_rust_build_tx_power_ctrl_cmd(eeprom, 6, 4, out_buf, out_written)`
- **Then** 函数返回 `0`
- **And** `out_written` 等于 `76` (32 字节 Header + 44 字节 Payload)
- **And** `out[4]` 为 `cid = 0xED` (`EXT_CID`)
- **And** `out[9]` 为 `ext_cid = 0x11` (`EXT_CMD_SET_TX_POWER_CTRL`)
- **And** `out[12..32]` 为 20 字节保留区，全部为 `0`
- **And** `out[32]` 为 `ucCenterChannel = 6`
- **And** `out[35]` 为 `aucTargetPower[0]`（TX0_G_BAND_TARGET_PWR 低字节）
- **Mapped Test:** `src/rust/src/mcu.rs:test_build_tx_power_ctrl_cmd`

#### Scenario 12: [SPEC-MCU-012] Build EDCA_SET Command Frame (运行期 32B 头)

EDCA/WMM 每 AC 竞争参数配置命令。本驱动 MAC 初始化仅通过 `ARB_TQCR0=0xFFFF_FFFF` 使能所有 TX 队列，但从未填写 EDCA 参数表；不配置则 LMAC TX 调度器对数据 AC（EP 0x05 / AC0）无 AIFS/CW/TxOP 而静默丢弃数据帧（4-Way 握手 M2 永远不上 air）。命令语义对齐厂商 `AsicSetAllWmmParam` / `CmdEdcaParameterSet`（`mcu/andes_mt.c:3721`，`EXT_CMD_ID_EDCA_SET=0x27`）。

- **Given** 标准 WMM EDCA 默认参数（按 WMM ACI 0=AC_BE,1=AC_BK,2=AC_VI,3=AC_VO）：
  - Aifsn = [3, 7, 2, 2]、Cwmin = [4, 4, 3, 2]、Cwmax = [10, 10, 4, 3]、Txop = [0, 0, 0x60, 0x2F]
  - 厂商 `wmm_aci_2_hw_ac_queue[0..4]` = [1, 0, 2, 3]（WMM ACI → 硬件 AC 队列索引）
  - `ucWinMin = (1 << Cwmin) - 1`、`u2WinMax = (1 << Cwmax) - 1`（LE16）
  - seq `5`（固件已运行，FW_RUN_TIME）
- **When** 调用 `mt7603_rust_build_edca_set_cmd(5, out_buf, out_written)`
- **Then** 函数返回 `0`
- **And** `out_written` 等于 `68` (32 字节 Header + 36 字节 Payload)
- **And** `out[0..1]` 包含帧总长度 `68` (小端序)
- **And** `out[4]` 为 `cid = 0xED` (`EXT_CID`)
- **And** `out[6]` 为 `set_query = 1` (`CMD_SET`)
- **And** `out[9]` 为 `ext_cid = 0x27` (`EXT_CMD_EDCA_SET`)
- **And** `out[11]` 为 `ext_cid_option = 1` (`NEED_ACK`)
- **And** `out[12..32]` 为 20 字节保留区，全部为 `0`
- **And** `out[32]` 为 `ucTotalNum = 4`（`CMD_EDCA_AC_MAX`）、`out[33..36]` 为 `aucReserve[3]`（全 0）
- **And** `rAcParam[]` 按硬件 AC 队列索引摆放（共 4×8=32 字节，从 `out[36]` 起）：
  - 硬件队列 0（Q_IDX_AC0，对应 WMM ACI 1 = AC_BK）：`ucAcNum=1`、`ucVaildBit=0x0F`、`ucAifs=7`、`ucWinMin=15`、`u2WinMax=1023`(LE16)、`u2Txop=0`(LE16)
  - 硬件队列 1（Q_IDX_AC1，对应 WMM ACI 0 = AC_BE）：`ucAcNum=0`、`ucVaildBit=0x0F`、`ucAifs=3`、`ucWinMin=15`、`u2WinMax=1023`(LE16)、`u2Txop=0`(LE16)
  - 硬件队列 2（Q_IDX_AC2，对应 WMM ACI 2 = AC_VI）：`ucAcNum=2`、`ucVaildBit=0x0F`、`ucAifs=2`、`ucWinMin=7`、`u2WinMax=15`(LE16)、`u2Txop=0x60`(LE16)
  - 硬件队列 3（Q_IDX_AC3，对应 WMM ACI 3 = AC_VO）：`ucAcNum=3`、`ucVaildBit=0x0F`、`ucAifs=2`、`ucWinMin=3`、`u2WinMax=7`(LE16)、`u2Txop=0x2F`(LE16)
- **Mapped Test:** `src/rust/src/mcu.rs:test_build_edca_set_cmd`

