# Bug RCA: RX 完全断流导致 BSS signal=0 (RMAC_CHFREQ 未写入)

- **Date:** 2026-08-16
- **Severity:** High（间歇性，物理拔插后有时可自愈）
- **Status:** Verified

## 1. Symptom & Impact
- TX 持续正常（mac80211 软扫描每 ~3s 发 probe request，USB ret=0 无错误）
- RX 完全无数据：EP 0x84 的 4 个 24KB URB 已提交但回调从不触发
- `iw dev wlx... scan dump` 所有 BSS `signal: 0.00 dBm`，cfg80211 收不到任何帧
- mac80211 debugfs statistics 只有 dot11ACK/RTS/FCS 计数器，无 `rx_packets`/`rx_bytes`
- **间歇性**：同 .ko、同物理拔插操作，一次正常（beacon 流 + RSSI 非零）一次完全断流

## 2. Root Cause Analysis (RCA)
1. **遗漏 vendor `AsicSwitchChannel` 的寄存器写入**：
   - vendor `mt7603_switch_channel` (chips/mt7603.c:152) 确实只发 MCU 命令，无寄存器写入；
   - 但其上层封装 `AsicSwitchChannel` (hw_ctrl/cmm_asic_mt.c:399-409) 在 `ChipSwitchChannel` 之后**无条件写 `RMAC_CHFREQ = 1`**：
     ```c
     RTMP_IO_READ32(pAd, RMAC_CHFREQ, &val);
     val = 1;
     RTMP_IO_WRITE32(pAd, RMAC_CHFREQ, val);
     ```
   - 我们只实现了 MCU 命令部分，漏掉这一层。Rust 侧 `build_channel_sequence` (mac.rs:189-204) 首 op 正是 `RMAC_CHFREQ=1`，但 FFI `mt7603_rust_get_channel_sequence` 在 C 侧**从未被调用**。
2. **`RMAC_CHFREQ` (WF_RMAC_BASE+0x090 = 0x00021890) 固件复位后不保证**：
   - 冷启动后该位可能残留 0 → RMAC RX 前端无信道频率 → 数据链路建立但 RX DMA 不工作；
   - 偶发为 1（正常实例）是硬件/eFuse 状态差异，不是驱动保证 → 解释"同 .ko 一次正常一次断流"。

## 3. Fix Summary
`src/c/mac80211.c:mt7603_set_channel` 在 MCU 命令下发后显式写 `RMAC_CHFREQ=1`：
```c
ret = mt7603_usb_write_reg(dev->udev, 0x00021890, 1);
if (ret)
    pr_warn("mt7603u: RMAC_CHFREQ write failed (%d)\n", ret);
```
调用链覆盖所有信道切换路径：`mac80211_start` (channel=1) 与 `mac80211_config` (扫描/信道调谐)。

## 4. Verification
- **物理拔插冷启动**：`UDMA_WLCFG_0 readback=0x00e41580` → `firmware is running` → RX callback status=0 大量触发 → `BEACON FRAME RECEIVED! ch=1 rssi=-72 dBm`
- **`iw scan dump` 6 个 BSS 全部 signal 非零**：-41 / -55 / -71 / -71 / -73 / -75 dBm（含 mt7601u 参照 AP fc:34:97:19:0e:01 @ -41 dBm）
- `./scripts/check.sh` 全绿（54/54 单测、0 spec drift）
- Spec 新增 SPEC-MAC-004 契约记录该行为

## 5. Related Facts (排查中确认)
- UDMA_WLCFG_0 (0x50029018) bit31=TxBusy（只读状态位）：冷启动=0，固件运行态=0x80e41580 为正常态，**非配置差异**
- `RT28XXDMAEnable` MT7603 分支（cmm_mac_usb.c:2285-2309）与我们的 UDMA 配置等价，非 RX 断流根因
- EFUSE_BUFFER_MODE 标定上传已在 start 中执行（缺失会导致 FCS 全错而非完全断流）
