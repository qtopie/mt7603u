# Module Spec: EEPROM Parsing & Calibration

## 1. Overview
规范定义 EEPROM 二进制 dump 解析模块的行为。该模块在 Rust 静态库中实现（`src/rust/src/eeprom.rs`），负责校验 EEPROM 长度与 Header，提取 MAC 地址、发射功率标定参数与国家码。

## 2. Interface / API Contract

- **Input:** `buf: *const u8`, `len: usize` (标准 512 或 256 字节)
- **Output:** `out: *mut EepromData`
- **Errors:**
  - `0`: 成功
  - `-22` (`EINVAL`): 空指针或长度小于 256 字节
  - `-5` (`EIO`): 校验和错误且无法 fallback
- **RSSI 校准偏移（EEPROM `0x46`）:** `out->rssi_offset_2g` = `buf[0x46]` 的有符号值（厂商 `EEPROM_RSSI_BG_OFFSET = 0x46`，`common/eeprom.c:122-123`）；超出 `-10..10` 范围时钳位为 `0`（厂商 `common/eeprom.c:261-262`）。C 侧 RX 路径将其作为 `rssi_offset` 传入 `parse_rx_frame`。

## 3. Acceptance Criteria (BDD)

### Feature: EEPROM Binary Parsing

#### Scenario 1: [SPEC-EEPROM-001] Valid EEPROM Dump Parsing
- **Given** 一个有效的 512 字节 EEPROM buffer，包含合法 MAC 地址 `00:0C:43:76:03:01`
- **When** 调用 `mt7603_rust_parse_eeprom(buf, 512, out)`
- **Then** 函数返回 `0`
- **And** `out->mac_addr` 等于 `00:0C:43:76:03:01`
- **And** `out->is_valid` 等于 `1`
- **Mapped Test:** `src/rust/src/eeprom.rs:test_parse_valid_eeprom`

#### Scenario 2: [SPEC-EEPROM-002] Invalid Buffer Size / Null Pointer
- **Given** 一个长度为 `128` 字节的无效 buffer 或 `null` 指针
- **When** 调用 `mt7603_rust_parse_eeprom(buf, 128, out)`
- **Then** 函数返回 `-22` (`EINVAL`)
- **And** `out` 内容保持不变
- **Mapped Test:** `src/rust/src/eeprom.rs:test_parse_invalid_buffer`

#### Scenario 4: [SPEC-EEPROM-004] RSSI Offset Extraction
- **Given** 一个有效的 512 字节 EEPROM buffer，偏移 `0x46` 处为 `0x02`（`0x47` 为 `0x01`）
- **When** 调用 `mt7603_rust_parse_eeprom(buf, 512, out)`
- **Then** 函数返回 `0`
- **And** `out->rssi_offset_2g` 等于 `2`
- **Mapped Test:** `src/rust/src/eeprom.rs:test_parse_rssi_offset`

#### Scenario 5: [SPEC-EEPROM-005] RSSI Offset Out-of-Range Clamp
- **Given** 偏移 `0x46` 处为 `0x30`（=48，超出 -10..10 校验范围）
- **When** 调用 `mt7603_rust_parse_eeprom(buf, 512, out)`
- **Then** `out->rssi_offset_2g` 等于 `0`
- **Mapped Test:** `src/rust/src/eeprom.rs:test_parse_rssi_offset_clamp`

#### Scenario 3: [SPEC-EEPROM-003] Default Fallback MAC Address Generation
- **Given** EEPROM 中 MAC 地址全为 `0xFF` 或 `0x00`
- **When** 调用 `mt7603_rust_parse_eeprom(buf, 512, out)`
- **Then** 函数返回 `0`
- **And** `out->mac_addr` 填充为随机/默认合法 MAC（如 `00:0C:43:00:00:01`）
- **And** `out->is_valid` 等于 `1`
- **Mapped Test:** `src/rust/src/eeprom.rs:test_parse_fallback_mac`
