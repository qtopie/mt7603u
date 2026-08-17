# Module Spec: User-Space USB Hardware Runner

## 1. Overview
规范定义用户态 USB 硬件测试工具（`src/rust/src/bin/user_runner.rs`）。工具通过 `rusb` 直接对接物理设备 `003/036` (VID `0x0E8D`, PID `0x760C` / `0x7603`)，实现免重启下的真实固件下发、寄存器初始化与 Wi-Fi 信号扫描。

## 2. Interface / Execution Flow
1. **Device Discovery**: 识别并打开 `0x0e8d:0x760c` 或 `0x0e8d:0x7603` 硬件 USB 句柄。
2. **Firmware Verification & Injection**: 载入真实物理固件 `harness/fixtures/mt7603u_e2.bin`，调用 `verify_firmware` 校验，并下发给 Andes N9 MCU。
3. **MAC Initialization**: 调用 `get_mac_init_sequence` 并执行 Vendor Request 寄存器写操作。
4. **Channel Switch & Probe Request**: 下发信道切换及发送 802.11 Probe Request 探针，捕获目标热点 `firefly`！

## 3. Acceptance Criteria (BDD)

### Feature: User-space Physical Hardware Communication

#### Scenario 1: [SPEC-USR-001] Physical USB Device Enumeration
- **Given** 接入物理 MT7603U USB Wi-Fi 网卡
- **When** 运行 `cargo run --features user-runner --bin mt7603u-user-runner`
- **Then** 成功找到并打开 USB 接口 (VID: `0x0e8d`, PID: `0x760c` / `0x7603`)
- **Mapped Test:** `src/rust/src/bin/user_runner.rs`
