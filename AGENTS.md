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

开发与维护本驱动时，行为与寄存器事实以本仓库 `specs/` 契约文档（Single Source of Truth）为准。外部背景知识可参考：

### 6.1 学习笔记: `~/workspace/qtopie.github.io/posts/notes/linux/drivers/`
Linux 驱动设计理论与 MT76 系列拆解笔记，用于理解架构与术语：
- `index.md`: Linux 驱动总体设计哲学（VFS 抽象、机制/策略分离、vtable 模式、bus/device/driver 模型、USB 协议栈）。
- `mt7601u.md`: **MT7601U 深度拆解**（姊妹芯片，架构高度相似）——厂商原版 vs mac80211 主线重构（Kuba），覆盖 MCU/Andes、USB 端点布局、EEPROM/eFuse、RTMP 框架分层。
- `crescentrose-writing-drivers.md`: Rust + `rusb`(libusb) 编写用户态 USB 驱动的完整路径（可作为 Rust 侧实现模式参考）。

### 6.2 硬件与协议规范 (SSOT)
- **`specs/` 目录**：包含所有 MT7603U 寄存器映射表（`specs/schemas/registers.md`）、MCU 命令格式（`specs/modules/mcu.spec.md`）、802.11 状态机与收发契约（`specs/modules/`）。
- 硬件 I/O 在 `harness/mocks/` 中 Mock（如 `RegOps` / `UsbBus` trait），所有硬件访问序列严格依据 `specs/`。

## 7. Project Layout (工程目录结构)

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

