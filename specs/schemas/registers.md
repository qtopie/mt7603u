# Schema Spec: MT7603U Register Map & Bitfields

## 1. Overview
定义 MT7603U 关键寄存器地址与位域定义。作为硬件事实规范（Single Source of Truth），所有 Rust 序列与 C 寄存器 I/O 必须严格遵循本定义。数据源自 MTK 官方驱动 `include/chip/mt7603.h`、`mac/mac_mt/*.h` 及 `common/mt_io.c`（地址映射）。

## 2. Register Address Base Domains (HIF 视角)

MT7603U 寄存器通过 USB vendor request 访问。`regops` 必须先经 `mt_physical_addr_map` 将 HIF 地址映射为全局物理地址，再发起 vendor request。

### 2.1 地址映射域（HIF 地址 → 全局物理地址）

| HIF 地址范围 | 映射公式 | 说明 |
|---|---|---|
| `[0x0000, 0x2000)` | `0x8002_0000 + addr` | `TOP_CFG` / AON 顶部配置 |
| `[0x2000, 0x4000)` | `0x8000_0000 + (addr - 0x2000)` | `MCU_CFG`（含 TOP_MISC2 0x1134 → 0x8002_1134） |
| `[0x4000, 0x8000)` | `0x5000_0000 + (addr - 0x4000)` | `PDMA_CFG`（含 HIF/SCH，如 SCH_REG4 0x4594 → 0x5000_0594） |
| `[0x8000, 0x10000)` | `0xA000_0000 + (addr - 0x8000)` | `PSE_CFG` |
| `[0x10000, 0x20000)` | `0x6020_0000 + (addr - 0x10000)` | `WF_PHY` |
| `[0x20000, 0x40000)` | 查 `mt_mac_cr_range` 表（见 2.2） | MAC 功能块 |
| `[0x40000, 0x80000)` | `0xA500_0000 + (addr - 0x40000)` | `WTBL` |
| `[0xC0000, 0xC0100)` | `0x800C_0000 + (addr - 0xC0000)` | PSE Client |
| 其他 | `addr` 原样 | — |

### 2.2 `mt_mac_cr_range` 表（MAC 功能块地址段，HIF 0x20000–0x40000）

| 功能块 | 全局基地址 `mt_mac_cr_range[n]` | HIF 段起始 `[n+1]` | 段长 `[n+2]` |
|---|---|---|---|
| `WF_CFG` | `0x6000_0000` | `0x20000` | `0x200` |
| `WF_TRB` | `0x6010_0000` | `0x21000` | `0x200` |
| `WF_AGG` | `0x6011_0000` | `0x21200` | `0x200` |
| `WF_ARB` | `0x6012_0000` | `0x21400` | `0x200` |
| `WF_TMAC` | `0x6013_0000` | `0x21600` | `0x200` |
| `WF_RMAC` | `0x6014_0000` | `0x21800` | `0x200` |
| `WF_SEC` | `0x6015_0000` | `0x21A00` | `0x200` |
| `WF_DMA` | `0x6016_0000` | `0x21C00` | `0x200` |
| `WF_CFGOFF` | `0x6017_0000` | `0x21E00` | `0x200` |
| `WF_PF` | `0x6018_0000` | `0x22000` | `0x1000` |
| `WF_WTBLOFF` | `0x6019_0000` | `0x23000` | `0x200` |
| `WF_ETBF` | `0x601A_0000` | `0x23200` | `0x200` |
| `WF_LPON` | `0x6030_0000` | `0x24000` | `0x400` |
| `WF_INT` | `0x6031_0000` | `0x24400` | `0x200` |
| `WF_WTBLON` | `0x6032_0000` | `0x28000` | `0x4000` |
| `WF_MIB` | `0x6033_0000` | `0x2C000` | `0x200` |
| `WF_AON` | `0x6040_0000` | `0x2D000` | `0x200` |

映射算法（与厂商 `mt_physical_addr_map` 一致）：遍历表，若 `segment_start <= addr < segment_start + segment_len`，则 `global = table_base + (addr - segment_start)`。

## 3. Key Register Specifications

### 3.1 TOP Block (`0x0000_0000`)
- **`TOP_HVR` (`0x0000_0000`)**: Hardware Version Register.
  - Bit [31:16]: Chip ID (Expected: `0x7603` or `0x7628`).
- **`TOP_FVR` (`0x0000_0004`)**: Firmware Version Register.
- **`TOP_STRAP_STA` (`0x0000_0010`)**: Strap Status Register.
- **`TOP_MISC2` (`0x1134`)**: MCU 状态寄存器（固件下载关键轮询位）。
  - Bit [0]: `RomReady` — ROM 代码就绪（`1` 就绪）。
  - Bit [1]: `RamRunning` — RAM 固件已运行（`1` 运行中）。
  - 固件下载时序：
    - 发送 AddressLenReq 前：轮询 `RomReady==1 && RamRunning==0`。
    - 发送 FwStartReq 后：轮询 `RamRunning==1`。

### 3.2 HIF/SCH Block (`0x4000`)
- **`SCH_REG4` (`0x4594`)**: 调度器控制寄存器（固件下载时切换 bypass 模式）。
  - Bit [0:3] (`SCH_REG4_FORCE_QID`): 强制 QID 值。USB 固件下载时置 `8`。
  - Bit [5] (`SCH_REG4_BYPASS_MODE`): `1` = bypass 模式（固件下载），`0` = 正常模式。
  - Bit [8]: PSE 复位脉冲（恢复时先置 `1` 再清 `0`）。
  - `SCH_REG4_FORCE_QID_MASK = 0x0f`，`SCH_REG4_BYPASS_MODE_MASK = 0x20`。
- **`MT_HIF_BASE` (`0x4000`)**: HIF 基地址。

### 3.3 TMAC Block (`0x0002_1600`)
- **`TMAC_TCR` (`0x0002_1600`)**: Transmit Control Register.
- **`TMAC_CDTR` (`0x0002_1608`)**: CCK Delay & Timing Register (`0x003000E7`).
- **`TMAC_RRCR` (`0x0002_160C`)**: Rate Retry Control Register (`0x00000004`).
- **`TMAC_TRCR` (`0x0002_1614`)**: Transmit Retry Control Register (`0x80000000`).

### 3.4 RMAC Block (`0x0002_1800`)
- **`RMAC_RFCR` (`0x0002_1800`)**: RMAC Rx Filter Control Register (`0x00000000` = Promiscuous).
- **`RMAC_OMA0R0` (`0x0002_1824`)**: Own MAC Address 0 Low (bytes 0..3).
- **`RMAC_OMA0R1` (`0x0002_1828`)**: Own MAC Address 0 High (bytes 4..5 | (1 << 16) Enable).
- **`RMAC_RMACDR` (`0x0002_1878`)**: RMAC Drop Register (`0x40000000` = SELECT_RXMAXLEN_20BIT).
- **`RMAC_RMCR` (`0x0002_1880`)**: RMAC Rx Stream & SMPS Mode Control (`0x00F00000` = Stream 0+1 Enable, Disable SMPS).
- **`RMAC_MAXMINLEN` (`0x0002_1898`)**: Max/Min Frame Length Filter (`0x00019000` = Max 102400).
- **`RMAC_RFCR1` (`0x0002_18A4`)**: RMAC Rx Filter Control Register 1.

### 3.5 AGG Block (`0x0002_1200`)
- **`AGG_AWSCR` (`0x0002_1248`)**: Aggregation Window Size Control 0.
- **`AGG_AWSCR1` (`0x0002_124C`)**: Aggregation Window Size Control 1.
- **`AGG_AALCR` (`0x0002_1250`)**: Aggregation Limit Control 0.
- **`AGG_AALCR1` (`0x0002_1254`)**: Aggregation Limit Control 1.
- **`AGG_PCR1` (`0x0002_125C`)**: RTS Threshold Control.

### 3.6 DMA Block (`0x0002_1C00`)
- **`DMA_RCFR0` (`0x0002_1C70`)**: Rx Frame Filter Register 0 (`0xC0210000`).
- **`DMA_VCFR0` (`0x0002_1C78`)**: RxVector Routing Register 0 (`0x00002000` = RxRing 1).
- **`DMA_TMCFR0` (`0x0002_1C7C`)**: TMR Routing Register 0.

### 3.7 PSE Client Block (`0x000C_0000`)
- **`PSE_CLIENT_TX_PAD_DW2` (`0x000C_0040`)**: Short TXD Template DW2.
- **`PSE_CLIENT_TX_PAD_DW3` (`0x000C_0044`)**: Short TXD Template DW3 (`0x00000001` remain_tx_cnt=1).
- **`PSE_CLIENT_TX_PAD_DW4` (`0x000C_0048`)**: Short TXD Template DW4.
- **`PSE_CLIENT_TX_PAD_DW5` (`0x000C_004C`)**: Short TXD Template DW5 (`0x00000020` PID_DATA_AMPDU).
- **`PSE_CLIENT_TX_PAD_DW6` (`0x000C_0050`)**: Short TXD Template DW6.
- **`PSE_CLIENT_RXINF` (`0x000C_0068`)**: Rx Group 1, 2, 3 Enable (`0x00000007`).

### 3.7 UDMA (USB DMA) Block (`0x0002_4000`)
- **`USB_DMA_CFG` (`0x0002_4000`)**: USB DMA Configuration Register.
  - Bit [31]: `TxBusBusy`
  - Bit [30]: `RxBusBusy`
  - Bit [20]: `RxBulkEn`
  - Bit [19]: `TxBulkEn`
- **`USB_CYC_CFG` (`0x0002_4004`)**: USB Cycle Config Register.

## 4. Vendor Request Command Codes (EP0)

USB Vendor Requests 用于通过 Control Transfer 读写寄存器。**地址必须先用 `mt_physical_addr_map` 映射为全局物理地址**，再编码进 request：

- `bRequest = 0x63` (READ): `wValue = addr[31:16]`, `wIndex = addr[15:0]`
- `bRequest = 0x66` (WRITE): `wValue = addr[31:16]`, `wIndex = addr[15:0]`
- `bmRequestType = USB_TYPE_VENDOR | USB_RECIP_DEVICE`，方向按读/写置 `USB_DIR_IN` / `USB_DIR_OUT`
- 数据以 4 字节小端传输。

来源：`common/mtusb_io.c`（`mtusb_multiread` / `mtusb_multiwrite`）、`include/iface/rtmp_usb.h`（`DEVICE_VENDOR_REQUEST_OUT=0x40` / `IN=0xc0`）。

## 5. USB 端点布局

| 用途 | 端点号 | 说明 |
|---|---|---|
| 数据 TX (AC0) | `0x05` (EP5) | `WMM0ACBulkOutAddr[0]` |
| 数据 TX (AC1) | `0x04` (EP4) | `WMM0ACBulkOutAddr[1]` |
| 数据 TX (AC2) | `0x06` (EP6) | `WMM0ACBulkOutAddr[2]` |
| 数据 TX (AC3) | `0x07` (EP7) | `WMM0ACBulkOutAddr[3]` |
| 管理/命令 TX | `0x08` (EP6 别名) | `CommandBulkOutAddr`（最高优先级） |
| 数据 RX | `0x84` (EP4 IN) | `DataBulkInAddr` |
| 命令响应 RX | `0x85` (EP5 IN) | `CommandRspBulkInAddr` |

来源：`chips/mt7603.c`（`CommandBulkOutAddr=0x8`、`WMM0ACBulkOutAddr={0x5,0x4,0x6,0x7,0x8}`、`DataBulkInAddr=0x84`、`CommandRspBulkInAddr=0x85`）。
