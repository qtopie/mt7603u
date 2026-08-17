# Schema Spec: FFI Types & Memory Contract

## 1. Overview
定义 C 骨架层与 Rust 静态库之间的 FFI 结构体布局、对齐要求与内存借用约定。所有导出/导入结构体必须标记为 `repr(C)` 并在两端强保证内存一致性。

## 2. Structure Definitions

### 2.1 `struct reg_write_op`
描述一个离散的寄存器写操作项，用于 Rust 传回初始化序列给 C 侧批量执行。

```c
// C 声明 (include/mt7603u_rust.h)
struct reg_write_op {
    uint32_t addr;
    uint32_t val;
};
```

```rust
// Rust 声明 (src/rust/src/ffi.rs)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegWriteOp {
    pub addr: u32,
    pub val: u32,
}
```

### 2.2 `struct mt7603_eeprom_data`
解析后的 EEPROM 字段映射结构体。

```c
// C 声明
struct mt7603_eeprom_data {
    uint8_t mac_addr[6];
    uint8_t tx_power_2g[14];
    uint8_t nic_config;
    uint8_t country_code[2];
    uint16_t eeprom_version;
    int8_t rssi_offset_2g;  /* EEPROM 0x46 signed, clamped to [-10,10] */
    uint8_t is_valid;
};
```

```rust
// Rust 声明
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct EepromData {
    pub mac_addr: [u8; 6],
    pub tx_power_2g: [u8; 14],
    pub nic_config: u8,
    pub country_code: [u8; 2],
    pub eeprom_version: u16,
    pub rssi_offset_2g: i8,
    pub is_valid: u8,
}
```

### 2.3 `struct mt7603_rx_info`
802.11 / RxWI 数据包解析结果。

```c
// C 声明
struct mt7603_rx_info {
    uint16_t pkt_len;
    uint16_t hdr_len;
    int8_t rssi;       /* dBm, 0 = unknown (Group3 absent or IBRssi0==0) */
    uint8_t channel;
    uint8_t rate;
    uint8_t is_beacon;
    uint8_t is_data;
    uint8_t is_crc_error;
};
```

```rust
// Rust 声明
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RxInfo {
    pub pkt_len: u16,
    pub hdr_len: u16,
    pub rssi: i8,
    pub channel: u8,
    pub rate: u8,
    pub is_beacon: u8,
    pub is_data: u8,
    pub is_crc_error: u8,
}
```

### 2.4 `struct mt7603_sta_bss_info`
扫描/解析 Beacon 得到的 BSS 信息结构体。

```c
// C 声明
struct mt7603_sta_bss_info {
    uint8_t bssid[6];
    uint8_t ssid[32];
    uint8_t ssid_len;
    uint8_t channel;
    int8_t rssi;
    uint16_t capability;
};
```

```rust
// Rust 声明
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StaBssInfo {
    pub bssid: [u8; 6],
    pub ssid: [u8; 32],
    pub ssid_len: u8,
    pub channel: u8,
    pub rssi: i8,
    pub capability: u16,
}
```

## 3. Memory & FFI Safety Rules

1. **Alignment & Padding**: 所有结构体字段按自然对齐（32-bit 对齐 4 字节，16-bit 对齐 2 字节）。严禁在结构体中间出现未被显式说明的隐式填充段。
2. **Buffer Out Parameters**: 所有输出指针（`*mut T`）由 C 端声明在栈上或用 `kmalloc` 分配，Rust 仅通过写 Deref (`*out = val`) 填入结果。
3. **Null Check**: Rust FFI 入口必须校验所有 raw pointer 不为 `core::ptr::null()`，非法时返回 `-EINVAL` (`-22`)。
4. **Panic Isolation**: Rust 代码库设置 `panic = "abort"`，禁止 panic 跨边界回溯。
