# AGENTS.md - System Operating Guidelines

Welcome Agent! You are a core collaborator in this repository. You MUST strictly adhere to these operational rules.

## 1. Context Loading & Memory Rule
- **Always Check `.agents/TASK.md` First:** Before taking any action, read `.agents/TASK.md` to restore context.
- **Maintain `.agents/TASK.md`:** Update `.agents/TASK.md` checklist items as you progress. If interrupted, write the current status under `Current Context`.

## 2. Spec-First Gate (Strict Enforcement)
- **SSOT (Single Source of Truth):** All behavioral contracts belong in `specs/`. Never implement feature logic without an approved Spec.
- **No Spec, No Code:** 
  1. Draft/Update files in `specs/` or `docs/rfcs/`.
  2. Wait for explicit user approval (`APPROVE`).
  3. Generate or update test harness fixtures (`harness/`) and test stubs (`testings/`).
  4. Only then implement business logic code.

## 3. Harness Engineering & Testing Gate
- **Harness-Driven Development:** Maintain fixtures, mocks, and runners in `harness/`.
- **Structured Diagnostic Feedback:** When tests fail, read the `Harness Failure Report` automatically written to `.agents/TASK.md` to perform targeted fixes instead of guessing logs.
- **No Shallow Tests:** Never write trivial Getter/Setter tests; tests must cover real boundary conditions and error scenarios.
- **Mock External Dependencies:** Always mock databases, network I/O, external RPCs, and hardware APIs in unit tests and harness mocks (`harness/mocks/`).

## 4. Grounding & Code Rules
- **Read Before Write:** Read target files and their dependencies before editing.
- **Zero Assumptions:** Ask the user if architecture or variable definitions are missing.
- **Minimal Diff:** Modify only what is required. Do not refactor unrelated code.

## 5. Execution & Safety Red Lines
- **Prohibited Commands:** Never run `git push --force`, `rm -rf /`, or alter external systems.
- **Mandatory Self-Validation:** Run `./scripts/check.sh` (or `./scripts/check-harness.sh`) before marking a task complete.
- **Error Limit:** If test/compile fixes fail > 3 times, stop and ask the user for guidance.

## 6. Reference Guidance (外部参考指引)

开发本驱动时，必须优先参考以下两个外部目录来获取设计决策与寄存器/芯片事实依据。**参考目录为只读资料，禁止修改其中任何文件。**

### 6.1 学习笔记: `~/workspace/qtopie.github.io/posts/notes/linux/drivers/`
Linux 驱动设计理论与 MT76 系列拆解笔记，用于理解架构与术语：
- `index.md`: Linux 驱动总体设计哲学（VFS 抽象、机制/策略分离、vtable 模式、bus/device/driver 模型、USB 协议栈）。
- `mt7601u.md`: **MT7601U 深度拆解**（姊妹芯片，架构高度相似）——厂商原版 vs mac80211 主线重构（Kuba），覆盖 MCU/Andes、USB 端点布局、EEPROM/eFuse、RTMP 框架分层。
- `crescentrose-writing-drivers.md`: Rust + `rusb`(libusb) 编写用户态 USB 驱动的完整路径（可作为 Rust 侧实现模式参考）。

### 6.2 原厂驱动源码: `~/workspace/buildroot_platform_hardware_wifi_mtk_drivers_mt7603/`
MTK 官方 `MT7603U` 驱动（v1.14，`JEDI.L0.MP1.mt7603u.v1.14`）。**项目巨大，只允许参考与 mt7603u 直接相关的部分，不得通读/复制无关芯片代码。** 关键文件映射：
mt7601u在/home/qtopierw/workspace/projects/mt7601u

| 关注点 | 参考文件 (相对该项目根目录) |
|---|---|
| 芯片寄存器初始化序列 | `chips/mt7603.c` (BBPInit / init_mac_cr / switch_channel / tx_pwr) |
| 芯片寄存器宏定义 | `include/chip/mt7603.h` |
| EEPROM 布局与标定 | `include/eeprom/mt7603_e2p.h` + `eeprom/MT7603*_EEPROM_layout_*.bin` |
| MCU 固件头 | `include/mcu/mt7603_firmware.h`、`mt7603_e2_firmware.h`、`mt7603_firmware/` |
| MAC 层 USB 寄存器 | `include/mac/mac_mt/mt_mac_usb.h` 及 `mac/mac_mt/` 相关定义 |
| USB 设备 ID 表 | `common/rtusb_dev_id.c` (USB ID `0x0E8D:0x7603`) |
| USB 探测/枚举 | `os/linux/usb_main_dev.c` (`rtusb_probe` / `USBDevConfigInit`) |
| USB 收发路径 | `os/linux/rt_usb.c`、`rt_usb_util.c`、`common/` 下 `rtusb_bulk*` |

**使用规则：**
1. 涉及寄存器地址、位域、时序、固件加载、EEPROM 字段时，**必须**回查该项目源码确认事实，禁止凭记忆写值。
2. 仅参考与 mt7603u 相关的代码路径（`mt7603*`、`mt_mac_usb`、`rt_usb*`、`rtusb_dev_id.c` 中 `#ifdef MT7603` 块）；`mt7601`/`mt7628`/`rt28xx` 等其他芯片文件仅作跨代对比，不作为事实依据。
3. 移植到 Rust 时，参考其**行为与寄存器值**，重新按 Rust/SDD 规范组织架构，禁止直接复制 C 代码结构。
4. 硬件 I/O 在 `harness/mocks/` 中 Mock（如 `RegOps` / `UsbBus` trait），原厂驱动的真实访问序列写入 `specs/` 作为契约来源。
