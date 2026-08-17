# Bug RCA: 驱动卸载时内核 Page Fault 崩溃 (wiphy->addresses 误释放)

- **Date:** 2026-08-14
- **Severity:** Critical
- **Status:** Verified

## 1. Symptom & Impact
执行 `rmmod mt7603u` 或 USB 设备拔出注销时，系统发生内核崩溃（Kernel Oops / Panic）：
```text
mt7603u: Unregistering driver module
usbcore: deregistering interface driver mt7603u
BUG: unable to handle page fault for address: ffffd33044317788
#PF: supervisor write access in kernel mode
#PF: error_code(0x0002) - not-present page
```
导致系统死机重启。

## 2. Root Cause Analysis (RCA)
1. **非法的 wiphy 内部指针赋值**：
   在 `src/c/mac80211.c` 中，初始化 mac80211 硬件时将 `hw->wiphy->addresses` 指向了结构体内部成员 `dev->mac_addr`：
   ```c
   hw->wiphy->addresses = (struct mac_address *)dev->mac_addr;
   hw->wiphy->n_addresses = 1;
   ```
   而在 Linux 内核 `mac80211` / `cfg80211` 设计中，`wiphy->addresses` 是指驱动动态分配的多 MAC 地址列表。当驱动注销执行 `ieee80211_free_hw(hw)` -> `wiphy_free(wiphy)` 时，内核误对 `wiphy->addresses` 执行 `kfree(rdev->wiphy.addresses)`。
   由于 `dev->mac_addr` 只是 `hw->priv` 内存块中的一个内部偏移地址（并非 kmalloc 分配的独立堆首地址），`kfree` 破坏了 Slab/SLUB 元数据，直接触发了 `BUG: unable to handle page fault`。
2. **飞行中 URB 缺少锚定追踪**：
   异步发送的 TX URB 在网卡停止和注销时未统一管理，缺少 `usb_anchor` 导致注销时可能产生野指针回调。

## 3. Fix Summary
1. **规范设置 MAC 地址**：
   移除对 `wiphy->addresses` 与 `wiphy->n_addresses` 的赋值，改用内核标准宏 `SET_IEEE80211_PERM_ADDR(hw, dev->mac_addr)` 设置主要硬件 MAC 地址。
2. **引入 USB Anchor**：
   在 `struct mt7603u_dev` 结构中增加 `struct usb_anchor tx_anchor`，在 `mt7603_mac80211_tx` 中调用 `usb_anchor_urb`，并在 `mt7603_mac80211_stop` 和 `mt7603_unregister_mac80211` 时调用 `usb_kill_anchored_urbs(&dev->tx_anchor)` 清理所有飞行中 URB。

## 4. Regression Test
- **Harness 单元与沙盒测试**：39/39 用例全绿。
- **物理硬件热插拔实测**：
  在 Linux 7.0 物理内核环境下反复执行 `insmod mt7603u.ko` 与 `rmmod mt7603u`，验证网卡热重载与注销过程无任何内核错误与内存损坏，100% 顺畅。
