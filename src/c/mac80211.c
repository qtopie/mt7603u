#include <linux/module.h>
#include <linux/usb.h>
#include <linux/slab.h>
#include <linux/unaligned.h>
#include <linux/etherdevice.h>
#include <net/cfg80211.h>
#include <net/mac80211.h>
#include "mt7603u_rust.h"

static bool mt7603_dbg_rx;
module_param(mt7603_dbg_rx, bool, 0644);
MODULE_PARM_DESC(mt7603_dbg_rx, "Enable verbose per-frame RX logging (default off)");

#define MT7603_NUM_RX_URBS 4
#define MT7603_EEPROM_SIZE 1024
#define MT7603_RX_BUF_SIZE 24576
#define MT7603_DATA_BULK_IN 0x84
#define MT7603_CMD_RSP_BULK_IN 0x85
#define MT7603_DATA_BULK_OUT 0x05
#define MT7603_MGMT_BULK_OUT 0x08

/* WTBL (Wireless Table) register windows — ported from vendor
 * include/mac/mac_mt/wf_wtblon.h and mt_mac.c mt_wtbl_init. */
#define WTBL1_BASE       0x00028000 /* WTBL1 (per-station control), 32 B/entry */
#define WTBL1_ENTRY_SIZE 0x20
#define WTBL1OR          0x0002A300 /* WTBL1 flush control (PSM_W_FLAG bit 31) */
#define WTBL3_KEY_BASE   0x00042000 /* WTBL3 (per-station key SRAM), 64 B/entry */
#define WTBL3_ENTRY_SIZE 0x40

struct mt7603u_dev;

struct mt7603_rx_urb {
    struct urb *urb;
    void *buf;
    struct mt7603u_dev *dev;
};

struct mt7603u_dev {
    struct usb_device *udev;
    struct ieee80211_hw *hw;
    u8 mac_addr[ETH_ALEN];
    u8 current_channel;
    u8 cmd_seq;
    bool running;
    bool eeprom_valid;
    s8 rssi_offset_2g; /* signed EEPROM[0x46] calibration offset, clamped [-10,10] */
    u8 eeprom[MT7603_EEPROM_SIZE];
    struct mt7603_rx_urb rx_urbs[MT7603_NUM_RX_URBS];
    struct urb *cmd_urb;
    void *cmd_buf;
    struct usb_anchor tx_anchor;
};

static void mt7603_cmd_rsp_complete(struct urb *urb)
{
    struct mt7603u_dev *dev = urb->context;

    if (urb->status == 0 && urb->actual_length > 0) {
        pr_info_ratelimited("mt7603u: MCU event on EP 0x85 (len=%d, first=0x%02x)\n",
                            urb->actual_length, ((u8 *)urb->transfer_buffer)[0]);
    }

    if (dev->running && urb->status != -ENOENT && urb->status != -ESHUTDOWN && urb->status != -ECONNRESET) {
        usb_submit_urb(urb, GFP_ATOMIC);
    }
}

static void mt7603_rx_complete(struct urb *urb)
{
    struct mt7603_rx_urb *rx_entry = urb->context;
    struct mt7603u_dev *dev = rx_entry->dev;

    if (mt7603_dbg_rx)
        pr_info("mt7603u: RX callback fired: status=%d, actual_len=%d\n",
                urb->status, urb->actual_length);

    if (urb->status != 0 && urb->status != -ENOENT && urb->status != -ESHUTDOWN && urb->status != -ECONNRESET) {
        pr_warn("mt7603u: RX urb status error: %d\n", urb->status);
    }

    if (mt7603_dbg_rx && urb->status == 0 && urb->actual_length > 0) {
        pr_info("mt7603u: RX urb complete: len=%d, first=0x%02x\n",
                urb->actual_length, ((u8 *)urb->transfer_buffer)[0]);
    }

    if (urb->status == 0 && urb->actual_length >= 16) {
        size_t cur_pos = 0;

        while (cur_pos + 16 <= urb->actual_length) {
            u8 *frame_ptr = ((u8 *)urb->transfer_buffer) + cur_pos;
            size_t remaining = urb->actual_length - cur_pos;

            u16 rx_byte_cnt = frame_ptr[0] | (frame_ptr[1] << 8);
            if (rx_byte_cnt == 0 || rx_byte_cnt > remaining) {
                break;
            }

            /* Dispatch on pkt_type from DW0 bits [31:29] (LE: byte[3][7:5]).
             * 0x00=TXS, 0x01=TXRXV, 0x02=RX_NORMAL, 0x03=DUP_RFB, 0x07=EVENT.
             * Vendor: include/mac/mac_mt/mt_mac.h:524-532. */
            u8 pkt_type = (frame_ptr[3] >> 5) & 0x07;

            if (pkt_type == 0x00) {
                /* TXS (TX Status) report from LMAC — arrives on EP 0x84 alongside
                 * normal RX data. Format: RMAC_RXD_0_TXS (4 B DW0) followed by
                 * txs_cnt × 20-byte TXS_STRUC entries (DW0..DW4).
                 * Vendor ref: cmm_data.c:492-527, tx_rx/txs.c:108-112. */
                u8 txs_cnt = frame_ptr[2] & 0x1f; /* DW0 bits [20:16] = byte[2][4:0] */
                pr_info("mt7603u: TXS report: cnt=%u rx_byte_cnt=%u\n",
                        txs_cnt, rx_byte_cnt);
                if (txs_cnt > 0 &&
                        (size_t)rx_byte_cnt == (size_t)txs_cnt * 20 + 4 &&
                        cur_pos + (size_t)rx_byte_cnt <= urb->actual_length) {
                    int txs_i;
                    for (txs_i = 0; txs_i < (int)txs_cnt; txs_i++) {
                        u8 *e = frame_ptr + 4 + txs_i * 20;
                        /* TXS_D_0 (LE): [11:0]=tx_rate, [12]=fr, [13]=txsfm,
                         * [14]=txs2m, [15]=txs2h, [16]=ME, [17]=RE, [18]=LE,
                         * [19]=BE, [20]=txop, [21]=ps, [22]=baf, [25:23]=tid */
                        u32 txs_dw0 = e[0] | ((u32)e[1] << 8) | ((u32)e[2] << 16) | ((u32)e[3] << 24);
                        /* TXS_D_4 (LE): [11:0]=sn, [13:12]=tbw, [21:14]=pid,
                         * [22]=fm, [23]=am, [28:24]=mpdu_tx_cnt, [31:29]=last_tx_rate_idx */
                        u32 txs_dw4 = e[16] | ((u32)e[17] << 8) | ((u32)e[18] << 16) | ((u32)e[19] << 24);
                        u8 txs_pid = (u8)((txs_dw4 >> 14) & 0xff); /* TXS_D_4.pid */
                        u8 me      = (u8)((txs_dw0 >> 16) & 0x1);  /* TXS_D_0.ME: 0=ok, 1=err */
                        pr_info("mt7603u: TXS[%d]: pid=0x%02x ME=%u dw0=0x%08x dw4=0x%08x => %s\n",
                                txs_i, txs_pid, me, txs_dw0, txs_dw4,
                                me ? "LMAC_ERR/DROP" : "LMAC_TX_OK");
                    }
                } else {
                    pr_warn("mt7603u: TXS malformed: cnt=%u rx_byte_cnt=%u expected=%zu\n",
                            txs_cnt, rx_byte_cnt, (size_t)txs_cnt * 20 + 4);
                }
            } else {
                /* Normal / DUP-RFB / TXRXV / EVENT frames — parse via Rust and
                 * deliver to mac80211 as before. */
                struct mt7603_rx_info rx_info;
                int ret = mt7603_rust_parse_rx_frame(frame_ptr, remaining, dev->rssi_offset_2g, &rx_info);

                if (mt7603_dbg_rx)
                    pr_info_ratelimited("mt7603u: RX parse: ret=%d pkt_len=%d hdr_len=%d crc=%d pkt_type=0x%x byte_cnt=%d\n",
                                        ret, rx_info.pkt_len, rx_info.hdr_len,
                                        rx_info.is_crc_error, pkt_type, rx_byte_cnt);

                if (rx_info.pkt_len >= 2 && rx_info.pkt_len <= 60 && rx_info.is_crc_error == 0) {
                    size_t off = rx_info.hdr_len;
                    if (mt7603_dbg_rx)
                        pr_info("mt7603u: SHORTFRAME dump: pkt_len=%d fc=0x%04x addr1=%pM addr2=%pM addr3=%pM raw="
                            "%02x %02x %02x %02x %02x %02x %02x %02x %02x %02x %02x %02x %02x %02x %02x %02x\n",
                            rx_info.pkt_len,
                            (u16)(frame_ptr[off] | (frame_ptr[off+1] << 8)),
                            (off + 4 <= remaining) ? (frame_ptr + off + 4) : NULL,
                            (off + 10 <= remaining) ? (frame_ptr + off + 10) : NULL,
                            (off + 16 <= remaining) ? (frame_ptr + off + 16) : NULL,
                            frame_ptr[off], frame_ptr[off+1], frame_ptr[off+2], frame_ptr[off+3],
                            frame_ptr[off+4], frame_ptr[off+5], frame_ptr[off+6], frame_ptr[off+7],
                            frame_ptr[off+8], frame_ptr[off+9], frame_ptr[off+10], frame_ptr[off+11],
                            frame_ptr[off+12], frame_ptr[off+13], frame_ptr[off+14], frame_ptr[off+15]);
                }

                if (ret == 0 && rx_info.pkt_len > 4 && rx_info.is_crc_error == 0) {
                    size_t offset = rx_info.hdr_len;
                    /* NOTE: MT7603 rxd_0.rx_byte_cnt does NOT include the 4-byte
                     * hardware FCS (vendor mt_rx_info_2_blk sets MPDUtotalByteCnt =
                     * rx_byte_cnt - RMACInfoLen and passes it straight to cfg80211;
                     * the only FCS-subtract work-around in cmm_data.c is #if 0'd).
                     * Subtracting 4 here truncates every frame -- corrupt beacons
                     * and broken EAPOL -- so pass the frame through as-is. */
                    size_t payload_len = rx_info.pkt_len;
                    if (offset + payload_len <= remaining) {
                        u8 *hdr = frame_ptr + offset;
                        u16 fc = hdr[0] | (hdr[1] << 8);

                        if (mt7603_dbg_rx && (fc & 0x00FC) == 0x0080 && payload_len >= 24) {
                            pr_info("mt7603u: BEACON FRAME RECEIVED! BSSID=%pM, ch=%u, len=%zu, rssi=%d dBm\n",
                                    &hdr[16], dev->current_channel, payload_len, rx_info.rssi);
                        } else if (mt7603_dbg_rx && (fc & 0x000C) == 0x0008 && payload_len >= 24) {
                            /* Data frame: log type (0x2) / subtype and addrs */
                            pr_info("mt7603u: DATA FRAME RX! fc=0x%04x subtype=%d len=%zu da=%pM sa=%pM bssid=%pM\n",
                                    fc, (fc >> 4) & 0x0F, payload_len,
                                    &hdr[4], &hdr[10], &hdr[16]);
                        }

                        struct sk_buff *skb = dev_alloc_skb(payload_len + 2);
                        if (skb) {
                            skb_reserve(skb, 2);
                            skb_put_data(skb, hdr, payload_len);

                            struct ieee80211_rx_status *status = IEEE80211_SKB_RXCB(skb);
                            memset(status, 0, sizeof(*status));
                            status->band = NL80211_BAND_2GHZ;
                            status->freq = 2407 + (dev->current_channel ? dev->current_channel : 1) * 5;
                            status->chains = BIT(0);
                            if (rx_info.rssi != 0) {
                                status->signal = rx_info.rssi;
                                status->chain_signal[0] = rx_info.rssi;
                            } else {
                                status->flag |= RX_FLAG_NO_SIGNAL_VAL;
                            }

                            ieee80211_rx(dev->hw, skb);
                        }
                    }
                }
            } /* end pkt_type dispatch */

            size_t padding = (4 - (rx_byte_cnt % 4)) & 0x03;
            size_t subframe_len = rx_byte_cnt + padding + 4; // 4 bytes CSO at tail
            if (subframe_len < 16)
                break;
            cur_pos += subframe_len;
        }
    }

    if (dev->running && urb->status != -ENOENT && urb->status != -ESHUTDOWN && urb->status != -ECONNRESET) {
        int res = usb_submit_urb(urb, GFP_ATOMIC);
        if (res < 0) {
            pr_err_ratelimited("mt7603u: failed to resubmit RX URB: %d\n", res);
        }
    }
}

static int mt7603_start_rx(struct mt7603u_dev *dev)
{
    int i, ret;

    usb_clear_halt(dev->udev, usb_rcvbulkpipe(dev->udev, MT7603_DATA_BULK_IN));
    usb_clear_halt(dev->udev, usb_rcvbulkpipe(dev->udev, MT7603_CMD_RSP_BULK_IN));

    for (i = 0; i < MT7603_NUM_RX_URBS; i++) {
        struct mt7603_rx_urb *rx_entry = &dev->rx_urbs[i];

        if (!rx_entry->buf) {
            rx_entry->buf = kmalloc(MT7603_RX_BUF_SIZE, GFP_KERNEL);
            if (!rx_entry->buf)
                return -ENOMEM;
        }

        if (!rx_entry->urb) {
            rx_entry->urb = usb_alloc_urb(0, GFP_KERNEL);
            if (!rx_entry->urb)
                return -ENOMEM;
        }

        rx_entry->dev = dev;
        usb_fill_bulk_urb(rx_entry->urb, dev->udev,
                          usb_rcvbulkpipe(dev->udev, MT7603_DATA_BULK_IN),
                          rx_entry->buf, MT7603_RX_BUF_SIZE,
                          mt7603_rx_complete, rx_entry);

        ret = usb_submit_urb(rx_entry->urb, GFP_KERNEL);
        if (ret < 0) {
            pr_err("mt7603u: failed to submit RX URB %d: %d\n", i, ret);
        } else {
            pr_info("mt7603u: RX URB %d submitted on EP 0x84\n", i);
        }
    }

    if (!dev->cmd_buf) {
        dev->cmd_buf = kmalloc(512, GFP_KERNEL);
    }
    if (dev->cmd_buf && !dev->cmd_urb) {
        dev->cmd_urb = usb_alloc_urb(0, GFP_KERNEL);
        if (dev->cmd_urb) {
            usb_fill_bulk_urb(dev->cmd_urb, dev->udev,
                              usb_rcvbulkpipe(dev->udev, MT7603_CMD_RSP_BULK_IN),
                              dev->cmd_buf, 512,
                              mt7603_cmd_rsp_complete, dev);
            usb_submit_urb(dev->cmd_urb, GFP_KERNEL);
            pr_info("mt7603u: MCU cmd rsp URB submitted on EP 0x85\n");
        }
    }

    return 0;
}

static void mt7603_stop_rx(struct mt7603u_dev *dev)
{
    int i;

    if (dev->cmd_urb) {
        usb_kill_urb(dev->cmd_urb);
        usb_free_urb(dev->cmd_urb);
        dev->cmd_urb = NULL;
    }
    if (dev->cmd_buf) {
        kfree(dev->cmd_buf);
        dev->cmd_buf = NULL;
    }

    for (i = 0; i < MT7603_NUM_RX_URBS; i++) {
        struct mt7603_rx_urb *rx_entry = &dev->rx_urbs[i];
        if (rx_entry->urb) {
            usb_kill_urb(rx_entry->urb);
            usb_free_urb(rx_entry->urb);
            rx_entry->urb = NULL;
        }
        if (rx_entry->buf) {
            kfree(rx_entry->buf);
            rx_entry->buf = NULL;
        }
    }
}

static int mt7603_set_channel(struct mt7603u_dev *dev, u8 channel)
{
    u8 cmd_buf[128];
    size_t written = 0;
    int ret;
    u8 seq;

    if (channel < 1 || channel > 14)
        channel = 1;

    /* Wait briefly for any in-flight mgmt TX on EP 0x08 to drain */
    usb_wait_anchor_empty_timeout(&dev->tx_anchor, 50);

    /* Vendor `mt7603_switch_channel` (chips/mt7603.c:152) only issues MCU
     * commands — CmdChannelSwitch + CmdSetTxPowerCtrl — and writes no MAC
     * registers. Our earlier extra writes (ARB_RQCR/ARB_SCR) violated the
     * read-modify-write contract used by `AsicSetMacTxRx` (cmm_asic_mt.c)
     * and could stomp bits the firmware owns; drop them entirely. */
    seq = dev->cmd_seq = (dev->cmd_seq % 15) + 1;
    ret = mt7603_rust_build_chan_switch_cmd(channel, channel, 0, 2, 2, seq, cmd_buf, sizeof(cmd_buf), &written);
    if (ret == 0 && written > 0) {
        mt7603_usb_send_cmd(dev->udev, cmd_buf, written);
    }

    /* Vendor `AsicSwitchChannel` (hw_ctrl/cmm_asic_mt.c:399-409) wraps
     * ChipSwitchChannel and then writes RMAC_CHFREQ=1 on EVERY channel
     * switch. This bit (WF_RMAC_BASE+0x090) is NOT guaranteed by the
     * firmware after reset — when left 0 the RMAC RX frontend has no
     * channel frequency and no frames ever reach EP 0x84 (TX stays fine).
     * Root cause of the intermittent "TX ok, RX completely silent" state;
     * matches the leading op of `build_channel_sequence` (mac.rs:189-204). */
    ret = mt7603_usb_write_reg(dev->udev, 0x00021890, 1);
    if (ret)
        pr_warn("mt7603u: RMAC_CHFREQ write failed (%d)\n", ret);

    /* Vendor sends the TX power control command on every channel switch.
     * Fields are derived from the EEPROM image (eFuse path). */
    seq = dev->cmd_seq = (dev->cmd_seq % 15) + 1;
    ret = mt7603_rust_build_tx_power_ctrl_cmd(dev->eeprom, MT7603_EEPROM_SIZE, channel, seq,
                                             cmd_buf, sizeof(cmd_buf), &written);
    if (ret == 0 && written > 0) {
        mt7603_usb_send_cmd(dev->udev, cmd_buf, written);
    }

    dev->current_channel = channel;
    return 0;
}

static int mt7603_mac80211_start(struct ieee80211_hw *hw)
{
    struct mt7603u_dev *dev = hw->priv;
    struct reg_write_op ops[32];
    u8 *cmd_buf;
    size_t written = 0;
    int ret;
    u8 seq = dev->cmd_seq = (dev->cmd_seq % 15) + 1;

    pr_info("mt7603u: mac80211 start requested\n");
    dev->running = true;

    /* Vendor RT28XXDMAEnable enables USB RX bulk aggregation in normal mode.
     * Start RX ring (EP 0x84) and MCU cmd response ring (EP 0x85) FIRST so the
     * firmware can deliver event responses to EXT commands without stalling. */
    mt7603_usb_enable_udma(dev->udev, true);
    mt7603_start_rx(dev);

    cmd_buf = kzalloc(MT7603_EEPROM_SIZE + 64, GFP_KERNEL);
    if (!cmd_buf) {
        pr_err("mt7603u: failed to allocate cmd buffer\n");
        return -ENOMEM;
    }

    /* Push eFuse EEPROM calibration data to the firmware (EXT_CMD_EFUSE_BUFFER_MODE).
     * Without this, the BBP/RF frontend runs uncalibrated: PHY receives RF energy
     * (MIB MDRDY increments) but every frame fails FCS (RxMPDU stays 0). Must be
     * sent before any channel switch so the BBP uses calibrated RX gain. */
    if (dev->eeprom_valid) {
        if (mt7603_rust_build_efuse_buffer_mode_cmd(dev->eeprom, MT7603_EEPROM_SIZE, seq,
                                                    cmd_buf, MT7603_EEPROM_SIZE + 64, &written) == 0 &&
            written > 0) {
            ret = mt7603_usb_send_cmd(dev->udev, cmd_buf, written);
            pr_info("mt7603u: EFUSE_BUFFER_MODE cmd sent (%zu bytes, ret=%d)\n", written, ret);
        } else {
            pr_err("mt7603u: EFUSE_BUFFER_MODE build failed\n");
        }
        seq = dev->cmd_seq = (dev->cmd_seq % 15) + 1;
    } else {
        pr_warn("mt7603u: eFuse EEPROM not available, skipping calibration upload\n");
    }

    ret = mt7603_rust_get_mac_init_sequence(ops, 32, &written);
    if (ret == 0 && written > 0) {
        mt7603_execute_reg_ops(dev->udev, ops, written);
        pr_info("mt7603u: MAC init sequence applied (%zu ops)\n", written);
    }

    ret = mt7603_rust_build_own_mac_sequence(dev->mac_addr, ops, 32, &written);
    if (ret == 0 && written > 0) {
        mt7603_execute_reg_ops(dev->udev, ops, written);
        pr_info("mt7603u: Own MAC sequence applied (%pM)\n", dev->mac_addr);
    }

    {
        static const u8 bcast_addr[6] = {0xff, 0xff, 0xff, 0xff, 0xff, 0xff};
        ret = mt7603_rust_build_wtbl_sta_sequence(bcast_addr, ops, 32, &written);
        if (ret == 0 && written > 0) {
            mt7603_execute_reg_ops(dev->udev, ops, written);
            pr_info("mt7603u: WTBL1 default sequence applied (%zu ops)\n", written);
        }
    }

    // Power on RF radio
    if (mt7603_rust_build_radio_on_off_cmd(true, seq, cmd_buf, MT7603_EEPROM_SIZE + 64, &written) == 0 && written > 0) {
        mt7603_usb_send_cmd(dev->udev, cmd_buf, written);
    }

    /* Program EDCA/WMM per-AC contention parameters for all four access
     * categories. Our MAC init only enables the TX queues (ARB_TQCR0) but
     * never fills the EDCA table; without it the LMAC TX scheduler has no
     * AIFS/CW/TxOP for the data AC (EP 0x05 / AC0) and silently drops data
     * frames. mgmt uses a separate always-on queue, which is why scans/assoc
     * work but the 4-way handshake M2 never reaches the air. Mirrors vendor
     * AsicSetAllWmmParam / CmdEdcaParameterSet (EXT_CMD_ID_EDCA_SET=0x27). */
    seq = dev->cmd_seq = (dev->cmd_seq % 15) + 1;
    if (mt7603_rust_build_edca_set_cmd(seq, cmd_buf, MT7603_EEPROM_SIZE + 64, &written) == 0 && written > 0) {
        ret = mt7603_usb_send_cmd(dev->udev, cmd_buf, written);
        pr_info("mt7603u: EDCA_SET cmd sent (%zu bytes, ret=%d)\n", written, ret);
    } else {
        pr_err("mt7603u: EDCA_SET build failed\n");
    }

    kfree(cmd_buf);
    mt7603_set_channel(dev, 1);
    return 0;
}

static void mt7603_mac80211_stop(struct ieee80211_hw *hw, bool suspend)
{
    struct mt7603u_dev *dev = hw->priv;

    pr_info("mt7603u: mac80211 stop requested (suspend=%d)\n", suspend);
    dev->running = false;
    mt7603_stop_rx(dev);
    usb_kill_anchored_urbs(&dev->tx_anchor);

    /*
     * Do NOT send radio-off (EXT_CMD_RADIO_ON_OFF_CTRL) to the firmware here.
     * MT7603 firmware stays in RAM across rmmod/insmod; a radio-off command
     * wedges its MCU command interface (restart-dl / EP 0x84 / EP 0x85 stop
     * responding, -110 on the next probe). The vendor disables radio-off on
     * unload for MT7603 (AsicRadioOff = NULL, CmdRadioOnOffCtrl commented out
     * in rtmp_init_inf.c:1279 / usb_main_dev.c:622). MAC/DMA-level shutdown
     * above (stop_rx + kill TX URBs) is sufficient, matching the vendor.
     * See specs/modules/mac.spec.md SPEC-MAC-005.
     */
}

static int mt7603_mac80211_add_interface(struct ieee80211_hw *hw, struct ieee80211_vif *vif)
{
    struct mt7603u_dev *dev = hw->priv;
    struct reg_write_op ops[32];
    size_t written = 0;
    pr_info("mt7603u: add_interface requested (type=%d addr=%pM)\n", vif->type, vif->addr);

    if (vif && !is_zero_ether_addr(vif->addr)) {
        if (mt7603_rust_build_own_mac_sequence(vif->addr, ops, 32, &written) == 0 && written > 0) {
            mt7603_execute_reg_ops(dev->udev, ops, written);
            pr_info("mt7603u: Own MAC updated for vif (%pM)\n", vif->addr);
        }
    }
    return 0;
}

static void mt7603_mac80211_remove_interface(struct ieee80211_hw *hw, struct ieee80211_vif *vif)
{
    pr_info("mt7603u: remove_interface requested (type=%d)\n", vif->type);
}

static int mt7603_mac80211_config(struct ieee80211_hw *hw, int radio_idx, u32 changed)
{
    struct mt7603u_dev *dev = hw->priv;

    if (hw->conf.chandef.chan) {
        u8 ch = hw->conf.chandef.chan->hw_value;
        if ((changed & IEEE80211_CONF_CHANGE_CHANNEL) || ch != dev->current_channel) {
            pr_info("mt7603u: config() chan=%d freq=%d changed=0x%x\n",
                    ch, hw->conf.chandef.chan->center_freq, changed);
            mt7603_set_channel(dev, ch);
        }
    } else {
        pr_info("mt7603u: config() no chan\n");
    }
    return 0;
}

static void mt7603_mac80211_configure_filter(struct ieee80211_hw *hw,
                                              unsigned int changed_flags,
                                              unsigned int *total_flags,
                                              u64 flags)
{
    *total_flags &= FIF_ALLMULTI |
                    FIF_BCN_PRBRESP_PROMISC |
                    FIF_CONTROL |
                    FIF_OTHER_BSS |
                    FIF_FCSFAIL |
                    FIF_PLCPFAIL |
                    FIF_PSPOLL;
}

static void mt7603_mac80211_bss_info_changed(struct ieee80211_hw *hw, struct ieee80211_vif *vif, struct ieee80211_bss_conf *info, u64 changed)
{
    struct mt7603u_dev *dev = hw->priv;
    struct reg_write_op ops[32];
    size_t written = 0;
    int ret;
    const u8 *bssid = info ? info->bssid : NULL;

    pr_info("mt7603u: bss_info_changed: changed=0x%llx bssid=%pM assoc=%d\n",
            changed, bssid, vif ? vif->cfg.assoc : -1);

    if (changed & BSS_CHANGED_BEACON) {
        pr_info("mt7603u: AP beacon state changed (enabled=%d)\n", info ? info->enable_beacon : 0);
    }
    if (changed & (BSS_CHANGED_BSSID | BSS_CHANGED_ASSOC)) {
        pr_info("mt7603u: BSSID/ASSOC changed to %pM (assoc=%d)\n", bssid, vif ? vif->cfg.assoc : -1);
        if (bssid && !is_zero_ether_addr(bssid)) {
            struct cfg80211_bss *bss_entry = cfg80211_get_bss(hw->wiphy, NULL, bssid, NULL, 0,
                                                               IEEE80211_BSS_TYPE_ANY, IEEE80211_PRIVACY_ANY);
            if (bss_entry) {
                if (bss_entry->channel && bss_entry->channel->hw_value != dev->current_channel) {
                    pr_info("mt7603u: BSS scan cache hit for %pM: tuning to channel %d (%d MHz)\n",
                            bssid, bss_entry->channel->hw_value, bss_entry->channel->center_freq);
                    mt7603_set_channel(dev, bss_entry->channel->hw_value);
                }
                cfg80211_put_bss(hw->wiphy, bss_entry);
            } else if (info && info->bss && info->bss->channel) {
                u8 ch = info->bss->channel->hw_value;
                if (ch != dev->current_channel) {
                    pr_info("mt7603u: bss_info_changed: tuning to channel %d (%d MHz)\n",
                            ch, info->bss->channel->center_freq);
                    mt7603_set_channel(dev, ch);
                }
            } else if (vif && vif->bss_conf.bss && vif->bss_conf.bss->channel) {
                u8 ch = vif->bss_conf.bss->channel->hw_value;
                if (ch != dev->current_channel) {
                    pr_info("mt7603u: bss_info_changed: tuning to vif channel %d (%d MHz)\n",
                            ch, vif->bss_conf.bss->channel->center_freq);
                    mt7603_set_channel(dev, ch);
                }
            }

            u32 lo = (u32)bssid[0] | ((u32)bssid[1] << 8) |
                     ((u32)bssid[2] << 16) | ((u32)bssid[3] << 24);
            u32 hi = (u32)bssid[4] | ((u32)bssid[5] << 8) | BIT(16);
            int r;
            r = mt7603_usb_write_reg(dev->udev, 0x00021804, lo);
            if (r == 0)
                r = mt7603_usb_write_reg(dev->udev, 0x00021808, hi);
            if (r)
                pr_warn("mt7603u: RMAC_CB0R write failed (%d)\n", r);
            else
                pr_info("mt7603u: Current BSSID programmed (CB0R0=0x%08x CB0R1=0x%08x)\n", lo, hi);

            /* Clear MBSSID mask in RMAC_RMACDR (0x00021878) to ensure Single BSSID mode */
            u32 rmacdr = 0;
            if (mt7603_usb_read_reg(dev->udev, 0x00021878, &rmacdr) == 0) {
                rmacdr &= ~(0x3 << 24); /* Clear RMACDR_MBSSID_MASK */
                mt7603_usb_write_reg(dev->udev, 0x00021878, rmacdr);
                pr_info("mt7603u: RMAC_RMACDR configured for Single BSSID mode (0x%08x)\n", rmacdr);
            }

            ret = mt7603_rust_build_wtbl_sta_sequence(bssid, ops, 32, &written);
            if (ret == 0 && written > 0) {
                mt7603_execute_reg_ops(dev->udev, ops, written);
                pr_info("mt7603u: WTBL1 STA sequence applied for %pM (%zu ops)\n", bssid, written);
            }
        } else {
            mt7603_usb_write_reg(dev->udev, 0x00021804, 0);
            mt7603_usb_write_reg(dev->udev, 0x00021808, 0);
            mt7603_usb_write_reg(dev->udev, 0x00028014, 0);
            mt7603_usb_write_reg(dev->udev, 0x00028018, 0);
            mt7603_usb_write_reg(dev->udev, 0x0002801C, 0);
            pr_info("mt7603u: Current BSSID & WTBL1 Entry 1 cleared\n");
        }
    }
}



static void mt7603_tx_complete(struct urb *urb)
{
    struct sk_buff *skb = urb->context;
    if (urb->status != 0) {
        pr_info_ratelimited("mt7603u: TX URB failed: status=%d actual=%d len=%d\n",
                            urb->status, urb->actual_length, skb ? skb->len : -1);
    }
    dev_kfree_skb_any(skb);
    usb_free_urb(urb);
}

static void mt7603_mac80211_tx(struct ieee80211_hw *hw, struct ieee80211_tx_control *control, struct sk_buff *skb)
{
    struct mt7603u_dev *dev = hw->priv;
    struct ieee80211_hdr *hdr;
    struct mt7603_tx_params params;
    u8 txwi[32];
    u16 fc;
    bool is_mgmt;
    u8 ep;
    int pad_len = 0;
    int ret;

    if (!dev->running) {
        dev_kfree_skb_any(skb);
        return;
    }

    fc = get_unaligned_le16(skb->data);
    hdr = (struct ieee80211_hdr *)skb->data;
    is_mgmt = ieee80211_is_mgmt(fc);
    memset(&params, 0, sizeof(params));
    params.hdr_len = ieee80211_hdrlen(fc);
    params.frm_type = (fc >> 2) & 0x3;
    params.sub_type = (fc >> 4) & 0xf;
    params.is_bm = hdr->addr1[0] & 0x01;
    /* Probe requests (broadcast) expect no ACK; unicast data expects one. */
    params.no_ack = params.is_bm;
    /* mgmt frames route through LMAC MGMT queue (Q_IDX_AC4), data through AC0. */
    params.queue = is_mgmt ? 0x04 : 0x00;
    params.pid = params.is_bm ? 0 : 1; /* WCID 1 for associated AP, 0 for broadcast */
    params.rate_idx = 0;
    params.pkt_len = skb->len;

    /* mac80211 offloads 802.11 encryption to this driver once set_key succeeds;
     * it selects the offloaded key and stores it in info->control.hw_key, and
     * expects the HW to CCMP-encrypt. Mirror that into the TXWI protect_frm bit
     * so the MAC applies the WTBL3 key for this WCID. */
    {
        struct ieee80211_tx_info *info = IEEE80211_SKB_CB(skb);
        params.protect_frm = (info->control.hw_key != NULL) ? 1 : 0;
    }

    /* Option #2 diagnostic: dump raw skb bytes at TX entry for data frames —
     * BEFORE any hdr_pad insertion or TXD prepend — to verify that mac80211
     * hands us the correct EAPOL/data content. Compare with the URB-submit
     * dump to detect any corruption introduced by the driver's own code path. */
    if (params.frm_type == 2) {
        const u8 *_dp = skb->data;
        int _dlen = (int)skb->len < 80 ? (int)skb->len : 80;
        int _di;
        pr_info("mt7603u: DATA TX entry: fc=0x%04x hdr_len=%u skb_len=%u protect_frm=%d\n",
                fc, params.hdr_len, skb->len, params.protect_frm);
        for (_di = 0; _di + 8 <= _dlen; _di += 8)
            pr_info("mt7603u: TX-ENTRY+%02d: %02x %02x %02x %02x  %02x %02x %02x %02x\n",
                    _di, _dp[_di], _dp[_di+1], _dp[_di+2], _dp[_di+3],
                    _dp[_di+4], _dp[_di+5], _dp[_di+6], _dp[_di+7]);
    }

    /* Vendor MlmeTransmit rate selection (cmm_data.c:1666-1683):
     * 2.4G (ch<=14) -> CCK 1M LONG_PREAMBLE, 5G -> OFDM 6M. */
    if (dev->current_channel > 14) {
        params.rate_mode = 1; /* MODE_OFDM */
        params.rate_mcs = 0;  /* 6M */
        params.preamble = 1;  /* LONG_PREAMBLE */
    } else {
        params.rate_mode = 0; /* MODE_CCK */
        params.rate_mcs = 0;  /* 1M */
        params.preamble = 1;  /* LONG_PREAMBLE */
    }
    params.bw = 0; /* BW_20 */

    ret = mt7603_rust_build_txwi(&params, txwi, sizeof(txwi));
    if (ret != 0) {
        pr_warn_ratelimited("mt7603u: build_txwi failed (%d)\n", ret);
        dev_kfree_skb_any(skb);
        return;
    }

    /* Option #1 diagnostic: for data frames request LMAC TX-status feedback.
     * DW5[10]=tx_status_2_host causes LMAC to send a TXS report (pkt_type=0x00)
     * on EP 0x84 after the TX attempt. DW5[7:0]=pid=0x42 is a sentinel so we
     * can identify our data-frame TXS reports in mt7603_rx_complete.
     * ME=0 in the TXS → frame reached air; ME=1 → LMAC dropped / errored.
     * Vendor ref: mt_mac.h TXS_D_4.pid[21:14], tx_rx/txs.c DataTxSHandler. */
    if (params.frm_type == 2) {
        txwi[20] = 0x42;        /* DW5[7:0]  = pid sentinel 0x42 */
        txwi[21] |= (1u << 2);  /* DW5[10]   = tx_status_2_host  */
    }

    /* MT7603 DMA requires 4-byte alignment for payload following 802.11 header.
     * When hdr_len % 4 != 0 (e.g. 26-byte QoS data), insert padding bytes
     * between the header and payload so the LMAC TX DMA can fetch the LLC
     * header on a 4-byte boundary. */
    int hdr_pad_len = (4 - (params.hdr_len & 0x03)) & 0x03;
    if (hdr_pad_len > 0) {
        if (skb_tailroom(skb) < hdr_pad_len) {
            struct sk_buff *nskb = skb_copy_expand(skb, skb_headroom(skb), hdr_pad_len, GFP_ATOMIC);
            dev_kfree_skb_any(skb);
            if (!nskb)
                return;
            skb = nskb;
        }
        skb_put(skb, hdr_pad_len);
        memmove(skb->data + params.hdr_len + hdr_pad_len,
                skb->data + params.hdr_len,
                skb->len - params.hdr_len - hdr_pad_len);
        memset(skb->data + params.hdr_len, 0, hdr_pad_len);
    }

    /* Make room for the 32-byte TMAC_TXD_L in front of the 802.11 frame.
     * hw->extra_tx_headroom = 32 normally guarantees this, but keep the
     * defensive realloc for any frame that bypassed the normal tx path. */
    if (skb_headroom(skb) < 32) {
        struct sk_buff *nskb = skb_realloc_headroom(skb, 32);
        dev_kfree_skb_any(skb);
        if (!nskb)
            return;
        skb = nskb;
    }

    skb_push(skb, 32);
    memcpy(skb->data, txwi, 32);

    ep = is_mgmt ? MT7603_MGMT_BULK_OUT : MT7603_DATA_BULK_OUT;
    pad_len = ((skb->len + 3) & ~3) + 4 - skb->len;
    if (skb_tailroom(skb) < pad_len) {
        struct sk_buff *nskb = skb_copy_expand(skb, skb_headroom(skb), pad_len, GFP_ATOMIC);
        dev_kfree_skb_any(skb);
        if (!nskb)
            return;
        skb = nskb;
    }
    memset(skb_put(skb, pad_len), 0, pad_len);

    struct urb *urb = usb_alloc_urb(0, GFP_ATOMIC);
    if (!urb) {
        dev_kfree_skb_any(skb);
        return;
    }
    usb_fill_bulk_urb(urb, dev->udev,
                      usb_sndbulkpipe(dev->udev, ep),
                      skb->data, skb->len,
                      mt7603_tx_complete, skb);
    usb_anchor_urb(urb, &dev->tx_anchor);
    ret = usb_submit_urb(urb, GFP_ATOMIC);
    if (ret < 0) {
        usb_unanchor_urb(urb);
        usb_free_urb(urb);
        dev_kfree_skb_any(skb);
    } else if (params.frm_type == 2) {
        pr_info("mt7603u: DATA TX submitted: type=%u subtype=%u ep=0x%02x len=%d pid=%u queue=%u\n",
                params.frm_type, params.sub_type, ep, skb->len, params.pid, params.queue);
    } else {
        pr_info_ratelimited("mt7603u: TX submitted: type=%u subtype=%u ep=0x%02x len=%d\n",
                            params.frm_type, params.sub_type, ep, skb->len);
    }
}

static int mt7603_mac80211_sta_add(struct ieee80211_hw *hw, struct ieee80211_vif *vif, struct ieee80211_sta *sta)
{
    struct mt7603u_dev *dev = hw->priv;
    struct reg_write_op ops[32];
    size_t written = 0;
    int ret;

    pr_info("mt7603u: sta_add: addr=%pM aid=%d vif_type=%d\n", sta->addr, sta->aid, vif ? vif->type : -1);

    if (sta && !is_zero_ether_addr(sta->addr)) {
        struct cfg80211_bss *bss_entry = cfg80211_get_bss(hw->wiphy, NULL, sta->addr, NULL, 0,
                                                           IEEE80211_BSS_TYPE_ANY, IEEE80211_PRIVACY_ANY);
        if (bss_entry) {
            if (bss_entry->channel && bss_entry->channel->hw_value != dev->current_channel) {
                pr_info("mt7603u: sta_add: BSS scan cache hit for %pM: tuning to channel %d (%d MHz)\n",
                        sta->addr, bss_entry->channel->hw_value, bss_entry->channel->center_freq);
                mt7603_set_channel(dev, bss_entry->channel->hw_value);
            }
            cfg80211_put_bss(hw->wiphy, bss_entry);
        } else if (vif && vif->bss_conf.bss && vif->bss_conf.bss->channel) {
            u8 ch = vif->bss_conf.bss->channel->hw_value;
            if (ch != dev->current_channel) {
                pr_info("mt7603u: sta_add: tuning to channel %d (%d MHz)\n",
                        ch, vif->bss_conf.bss->channel->center_freq);
                mt7603_set_channel(dev, ch);
            }
        }
    }

    if (sta && !is_zero_ether_addr(sta->addr)) {
        u32 lo = (u32)sta->addr[0] | ((u32)sta->addr[1] << 8) |
                 ((u32)sta->addr[2] << 16) | ((u32)sta->addr[3] << 24);
        u32 hi = (u32)sta->addr[4] | ((u32)sta->addr[5] << 8) | BIT(16);
        mt7603_usb_write_reg(dev->udev, 0x00021804, lo);
        mt7603_usb_write_reg(dev->udev, 0x00021808, hi);

        ret = mt7603_rust_build_wtbl_sta_sequence(sta->addr, ops, 32, &written);
        if (ret == 0 && written > 0) {
            mt7603_execute_reg_ops(dev->udev, ops, written);
            pr_info("mt7603u: WTBL1 STA sequence applied in sta_add for %pM (%zu ops)\n", sta->addr, written);
        }
    }
    return 0;
}

static int mt7603_mac80211_sta_remove(struct ieee80211_hw *hw, struct ieee80211_vif *vif, struct ieee80211_sta *sta)
{
    struct mt7603u_dev *dev = hw->priv;
    pr_info("mt7603u: sta_remove: addr=%pM\n", sta ? sta->addr : NULL);
    mt7603_usb_write_reg(dev->udev, 0x00021804, 0);
    mt7603_usb_write_reg(dev->udev, 0x00021808, 0);
    mt7603_usb_write_reg(dev->udev, 0x00028014, 0);
    mt7603_usb_write_reg(dev->udev, 0x00028018, 0);
    mt7603_usb_write_reg(dev->udev, 0x0002801C, 0);
    return 0;
}

static void mt7603_mac80211_sta_notify(struct ieee80211_hw *hw, struct ieee80211_vif *vif, enum sta_notify_cmd cmd, struct ieee80211_sta *sta)
{
}



static int mt7603_mac80211_set_key(struct ieee80211_hw *hw, enum set_key_cmd cmd, struct ieee80211_vif *vif, struct ieee80211_sta *sta, struct ieee80211_key_conf *key)
{
    struct mt7603u_dev *dev = hw->priv;
    int wcid;
    u32 wtbl1_base, key_base, dw;
    int i, ret = 0;

    pr_info("mt7603u: set_key requested (cmd=%d, cipher=0x%x, key_idx=%d, flags=0x%x, len=%d)\n",
            cmd, key->cipher, key->keyidx, key->flags, key->keylen);

    if (cmd != SET_KEY) {
        /* Key removal: leave the entry; a follow-up SET_KEY re-programs it.
         * TODO: clear adm/cipher_suit/key bytes for a clean removal. */
        return 0;
    }

    /* Only CCMP (WPA2) is offloaded to hardware today. For any other cipher
     * we decline so mac80211 falls back to software crypto rather than sending
     * a frame in clear. */
    if (key->cipher != WLAN_CIPHER_SUITE_CCMP) {
        pr_warn("mt7603u: set_key: cipher 0x%x not offloaded to HW (CCMP only)\n",
                key->cipher);
        return -EOPNOTSUPP;
    }

    /* Pairwise key (PTK) is shared with the AP -> WCID 1 (the AP entry used
     * for both TX to and RX from the AP). Group key (GTK) -> broadcast WCID 0. */
    if (key->flags & IEEE80211_KEY_FLAG_PAIRWISE)
        wcid = 1;
    else if (sta && !is_zero_ether_addr(sta->addr))
        wcid = 1;
    else
        wcid = 0;

    /* Write the 16-byte TK into the WTBL3 key SRAM (host remap window).
     * Vendor CmdProcAddRemoveKey (cmm_asic_mt.c:3471) writes the CCMP TK as
     * four little-endian 32-bit words at wtbl_base_addr[2] + wcid*64. */
    key_base = WTBL3_KEY_BASE + (u32)wcid * WTBL3_ENTRY_SIZE;
    for (i = 0; i < 4; i++) {
        u32 w = (u32)key->key[i * 4]
              | ((u32)key->key[i * 4 + 1] << 8)
              | ((u32)key->key[i * 4 + 2] << 16)
              | ((u32)key->key[i * 4 + 3] << 24);
        mt7603_usb_write_reg(dev->udev, key_base + i * 4, w);
    }

    /* WTBL1 DW0: mark RX valid (rv), RX key valid (rkv), key index (kid),
     * and the rate/BA control bits the vendor sets for a keyed entry. */
    wtbl1_base = WTBL1_BASE + (u32)wcid * WTBL1_ENTRY_SIZE;
    if (mt7603_usb_read_reg(dev->udev, wtbl1_base, &dw) == 0) {
        dw |= (1u << 28) /* rv */
            | (1u << 26)  /* rkv */
            | (1u << 22)  /* rc_a1 */
            | (1u << 25)  /* rc_id */
            | ((u32)(key->keyidx & 0x3) << 23); /* kid */
        mt7603_usb_write_reg(dev->udev, wtbl1_base, dw);
    }

    /* WTBL1 DW2: set cipher_suit = CCMP_128 (4) and adm = 1 so the MAC applies
     * the WTBL3 key. Preserve qos/ht/baf_en/dyn_bw already programmed at init. */
    if (mt7603_usb_read_reg(dev->udev, wtbl1_base + 0x08, &dw) == 0) {
        dw &= ~0x78u;          /* clear cipher_suit[6:3] */
        dw |= (4u << 3);       /* CCMP_128 */
        dw |= (1u << 30);      /* adm */
        mt7603_usb_write_reg(dev->udev, wtbl1_base + 0x08, dw);
    }

    /* Flush the WTBL1 table so the new entry takes effect (PSM_W_FLAG). */
    mt7603_usb_write_reg(dev->udev, WTBL1OR, 0x80000000);
    mt7603_usb_write_reg(dev->udev, WTBL1OR, 0x00000000);

    /* Tell mac80211 which hardware slot (WCID) holds this key so it offloads
     * encryption and sets IEEE80211_TX_CTL_PROTECTED on outgoing frames. */
    key->hw_key_idx = wcid;

    pr_info("mt7603u: set_key: PTK/GTK installed (wcid=%d keyidx=%d cipher=CCMP)\n",
            wcid, key->keyidx);
    return ret;
}

static int mt7603_mac80211_conf_tx(struct ieee80211_hw *hw, struct ieee80211_vif *vif, unsigned int link_id, u16 ac, const struct ieee80211_tx_queue_params *params)
{
    return 0;
}

static void mt7603_mac80211_sw_scan(struct ieee80211_hw *hw, struct ieee80211_vif *vif, const u8 *mac_addr)
{
}

static void mt7603_mac80211_sw_scan_complete(struct ieee80211_hw *hw, struct ieee80211_vif *vif)
{
}

static int mt7603_mac80211_ampdu_action(struct ieee80211_hw *hw, struct ieee80211_vif *vif, struct ieee80211_ampdu_params *params)
{
    return 0;
}

static void mt7603_mac80211_sta_rate_tbl_update(struct ieee80211_hw *hw, struct ieee80211_vif *vif, struct ieee80211_sta *sta)
{
}

const struct ieee80211_ops mt7603_mac80211_ops = {
    .tx                 = mt7603_mac80211_tx,
    .start              = mt7603_mac80211_start,
    .stop               = mt7603_mac80211_stop,
    .add_interface      = mt7603_mac80211_add_interface,
    .remove_interface   = mt7603_mac80211_remove_interface,
    .config             = mt7603_mac80211_config,
    .configure_filter   = mt7603_mac80211_configure_filter,
    .bss_info_changed   = mt7603_mac80211_bss_info_changed,
    .sta_add            = mt7603_mac80211_sta_add,
    .sta_remove         = mt7603_mac80211_sta_remove,
    .sta_notify         = mt7603_mac80211_sta_notify,
    .set_key            = mt7603_mac80211_set_key,
    .conf_tx            = mt7603_mac80211_conf_tx,
    .sw_scan_start      = mt7603_mac80211_sw_scan,
    .sw_scan_complete   = mt7603_mac80211_sw_scan_complete,
    .ampdu_action       = mt7603_mac80211_ampdu_action,
    .sta_rate_tbl_update = mt7603_mac80211_sta_rate_tbl_update,
    .wake_tx_queue      = ieee80211_handle_wake_tx_queue,
    /* Required chanctx emulation for single-channel devices */
    .add_chanctx        = ieee80211_emulate_add_chanctx,
    .remove_chanctx     = ieee80211_emulate_remove_chanctx,
    .change_chanctx     = ieee80211_emulate_change_chanctx,
};

static struct ieee80211_channel mt7603_channels_2ghz[] = {
    { .center_freq = 2412, .hw_value = 1, .band = NL80211_BAND_2GHZ, .flags = 0 },
    { .center_freq = 2417, .hw_value = 2, .band = NL80211_BAND_2GHZ, .flags = 0 },
    { .center_freq = 2422, .hw_value = 3, .band = NL80211_BAND_2GHZ, .flags = 0 },
    { .center_freq = 2427, .hw_value = 4, .band = NL80211_BAND_2GHZ, .flags = 0 },
    { .center_freq = 2432, .hw_value = 5, .band = NL80211_BAND_2GHZ, .flags = 0 },
    { .center_freq = 2437, .hw_value = 6, .band = NL80211_BAND_2GHZ, .flags = 0 },
    { .center_freq = 2442, .hw_value = 7, .band = NL80211_BAND_2GHZ, .flags = 0 },
    { .center_freq = 2447, .hw_value = 8, .band = NL80211_BAND_2GHZ, .flags = 0 },
    { .center_freq = 2452, .hw_value = 9, .band = NL80211_BAND_2GHZ, .flags = 0 },
    { .center_freq = 2457, .hw_value = 10, .band = NL80211_BAND_2GHZ, .flags = 0 },
    { .center_freq = 2462, .hw_value = 11, .band = NL80211_BAND_2GHZ, .flags = 0 },
    { .center_freq = 2467, .hw_value = 12, .band = NL80211_BAND_2GHZ, .flags = 0 },
    { .center_freq = 2472, .hw_value = 13, .band = NL80211_BAND_2GHZ, .flags = 0 },
};

static struct ieee80211_rate mt7603_rates_2ghz[] = {
    { .flags = IEEE80211_RATE_MANDATORY_B | IEEE80211_RATE_SHORT_PREAMBLE, .bitrate = 10, .hw_value = 0 },
    { .flags = IEEE80211_RATE_MANDATORY_B | IEEE80211_RATE_SHORT_PREAMBLE, .bitrate = 20, .hw_value = 1 },
    { .flags = IEEE80211_RATE_MANDATORY_B | IEEE80211_RATE_SHORT_PREAMBLE, .bitrate = 55, .hw_value = 2 },
    { .flags = IEEE80211_RATE_MANDATORY_B | IEEE80211_RATE_SHORT_PREAMBLE, .bitrate = 110, .hw_value = 3 },
    { .flags = IEEE80211_RATE_MANDATORY_G | IEEE80211_RATE_ERP_G, .bitrate = 60, .hw_value = 4 },
    { .flags = IEEE80211_RATE_ERP_G, .bitrate = 90, .hw_value = 5 },
    { .flags = IEEE80211_RATE_MANDATORY_G | IEEE80211_RATE_ERP_G, .bitrate = 120, .hw_value = 6 },
    { .flags = IEEE80211_RATE_ERP_G, .bitrate = 180, .hw_value = 7 },
    { .flags = IEEE80211_RATE_MANDATORY_G | IEEE80211_RATE_ERP_G, .bitrate = 240, .hw_value = 8 },
    { .flags = IEEE80211_RATE_ERP_G, .bitrate = 360, .hw_value = 9 },
    { .flags = IEEE80211_RATE_ERP_G, .bitrate = 480, .hw_value = 10 },
    { .flags = IEEE80211_RATE_ERP_G, .bitrate = 540, .hw_value = 11 },
};

static struct ieee80211_supported_band mt7603_band_2ghz = {
    .channels   = mt7603_channels_2ghz,
    .n_channels = ARRAY_SIZE(mt7603_channels_2ghz),
    .bitrates   = mt7603_rates_2ghz,
    .n_bitrates = ARRAY_SIZE(mt7603_rates_2ghz),
};

int mt7603_register_mac80211(struct usb_interface *intf, struct ieee80211_hw **out_hw)
{
    struct ieee80211_hw *hw;
    struct mt7603u_dev *dev;
    struct mt7603_eeprom_data eeprom;
    u8 dummy_e2p[512];
    int ret;

    pr_info("mt7603u ieee80211_ops size=%lud, offset start=%lud, stop=%lud, add_if=%lud, conf_filter=%lud, wake_tx=%lud\n",
            sizeof(struct ieee80211_ops),
            offsetof(struct ieee80211_ops, start),
            offsetof(struct ieee80211_ops, stop),
            offsetof(struct ieee80211_ops, add_interface),
            offsetof(struct ieee80211_ops, configure_filter),
            offsetof(struct ieee80211_ops, wake_tx_queue));

    hw = ieee80211_alloc_hw(sizeof(*dev), &mt7603_mac80211_ops);
    if (!hw) {
        pr_err("mt7603u: ieee80211_alloc_hw returned NULL\n");
        return -ENOMEM;
    }

    dev = hw->priv;
    dev->udev = interface_to_usbdev(intf);
    dev->hw = hw;

    /* Read the full 1024-byte eFuse EEPROM image. This holds the factory
     * calibration (TX power, XTAL trim, ELAN RX gain) required by the BBP.
     * Fall back to a dummy image (MAC only) if the bank is empty. */
    memset(dev->eeprom, 0xff, sizeof(dev->eeprom));
    if (mt7603_efuse_read_all(dev->udev, dev->eeprom, sizeof(dev->eeprom)) == 0) {
        dev->eeprom_valid = true;
        pr_info("mt7603u: eFuse EEPROM read OK (%.2x %.2x %.2x %.2x ...)\n",
                dev->eeprom[0], dev->eeprom[1], dev->eeprom[2], dev->eeprom[3]);
    } else {
        dev->eeprom_valid = false;
        pr_warn("mt7603u: eFuse EEPROM read failed, using dummy EEPROM\n");
    }

    if (dev->eeprom_valid) {
        memcpy(dummy_e2p, dev->eeprom, sizeof(dummy_e2p));
    } else {
        memset(dummy_e2p, 0xff, sizeof(dummy_e2p));
        dummy_e2p[0x04] = 0x00;
        dummy_e2p[0x05] = 0x0c;
        dummy_e2p[0x06] = 0x43;
        dummy_e2p[0x07] = 0x76;
        dummy_e2p[0x08] = 0x03;
        dummy_e2p[0x09] = 0x01;
    }

    if (mt7603_rust_parse_eeprom(dummy_e2p, sizeof(dummy_e2p), &eeprom) == 0 && eeprom.is_valid) {
        memcpy(dev->mac_addr, eeprom.mac_addr, ETH_ALEN);
        pr_info("mt7603u: parsed EEPROM MAC %pM\n", dev->mac_addr);
    } else {
        dev->mac_addr[0] = 0x00;
        dev->mac_addr[1] = 0x0c;
        dev->mac_addr[2] = 0x43;
        dev->mac_addr[3] = 0x76;
        dev->mac_addr[4] = 0x03;
        dev->mac_addr[5] = 0x01;
    }

    /* RSSI calibration offset = EEPROM[0x46] (vendor EEPROM_RSSI_BG_OFFSET,
     * common/eeprom.c:122-123), clamped to [-10,10] in Rust. Only meaningful
     * when the real eFuse image was read; otherwise 0. */
    dev->rssi_offset_2g = dev->eeprom_valid ? eeprom.rssi_offset_2g : 0;
    pr_info("mt7603u: eFuse calib: LNA[0x44]=%u 0x45=%u RSSIoff[0x46]=%d (0x%02x) 0x47=%u\n",
            dev->eeprom[0x44], dev->eeprom[0x45], dev->rssi_offset_2g,
            dev->eeprom[0x46], dev->eeprom[0x47]);

    init_usb_anchor(&dev->tx_anchor);

    SET_IEEE80211_DEV(hw, &intf->dev);
    hw->queues = 4;
    set_bit(IEEE80211_HW_SIGNAL_DBM, hw->flags);
    /* Reserve 32 bytes in front of every 802.11 TX frame for the
     * TMAC_TXD_L long TXD (vendor TXWISize = sizeof(TMAC_TXD_L) = 32). */
    hw->extra_tx_headroom = 32;
    hw->wiphy->max_scan_ssids = 4;
    hw->wiphy->max_scan_ie_len = 512;
    hw->wiphy->signal_type = CFG80211_SIGNAL_TYPE_MBM;

    SET_IEEE80211_PERM_ADDR(hw, dev->mac_addr);

    // Supported interface modes: Station & AP
    hw->wiphy->interface_modes = BIT(NL80211_IFTYPE_STATION) | BIT(NL80211_IFTYPE_AP);
    hw->wiphy->bands[NL80211_BAND_2GHZ] = &mt7603_band_2ghz;

    ret = ieee80211_register_hw(hw);
    if (ret < 0) {
        pr_err("mt7603u: ieee80211_register_hw failed with error: %d\n", ret);
        ieee80211_free_hw(hw);
        return ret;
    }

    *out_hw = hw;
    return 0;
}

void mt7603_unregister_mac80211(struct ieee80211_hw *hw)
{
    struct mt7603u_dev *dev;
    if (!hw) return;
    dev = hw->priv;
    dev->running = false;
    mt7603_stop_rx(dev);
    usb_kill_anchored_urbs(&dev->tx_anchor);
    ieee80211_unregister_hw(hw);
    ieee80211_free_hw(hw);
}
