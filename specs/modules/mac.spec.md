# Module Spec: MAC & Register Sequence Generator

## 1. Overview
规范定义 MAC 初始化与 BBP/PHY 寄存器序列生成模块（`src/rust/src/mac.rs`）。该模块不进行实际硬件 I/O，而是返回按顺序排布的 `RegWriteOp` 数组供 C 骨架执行。

## 2. Interface / API Contract

- **Inputs:** `ops_buf: *mut RegWriteOp`, `max_ops: usize`, `out_count: *mut usize`
- **Outputs:** 填入的写指令序列及实际生成的操作数
- **Errors:**
  - `0`: 成功
  - `-22` (`EINVAL`): 指针为空
  - `-28` (`ENOSPC`): `max_ops` 缓冲区不足以容纳完整的初始化序列

## 3. Acceptance Criteria (BDD)

### Feature: MAC Initialization Sequence Generation

#### Scenario 1: [SPEC-MAC-001] Complete MAC CR Init Sequence Generation
- **Given** 一个足够大的 `ops_buf` 数组（容量 >= 128）
- **When** 调用 `mt7603_rust_get_mac_init_sequence(ops_buf, 128, out_count)`
- **Then** 函数返回 `0`
- **And** `out_count` 大于 0
- **And** 序列中包含对 `AGG_AWSCR` (`0x0002_1248`) 和 `RMAC_RMACDR` (`0x0002_1878`) 的初始化写入指令
- **Mapped Test:** `src/rust/src/mac.rs:test_mac_init_sequence`

#### Scenario 2: [SPEC-MAC-002] Buffer Overflow Protection
- **Given** 一个小容量 `ops_buf` 数组（容量 = 2）
- **When** 调用 `mt7603_rust_get_mac_init_sequence(ops_buf, 2, out_count)`
- **Then** 函数返回 `-28` (`ENOSPC`)
- **Mapped Test:** `src/rust/src/mac.rs:test_mac_init_buffer_overflow`

### Feature: Channel Switching Sequence

#### Scenario 3: [SPEC-MAC-003] Channel 1-14 Switching Register Sequence
- **Given** 目标信道为 Channel 6, 带宽 20MHz
- **When** 调用 `mt7603_rust_get_channel_sequence(6, 0, ops_buf, 128, out_count)`
- **Then** 函数返回 `0`
- **And** 序列包含对 `RMAC_CHFREQ (0x0002_1890)` 写入 `1`
- **Mapped Test:** `src/rust/src/mac.rs:test_channel_switch_sequence`

#### Scenario 4: [SPEC-MAC-004] RMAC_CHFREQ MUST be written on every channel switch
- **Given** 驱动正处于信道切换流程 (`mt7603_set_channel`)
- **When** 向固件下发 `CmdChannelSwitch`/`CmdSetTxPowerCtrl` MCU 命令后
- **Then** 驱动必须写 `RMAC_CHFREQ (0x0002_1890)` = `1`
- **And** 不得依赖固件复位后的默认值（该位不可靠，残留 0 时 RMAC RX 前端无信道频率，EP 0x84 永无数据而 TX 正常）
- **Rationale:** 厂商 `AsicSwitchChannel` (hw_ctrl/cmm_asic_mt.c:399-409) 在 `ChipSwitchChannel` 之后无条件 `RTMP_IO_WRITE32(RMAC_CHFREQ, 1)`；Rust `build_channel_sequence` (mac.rs:189-204) 的首 op 即此写入，C 侧此前遗漏调用该序列导致间歇性 RX 断流。
- **Mapped Test:** `src/c/mac80211.c:mt7603_set_channel`（人工硬件回归：反复物理拔插 RX beacon 持续上报）

#### Scenario 5: [SPEC-MAC-005] Own MAC Address Register Sequence
- **Given** 网卡 MAC 地址 `[0x00, 0x0c, 0x43, 0x76, 0x03, 0x01]`
- **When** 调用 `build_own_mac_sequence(&mac, &mut ops)`
- **Then** 函数返回 `2`
- **And** `ops[0]` 写入 `RMAC_OMA0R0 (0x0002_1824)` 值为 `0x76430c00`
- **And** `ops[1]` 写入 `RMAC_OMA0R1 (0x0002_1828)` 值为 `0x00010103` (含 `1 << 16` ENABLE)
- **Mapped Test:** `src/rust/src/mac.rs:test_own_mac_sequence`

### Feature: Radio On/Off Lifecycle (mac80211 glue)

#### Scenario 5: [SPEC-MAC-005] Radio-off MUST NOT be sent on mac80211 stop
- **Given** MT7603U 固件已下载并运行（RAM 固件，`TOP_MISC2 bit1 = 1`）
- **When** 接口 down（`mac80211_stop`）或驱动卸载（`rmmod`）触发
- **Then** 驱动**不得**向固件下发 `EXT_CMD_RADIO_ON_OFF_CTRL` (radio-off, 0xED/0x05)
- **And** 停止路径仅做 MAC/DMA 级关闭：清空 RX 接收环 (`stop_rx`) 与击毙 TX URB (`usb_kill_anchored_urbs`)
- **Rationale:** 厂商 MT7603 明确不实现 radio on/off 卸载路径 —— `AsicRadioOn/AsicRadioOff = NULL` (`chips/mt7603.c`)，`RT28xxUsbAsicRadioOff` 与 `CmdRadioOnOffCtrl(WIFI_RADIO_OFF)` 均被注释 (`usb_main_dev.c:622`, `rtmp_init_inf.c:1279`)。对仍驻留 RAM 的固件下发 radio-off 会使 MCU 命令接口（`restart-dl`/EP 0x84/0x85）静默失效，导致下一次 insmod 热探测 `-110` 超时。
- **Mapped Test:** `src/c/mac80211.c` stop 路径（人工硬件回归：`rmmod` + `insmod` 热探测 100% 通过）
