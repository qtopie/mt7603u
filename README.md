# MT7603U Linux WiFi Driver (C + Rust / mac80211)

A modern, memory-safe Linux kernel driver for MediaTek **MT7603U** 802.11b/g/n USB wireless network adapters.

Built with a hybrid architecture combining a lean C kernel-module skeleton with a pure `no_std` Rust logic engine, integrated directly into the standard Linux **`mac80211`** wireless networking subsystem.

---

## 🌟 Key Highlights

- **`mac80211` Subsystem Integration**: Seamless integration with standard Linux wireless tools (`iw`, `wpa_supplicant`, `hostapd`, `NetworkManager`).
- **Memory-Safe Core**: Frame parsing, EEPROM/eFuse decoding, MCU command generation, and register sequence transitions implemented in 100% `no_std` Rust.
- **Robust Firmware Lifecycle**: Andes N9 MCU firmware uploader with ROM ready polling, dynamic sequence IDs, and warm restart-dl support.
- **Spec-First Engineering**: Every hardware interaction and protocol state machine is strictly governed by behavioral contracts in [`specs/`](specs/) and verified via automated test harnesses.
- **Multi-Mode Support**: Supports Station (STA), Access Point (AP), and Monitor modes.

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Linux Kernel (mt7603u.ko)                     │
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                     C Skeleton Layer                      │  │
│  │  • USB Device Driver (probe / disconnect / power mgmt)    │  │
│  │  • mac80211 Subsystem Glue (struct ieee80211_hw & ops)    │  │
│  │  • URB Allocation, Anchoring & Zero-Leak Teardown         │  │
│  │  • sk_buff Management & Zero-Copy Packet Rings            │  │
│  └─────────────────────────────┬─────────────────────────────┘  │
│                                │ C ABI (no_std FFI)             │
│  ┌─────────────────────────────▼─────────────────────────────┐  │
│  │          Rust Logic Engine (libmt7603u_logic.a)           │  │
│  │  • EEPROM / eFuse Parser & RSSI Calibration               │  │
│  │  • Register State Sequence Generator (MAC/BBP/Channel)    │  │
│  │  • 802.11 / RxWI / TxWI Packet Transcoder                 │  │
│  │  • MCU Command Packet Builder & Header Serializer         │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

For in-depth architectural decisions, see [RFC-0001: Architecture](docs/rfcs/0001-architecture.md) and [System Design](docs/system-design.md).

---

## 🌿 Repository Branches

This repository maintains clear isolation between the upstream vendor reference and the modern rewritten driver:

- **`main`**: The active C/Rust driver implementation based on modern Linux `mac80211`.
- **`vendor`**: The original MediaTek vendor driver codebase (v1.14), preserved as a read-only historical reference.

---

## 📡 Supported Hardware

| Property | Value |
|---|---|
| **Chipset** | MediaTek MT7603U (802.11b/g/n, 2T2R, 2.4 GHz) |
| **Supported USB IDs** | `0x0E8D:0x7603`, `0x0E8D:0x760C` |
| **Host Interface** | USB 2.0 High-Speed |
| **Target Kernels** | Linux 5.x / 6.x / 7.x (tested on 7.0.x) |

---

## 📱 Reference Test Device: 360 随身WiFi 3

This driver is validated and tested on the **Qihoo 360 随身WiFi 3** ([Official Page](https://wifi.360.cn/easy)), an MT7603U-powered USB dongle with dual onboard PIFA antennas.

<p align="center">
  <img src="docs/assets/360_wifi_3.jpg" alt="360 随身WiFi 3" width="600" />
</p>

### Hardware Specifications

| Specification | Details |
|---|---|
| **Product Model** | 360 随身WiFi 3 (Qihoo 360 Portable WiFi 3) |
| **USB ID** | `0x0E8D:0x760C` / `0x0E8D:0x7603` |
| **Bus Interface** | USB 2.0 |
| **Protocol Standards** | IEEE 802.11n, IEEE 802.11g, IEEE 802.11b |
| **Channel Bandwidth** | 20 MHz / 40 MHz |
| **Frequency Range** | 2.412 GHz – 2.4835 GHz |
| **Operating Channels** | Channels 1 – 11 (US/CN) / Channels 1 – 13 (ETSI) |
| **Antenna Design** | Built-in Dual MIMO PIFA Antennas (2T2R configuration) |
| **Max Transmission Power** | 19 dBm (Max) |
| **Transmission Rates** | • **802.11b**: 1, 2, 5.5, 11 Mbps<br>• **802.11g**: 6, 9, 12, 18, 24, 36, 48, 54 Mbps<br>• **802.11n**: Up to 300 Mbps (MCS0–MCS15, 2 Streams) |
| **Encryption / Security** | WPA-PSK / WPA2-PSK (TKIP/CCMP) |
| **Supported Features** | Auto rate adaptation, QoS-WMM, WMM-PS, Cisco CCX, Infrastructure & Ad-Hoc modes, Power Management, Activity LED |
| **Operating Environment** | Working Temp: 0℃ ~ 40℃, Storage Temp: -20℃ ~ 70℃<br>Working Humidity: 10% ~ 90% RH (non-condensing) |
| **Physical Dimensions** | 49 mm × 19 mm × 8.1 mm, Weight: 6.5 g (ABS + PC plastic casing) |

---

## 🚀 Getting Started

### Prerequisites

Ensure you have the required build tools and Rust toolchain installed:

```bash
# Ubuntu / Debian
sudo apt update
sudo apt install -y build-essential linux-headers-$(uname -r) pkg-config libelf-dev

# Rust toolchain (nightly or stable with no_std target)
rustup target add x86_64-unknown-none
```

### Firmware Setup

Place the MT7603U firmware binary (`mt7603u_e2.bin`) in `/lib/firmware/mt7603u.bin`:

```bash
sudo cp harness/fixtures/mt7603u_e2.bin /lib/firmware/mt7603u.bin
```

### Building the Driver

To compile both the Rust static library and the Linux kernel module:

```bash
make
```

### Loading the Driver

```bash
# Load mac80211 dependency if not already loaded
sudo modprobe mac80211

# Insert mt7603u kernel module
sudo insmod mt7603u.ko
```

Verify device initialization via `dmesg`:

```bash
sudo dmesg | grep mt7603
```

Check the network interface using `iw`:

```bash
iw dev
```

---

## 🧪 Testing & Validation

The codebase follows strict **Harness-Driven Development (HDD)**. You can run all unit tests, spec contracts, and linter checks without physical hardware:

```bash
# Run full automated validation suite
./scripts/check.sh

# Run harness tests and BDD specs only
./scripts/check-harness.sh
```

### Hardware Active Scan (Optional)

If a physical MT7603U adapter is connected:

```bash
sudo ./scripts/test-hardware-scan.sh
```

---

## 📂 Project Layout

```
├── .agents/          # Agent dynamic boards, tasks, and configurations
├── AGENTS.md         # System operating guidelines & safety rules
├── Makefile          # Kbuild + Cargo hybrid build system
├── docs/             # Technical designs, RFCs, and bug RCA reports
│   ├── bugs/         # Root Cause Analysis (RCA) records
│   └── rfcs/         # Architectural RFC proposals
├── harness/          # Test harness, hardware mocks, and test fixtures
│   ├── fixtures/     # Reference firmware binaries and dumps
│   ├── mocks/        # Mock implementations for RegOps and USB
│   └── runners/      # BDD scenario runners
├── scripts/          # Automation toolchain (check, build, test)
├── specs/            # Single Source of Truth (SSOT) behavioral specifications
│   ├── modules/      # Module specifications (MCU, STA, AP, RX/TX, EEPROM)
│   └── schemas/      # Register maps and FFI type definitions
└── src/
    ├── c/            # C kernel module skeleton (USB, mac80211, RegOps)
    └── rust/         # Rust no_std core logic engine (crates / modules)
```

---

## 📜 License

This project is licensed under the GPLv2 (compatible with Linux kernel driver guidelines).
