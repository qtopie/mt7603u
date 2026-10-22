# MT7603U Linux WiFi Driver (C + Rust / mac80211)

A modern, high-performance, and memory-safe Linux kernel driver for MediaTek **MT7603U** 802.11b/g/n 2T2R USB wireless network adapters.

Built with a hybrid architecture combining a lean C kernel-module skeleton with a pure `no_std` Rust logic engine, integrated directly into the standard Linux **`mac80211`** wireless networking subsystem.

---

## 🌟 Key Features

### 🚀 802.11n 2x2 MIMO High Throughput (Up to 300 Mbps)
- **Dual Spatial Streams (2T2R)**: Supports 2-Stream MIMO with MCS 0 through MCS 15.
- **40 MHz High Throughput (HT40)**: Dynamic 20 MHz / 40 MHz channel bandwidth switching and center-frequency alignment.
- **Short Guard Interval (Short GI)**: 400 ns Short GI support across both HT20 and HT40, enabling physical link rates up to 300 Mbps.
- **A-MPDU Aggregation & BlockAck**: High-efficiency frame aggregation and hardware BlockAck session management (`BSCR0/1`, `WTBL2`).
- **Rate Control Integration**: Seamless interoperation with the kernel's `Minstrel-HT` rate control algorithm and hardware transmission status feedback (`TXS`).

### 📱 Station Mode (STA / Client)
- **Active & Passive Scanning**: Complete 802.11b/g/n scanning across 2.4 GHz channels 1–13 with dynamic channel switching.
- **WPA / WPA2-PSK Security**: Robust CCMP (AES) and TKIP cipher suites with hardware-accelerated encryption and decryption.
- **Accurate RSSI Reporting**: Real-time signal strength computation calibrated against onboard eFuse/EEPROM gain offsets.
- **Reliable Networking**: Clean 802.11 authentication and association state machine, zero-loss DHCPv4 lease negotiation, and low-latency TCP/UDP/ICMP traffic.

### 📡 Access Point Mode (AP / SoftAP)
- **Hostapd Integration**: Standard Linux Access Point operation via `NL80211_IFTYPE_AP` and `hostapd`.
- **Hardware TBTT & Beacon Queue**: Hardware LPON TBTT timing synchronization (`LPON_T0CR`) combined with periodic Beacon generation.
- **Client Lifecycle Management**: Station admission tracking, multi-client association, and PTK/GTK encryption key distribution.

### 🔍 Monitor Mode & Raw Frame Injection
- **Promiscuous Sniffing**: Raw over-the-air packet capture (`NL80211_IFTYPE_MONITOR`) with dynamic RMAC RFCR filter configuration.
- **Radiotap Metadata Generation**: Comprehensive per-packet Radiotap headers with Group3 RXV1 PHY metadata (RxRate, Modulation, Channel, RSSI, Short GI).
- **Packet Injection**: Raw 802.11 frame transmission through dedicated hardware beacon and management queues.

### 🛡️ Memory-Safe Hybrid Design
- **100% `no_std` Rust Engine**: Safe parsing and generation for 802.11 frames, EEPROM/eFuse maps, MCU command packets, and register sequences.
- **Lean C Kernel Skeleton**: Robust USB device lifecycle handling, URB anchoring, DMA zero-copy rings, and clean, leak-free teardown.
- **Reliable Firmware Management**: Andes N9 MCU firmware loader featuring ROM-ready polling and warm restart-dl recovery.

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

For detailed technical designs, see [RFC-0001: Architecture](docs/rfcs/0001-architecture.md) and [System Design](docs/system-design.md).

---

## 📡 Supported Hardware & Tested Platforms

### Supported Chipset & USB IDs

| Property | Value |
|---|---|
| **Chipset** | MediaTek MT7603U (802.11b/g/n, 2T2R, 2.4 GHz) |
| **Supported USB IDs** | `0x0E8D:0x7603`, `0x0E8D:0x760C` |
| **Host Interface** | USB 2.0 High-Speed (480 Mbps) |
| **Target Kernels** | Linux 5.10 LTS through Linux 7.x (tested on 5.10.4 and 7.0.x) |

### Verified Target Platforms

| Platform / Board | Architecture | Kernel & Environment | Validated Capabilities |
|---|---|---|---|
| **x86_64 Host PC** | x86_64 | Linux 7.0.0-generic (glibc) | • 802.11n 2T2R 300 Mbps MIMO<br>• STA mode (WPA2-PSK, DHCPv4, Ping)<br>• AP mode (`hostapd`, TBTT, Beacon)<br>• Monitor mode & Radiotap capture |
| **Milk-V Duo S** (SG2000) | RISC-V 64 (`rv64imac`) | Linux 5.10.4-tag- (musl libc) | • USB Host enumeration (`0e8d:760c`)<br>• Andes N9 firmware loading<br>• Ch 1–13 active scan & RSSI reporting<br>• AP mode Beacon broadcasting (`AP-ENABLED`)<br>• Monitor mode promiscuous sniffing<br>• STA Auth / Assoc / EAPOL DATA TX (`LMAC_TX_OK`) |

---

## 📱 Reference Test Device: 360 随身WiFi 3

This driver is tested and validated against the **Qihoo 360 随身WiFi 3** ([Official Page](https://wifi.360.cn/easy)), a popular MT7603U-powered USB dongle with dual onboard PIFA antennas.

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

Ensure you have the required build tools, kernel headers, and Rust toolchain installed:

```bash
# Ubuntu / Debian
sudo apt update
sudo apt install -y build-essential linux-headers-$(uname -r) pkg-config libelf-dev

# Rust toolchain with no_std target
rustup target add x86_64-unknown-none
```

### Firmware Setup

Place the MT7603U firmware binary (`mt7603u_e2.bin`) into `/lib/firmware/mt7603u.bin`:

```bash
sudo cp harness/fixtures/mt7603u_e2.bin /lib/firmware/mt7603u.bin
```

### Building the Driver

#### Native Build (x86_64)

Compile both the Rust static library and the Linux kernel module:

```bash
make
```

#### Cross-Compiling for Embedded RISC-V (Milk-V Duo S / SG2000)

```bash
# 1. Install RISC-V soft-float no_std target matching kernel lp64 ABI
rustup target add riscv64imac-unknown-none-elf

# 2. Build Rust logic engine
cd src/rust && cargo build --release --target riscv64imac-unknown-none-elf && cd ../..

# 3. Link static object and strip debug/extended ISA attributes
rust-lld -flavor gnu -r --gc-sections --whole-archive \
    src/rust/target/riscv64imac-unknown-none-elf/release/libmt7603u_logic.a -o src/c/rust_logic.o
llvm-objcopy --strip-debug -R .riscv.attributes -R .llvmbc -R .llvmcmd src/c/rust_logic.o

# 4. Compile kernel module against Duo S Linux 5.10 build tree
make -C <DUO_SDK>/linux_5.10/build/sg2000_milkv_duos_musl_riscv64_emmc \
    M=$(pwd) ARCH=riscv CROSS_COMPILE=riscv64-unknown-linux-musl- modules
```

### Loading the Driver

```bash
# Load mac80211 subsystem dependency
sudo modprobe mac80211

# Insert mt7603u kernel module
sudo insmod mt7603u.ko
```

Verify device initialization via `dmesg`:

```bash
sudo dmesg | grep mt7603
```

Check the recognized network interface using `iw`:

```bash
iw dev
```

---

## 💡 Usage Examples

### 1. Connecting to Wi-Fi via NetworkManager (`nmcli`)

```bash
# List nearby Wi-Fi networks
nmcli device wifi list ifname <interface_name>

# Connect to an access point with WPA2-PSK
nmcli device wifi connect "<SSID>" password "<PASSWORD>" ifname <interface_name>
```

### 2. Creating a SoftAP Hotspot (`hostapd`)

Create a minimal `hostapd.conf`:

```ini
interface=wlxb0d59d8c8c7a
driver=nl80211
ssid=MT7603U-AP
hw_mode=g
channel=6
ieee80211n=1
ht_capab=[HT40+][SHORT-GI-20][SHORT-GI-40]
wpa=2
wpa_passphrase=mysecretpassword
wpa_key_mgmt=WPA-PSK
rsn_pairwise=CCMP
```

Launch hostapd:

```bash
sudo hostapd hostapd.conf
```

### 3. Monitor Mode & Packet Capture

```bash
# Bring interface down
sudo ip link set <interface_name> down

# Switch to monitor mode
sudo iw dev <interface_name> set type monitor

# Bring interface up
sudo ip link set <interface_name> up

# Set channel (e.g. channel 6)
sudo iw dev <interface_name> set channel 6

# Capture packets with full Radiotap headers
sudo tcpdump -i <interface_name> -y IEEE802_11_RADIO -s 0 -vvv
```

---

## 🧪 Testing & Validation

The codebase includes an extensive automated test suite covering unit tests, state machine transitions, and packet transcoding:

```bash
# Run full automated validation suite
./scripts/check.sh

# Run Rust unit tests only
cargo test --manifest-path src/rust/Cargo.toml
```

### Hardware Verification (Optional)

If a physical MT7603U adapter is connected:

```bash
# Test hardware channel scanning & RSSI readout
sudo ./scripts/test-hardware-scan.sh
```

---

## 📜 License

This project is licensed under the **GPLv2** (compatible with Linux kernel driver guidelines).
