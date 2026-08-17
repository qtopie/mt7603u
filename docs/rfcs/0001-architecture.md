# RFC-0001: MT7603U C 骨架与 Rust 静态库混合架构设计

- **Status:** Approved
- **Author:** Antigravity Agent
- **Created Date:** 2026-08-13

## 1. Summary
本提案确定 MT7603U Linux 驱动的实现架构：采用 C 语言编写内核模块骨架（负责 USB probe/disconnect、mac80211 接入、URB 内存管理），并将复杂的帧解析、EEPROM 校验、寄存器配置序列与 MCU 命令生成下沉至纯 `no_std` Rust 静态库 (`libmt7603u_logic.a`) 中，通过 C ABI FFI 进行通信。

## 2. Motivation
1. **规避内核绑定成熟度风险**：内核态 `kernel` crate 目前对 `mac80211` 和底层 USB URB 的封装尚不完整，直接纯 Rust 编写内核模块会导致大量的 `unsafe` C 桥接或维护自定义内核 binding。
2. **最大化 Rust 安全收益**：网络驱动中的 CVE 大多数源于帧头部解析越界、EEPROM/位域字段转换错误及状态机混乱。将这些纯逻辑收拢到 Rust `no_std` 中可以获得 100% 内存安全保证。
3. **极佳的可测试性（Harness-Driven）**：Rust 业务逻辑不依赖任何 Linux 内核符号，可以用标准的 `cargo test` 在用户态进行完整的单元测试与 BDD 断言校验。

## 3. Detailed Design

### 3.1 架构分层图

```
┌─────────────────────────────────────────────────────────────┐
│                   Linux 内核 (mt7603u.ko)                   │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │                C 骨架层 (src/c/)                      │  │
│  │  • USB Device Driver (probe / disconnect)             │  │
│  │  • mac80211 Subsystem Glue (struct ieee80211_hw/ops)  │  │
│  │  • URB DMA Allocation & Submission                    │  │
│  │  • sk_buff Pool & Push/Pull Operations                │  │
│  └──────────────────────────┬────────────────────────────┘  │
│                             │ C ABI (no_std FFI)            │
│  ┌──────────────────────────▼────────────────────────────┐  │
│  │          Rust 静态逻辑库 (libmt7603u_logic.a)         │  │
│  │  • EEPROM Parser & Calibration Validator              │  │
│  │  • Register Sequence Generator (MAC/BBP/Channel)      │  │
│  │  • 802.11 / RxWI / TxWI Packet Transcoder             │  │
│  │  • MCU Command Packet Builder                         │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 FFI 交互模式（寄存器序列模式）

Rust 侧不直接写物理寄存器，而是将初始化逻辑表达为“寄存器写操作列表”。C 侧从 Rust 获取列表后在内核态执行实际的 USB Control Transfer。

```c
struct reg_write_op {
    uint32_t addr;
    uint32_t val;
};
```

此设计解耦了硬件 I/O 与逻辑状态机。

### 3.3 内存与生命周期规则
1. **Zero Ownership Transfer Across FFI**：C 侧分配和释放 `sk_buff` / `kmalloc` 内存，Rust 侧仅借用裸指针 `*const u8` / `*mut u8` 并立即转为安全的 slice `core::slice::from_raw_parts`。
2. **Panic Behavior**：Rust 侧设置 `panic = "abort"`，严格防止 panic 跨越 FFI 边界进入 C 内核空间。

## 4. Alternatives Considered
- **全 C 驱动（原厂驱动分支）**：代码体量庞大（~10万行），包含过时的 RTMP 协议栈与大量 C 内存缺陷。
- **全 Rust 内核模块（Rust for Linux）**：`mac80211` 和 USB URB 的内核 Binding 尚未稳定，需要极高维护成本。

## 5. Security & Performance Considerations
- **安全性**：解析越界与格式非法会在 Rust slice 校验时直接返回错误码（如 `-EINVAL`），保护内核不崩溃。
- **性能**：FFI 传参仅为指针与长度（零拷贝），开销可忽略不计；无堆分配(`alloc`)损耗。
