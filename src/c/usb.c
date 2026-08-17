/* MT7603U USB Probe & Disconnect */
#include <linux/module.h>
#include <linux/usb.h>
#include <linux/firmware.h>
#include <linux/delay.h>
#include <linux/completion.h>
#include <net/mac80211.h>
#include "mt7603u_rust.h"

#define MT7603_VID 0x0e8d
#define MT7603_PID 0x7603
#define MT7603_PID_ALT 0x760c
#define MT7603_FIRMWARE_NAME "mt7603u.bin"

/* USB endpoints (chips/mt7603.c) */
#define MT7603_CMD_BULK_OUT  0x08   /* CommandBulkOutAddr */
#define MT7603_CMD_RSP_BULK_IN 0x85 /* CommandRspBulkInAddr (RX_RING1, normal mode) */
#define MT7603_DATA_BULK_IN  0x84   /* DataBulkInAddr (carries cmd rsp during download) */
#define MT7603_USB_END_PADDING 4

/* Firmware download registers (mcu/andes_mt.c) */
#define MT7603_SCH_REG4      0x4594
#define MT7603_TOP_MISC2     0x1134
#define MT7603_SCH_REG4_FORCE_QID_MASK  0x0f
#define MT7603_SCH_REG4_BYPASS_MODE_MASK (0x1 << 5)
#define MT7603_SCH_REG4_FORCE_QID_USB   8
#define MT7603_SCH_REG4_PSE_RESET_BIT   (1 << 8)

/*
 * USB DMA (UDMA) config. Vendor enables this BEFORE firmware download
 * (AndesMTUsbFwInit -> RT28XXDMAEnable, common/cmm_mac_usb.c MT7603 branch),
 * writing UDMA_WLCFG_0 (0x50029018) via vendor request 0x66. Without
 * RxBulkEn/TxBulkEn the ROM code's command response never reaches the
 * bulk-IN endpoint (observed as bulk-in timeout -110).
 */
#define MT7603_UDMA_WLCFG_0      0x50029018
#define MT7603_UDMA_RX_AGG_TOUT  0x80            /* bits 0-7 */
#define MT7603_UDMA_RX_AGG_LMT   0x15            /* bits 8-15: (2048*12/1024)-3 = 21 */
#define MT7603_UDMA_RX_MPSZ_PAD0 (1 << 18)
#define MT7603_UDMA_RX_AGG_EN    (1 << 21)
#define MT7603_UDMA_RX_BULK_EN   (1 << 22)
#define MT7603_UDMA_TX_BULK_EN   (1 << 23)

/* Firmware download parameters */
#define MT7603_FW_CODE_START_ADDRESS1 0x100000
#define MT7603_TARGET_ADDR_LEN_NEED_RSP 0x80000000
#define MT7603_FW_SCATTER_MAX_PAYLOAD 4064  /* 4096 - sizeof(FW_TXD) */
#define MT7603_ROM_READY_POLL_MAX     500   /* iterations x 1ms */
#define MT7603_CMD_RSP_TIMEOUT_MS     3000
/* EP 0x85 (CommandRspBulkInAddr) URB size: vendor UsbRxCmdMsgSubmit hardcodes
 * 512 (andes_core.c:520/564). EP 0x84 (DataBulkInAddr) URB size: vendor
 * RTUSBInitRxDesc (rtusb_bulk.c:395-397) uses MAX_RXBULK_SIZE (24576 =
 * LOCAL_TXBUF_SIZE*RXBULKAGGRE_SIZE, rtmp_usb.h:42) when
 * BulkInMaxPacketSize >= 512, else 4096. Our device MPS=512 -> 24576.
 * A 512B EP 0x84 URB never completes because UDMA aggregation fills the
 * device's bulk-IN ring with larger transfer units. */
#define MT7603_CMD_RSP_BULK_SIZE      512
#define MT7603_DATA_RSP_BULK_SIZE     24576

static const struct usb_device_id mt7603u_device_table[] = {
    { USB_DEVICE(MT7603_VID, MT7603_PID) },
    { USB_DEVICE(MT7603_VID, MT7603_PID_ALT) },
    { }
};
MODULE_DEVICE_TABLE(usb, mt7603u_device_table);

extern int mt7603_execute_reg_ops(struct usb_device *udev, const struct reg_write_op *ops, size_t count);
extern int mt7603_register_mac80211(struct usb_interface *intf, struct ieee80211_hw **out_hw);
extern void mt7603_unregister_mac80211(struct ieee80211_hw *hw);

/*
 * Send one MCU command frame (FW_TXD header + payload) to the command
 * bulk-out endpoint (EP6, 0x08), appending the required 4-byte zero padding.
 */
int mt7603_usb_send_cmd(struct usb_device *udev, const u8 *frame, size_t frame_len)
{
    u8 *buf;
    int ret;

    buf = kmalloc(frame_len + MT7603_USB_END_PADDING, GFP_KERNEL);
    if (!buf)
        return -ENOMEM;

    memcpy(buf, frame, frame_len);
    memset(buf + frame_len, 0, MT7603_USB_END_PADDING);

    ret = usb_bulk_msg(udev, usb_sndbulkpipe(udev, MT7603_CMD_BULK_OUT),
                       buf, frame_len + MT7603_USB_END_PADDING, NULL, 500);
    dev_dbg(&udev->dev, "MT7603U: bulk-out send len=%zu ret=%d cid=0x%02x\n",
            frame_len + MT7603_USB_END_PADDING, ret, frame_len >= 5 ? frame[4] : 0);
    kfree(buf);
    return ret < 0 ? ret : 0;
}

/*
 * Command-response reception during firmware download.
 *
 * During download the chip runs in bypass mode (SCH_REG4 bypass=1, force
 * QID 8); the ROM delivers its command response (an EVENT_RXD frame) on the
 * DATA bulk-IN endpoint (EP 0x84, DataBulkInAddr), NOT on the command-response
 * endpoint (EP 0x85). Verified on hardware: a pending EP 0x84 URB received
 * `0c 00 00 e0 01 02 00 00 ...` = EVENT_RXD{length=12, pkt_type_id=0xe000,
 * eid=0x01 (MT_TARGET_ADDRESS_LEN_RSP), seq=2} right after the address/len
 * command. EP 0x85 (CommandRspBulkInAddr, RX_RING1) only carries command
 * responses once the firmware is running in normal mode.
 *
 * The vendor driver keeps both a 512-byte EP 0x85 URB (UsbRxCmdMsgsReceive)
 * and a large EP 0x84 URB (RTUSBBulkReceive -> DoBulkIn) outstanding, and its
 * data RX path (RTUSBBulkRxComplete -> parse_rx_packet_type -> RX_EVENT ->
 * AndesMTRxEventHandler -> AndesMTRxProcessEvent) matches the response seq
 * against the ackq. We reproduce the pending-URB model on the correct (data)
 * endpoint: submit the IN URB first, send the command, then wait.
 */
struct mt7603_rsp_session {
    struct completion done;
    struct completion *gate; /* optional: fired when ANY session completes */
    struct urb *urb;
    u8 *buf;
    int status;
    int actual;
};

static void mt7603_rsp_complete(struct urb *urb)
{
    struct mt7603_rsp_session *s = urb->context;
    int i, n = urb->actual_length;

    if (n > 32)
        n = 32;
    s->status = urb->status;
    s->actual = urb->actual_length;
    dev_info(&urb->dev->dev,
             "MT7603U: rsp urb complete status=%d actual=%d\n",
             urb->status, urb->actual_length);
    for (i = 0; i < n; i += 16)
        dev_info(&urb->dev->dev, "MT7603U: rsp[%02d] %*ph\n", i, 16, &s->buf[i]);
    complete(&s->done);
    if (s->gate)
        complete(s->gate);
}

/* Submit a pending response URB *before* the need_rsp command is sent.
 * `ep` selects the bulk-IN endpoint: EP 0x84 (DataBulkInAddr, download mode)
 * or EP 0x85 (CommandRspBulkInAddr, normal mode). The buffer size is chosen
 * per endpoint to match the vendor: EP 0x84 gets MAX_RXBULK_SIZE (24576)
 * since UDMA aggregation on a MPS>=512 device fills large transfer units;
 * EP 0x85 keeps 512 as the vendor UsbRxCmdMsgSubmit hardcodes. */
static int mt7603_rsp_submit(struct usb_device *udev, u8 ep, struct mt7603_rsp_session *s)
{
    size_t buf_size = (ep == MT7603_DATA_BULK_IN) ?
                      MT7603_DATA_RSP_BULK_SIZE : MT7603_CMD_RSP_BULK_SIZE;
    int ret;

    init_completion(&s->done);
    s->urb = NULL;
    s->status = 0;
    s->actual = 0;

    s->buf = kmalloc(buf_size, GFP_KERNEL);
    if (!s->buf)
        return -ENOMEM;

    s->urb = usb_alloc_urb(0, GFP_KERNEL);
    if (!s->urb) {
        kfree(s->buf);
        return -ENOMEM;
    }

    usb_fill_bulk_urb(s->urb, udev,
                      usb_rcvbulkpipe(udev, ep),
                      s->buf, buf_size,
                      mt7603_rsp_complete, s);

    ret = usb_submit_urb(s->urb, GFP_KERNEL);
    if (ret < 0) {
        dev_err(&udev->dev, "MT7603U: submit rsp urb failed (%d)\n", ret);
        usb_free_urb(s->urb);
        kfree(s->buf);
        s->urb = NULL;
        return ret;
    }
    dev_info(&udev->dev, "MT7603U: rsp urb submitted (pipe=0x%x, len=%zu)\n",
             usb_rcvbulkpipe(udev, ep), buf_size);
    return 0;
}

/* Wait for the previously-submitted response URB to complete. */
static int mt7603_rsp_wait(struct usb_device *udev, struct mt7603_rsp_session *s)
{
    long left;

    if (!s->urb)
        return -EINVAL;

    left = wait_for_completion_timeout(&s->done,
                                       msecs_to_jiffies(MT7603_CMD_RSP_TIMEOUT_MS));
    if (left == 0) {
        dev_err(&udev->dev, "MT7603U: cmd rsp timeout (no response)\n");
        usb_kill_urb(s->urb);
        usb_free_urb(s->urb);
        kfree(s->buf);
        s->urb = NULL;
        return -ETIMEDOUT;
    }

    dev_info(&udev->dev, "MT7603U: cmd rsp: status=%d actual=%d first=0x%02x\n",
             s->status, s->actual, s->actual > 0 ? s->buf[0] : 0);
    usb_free_urb(s->urb);
    kfree(s->buf);
    s->urb = NULL;
    return s->status ? s->status : 0;
}

/*
 * Send CmdRestartDLReq and wait for its ack.
 *
 * Unlike the address/len and fw-start commands (sent while the ROM runs in
 * bypass mode, ack on EP 0x84), the restart-dl is sent while the RAM firmware
 * is STILL RUNNING in normal mode, so its ack may be delivered on either
 * EP 0x84 (DataBulkInAddr) or EP 0x85 (CommandRspBulkInAddr). The vendor keeps
 * both a 512-byte EP 0x85 URB (UsbRxCmdMsgsReceive) and a large EP 0x84 URB
 * (RTUSBBulkReceive) outstanding before the download, and its RX path accepts
 * the EVENT_RXD from either. We mirror that by submitting a pending URB on
 * both endpoints, sending the command, then waiting on a shared gate.
 */
static int mt7603_restart_dl_rsp(struct usb_device *udev, const u8 *cmd, size_t cmd_len)
{
    struct mt7603_rsp_session s84 = {0}, s85 = {0};
    struct completion gate;
    long left;
    int ret;

    init_completion(&gate);
    s84.gate = &gate;
    s85.gate = &gate;

    ret = mt7603_rsp_submit(udev, MT7603_DATA_BULK_IN, &s84);
    if (ret < 0)
        return ret;
    ret = mt7603_rsp_submit(udev, MT7603_CMD_RSP_BULK_IN, &s85);
    if (ret < 0) {
        usb_kill_urb(s84.urb);
        usb_free_urb(s84.urb);
        kfree(s84.buf);
        return ret;
    }

    ret = mt7603_usb_send_cmd(udev, cmd, cmd_len);
    if (ret < 0)
        goto out;

    left = wait_for_completion_timeout(&gate,
                                       msecs_to_jiffies(MT7603_CMD_RSP_TIMEOUT_MS));
    if (left == 0) {
        /* The RAM firmware may not ACK the restart (e.g. it was just stopped
         * and re-probed, or its MCU is busy). Per the vendor flow
         * (AndesMTLoadFwMethod1), an ACK is *not* required: after sending
         * CmdRestartDLReq the vendor simply polls TOP_MISC2 for the ROM to
         * become ready (bit0=1 && bit1=0). If the restart already took
         * effect, the poll below succeeds and the download proceeds; if not,
         * the poll times out and download fails. Report the ACK timeout as
         * informational only. */
        dev_warn(&udev->dev,
                 "MT7603U: restart-dl ack timeout on EP0x84/EP0x85 (polling ROM ready)\n");
        ret = 0;
        goto out;
    }

    if (s84.actual > 0)
        dev_info(&udev->dev, "MT7603U: restart-dl ack on EP0x84 (status=%d actual=%d first=0x%02x)\n",
                 s84.status, s84.actual, s84.buf[0]);
    if (s85.actual > 0)
        dev_info(&udev->dev, "MT7603U: restart-dl ack on EP0x85 (status=%d actual=%d first=0x%02x)\n",
                 s85.status, s85.actual, s85.buf[0]);
    ret = 0;

out:
    usb_kill_urb(s84.urb);
    usb_free_urb(s84.urb);
    kfree(s84.buf);
    usb_kill_urb(s85.urb);
    usb_free_urb(s85.urb);
    kfree(s85.buf);
    return ret;
}

/*
 * Poll TOP_MISC2 (0x1134) until condition on mask is met.
 * Returns 0 on success, -ETIMEDOUT on failure.
 */
static int mt7603_poll_top_misc2(struct usb_device *udev, u32 mask, u32 expect)
{
    int i;
    u32 val;

    for (i = 0; i < MT7603_ROM_READY_POLL_MAX; i++) {
        if (mt7603_usb_read_reg(udev, MT7603_TOP_MISC2, &val) < 0)
            return -EIO;
        if ((val & mask) == expect)
            return 0;
        msleep(1);
    }
    return -ETIMEDOUT;
}

/*
 * Enable the USB DMA (UDMA) bulk IN/OUT paths. Port of the MT7603 branch of
 * vendor RT28XXDMAEnable (common/cmm_mac_usb.c), which runs in
 * AndesMTUsbFwInit before NICLoadFirmware. Required for the ROM code to
 * deliver command responses to the bulk-IN endpoint.
 */
int mt7603_usb_enable_udma(struct usb_device *udev, bool rx_agg)
{
    u32 val;
    int ret;

    ret = mt7603_usb_cfg_read(udev, MT7603_UDMA_WLCFG_0, &val);
    if (ret < 0)
        return ret;

    val &= ~(0x0000ffff | MT7603_UDMA_RX_AGG_EN);
    val |= MT7603_UDMA_RX_AGG_TOUT | (MT7603_UDMA_RX_AGG_LMT << 8);
    if (rx_agg) {
        val |= MT7603_UDMA_RX_AGG_EN;
    }
    val |= MT7603_UDMA_RX_MPSZ_PAD0;
    val |= MT7603_UDMA_RX_BULK_EN | MT7603_UDMA_TX_BULK_EN;

    ret = mt7603_usb_cfg_write(udev, MT7603_UDMA_WLCFG_0, val);
    if (ret < 0)
        return ret;
    {
        u32 rdbk = 0;
        mt7603_usb_cfg_read(udev, MT7603_UDMA_WLCFG_0, &rdbk);
        dev_info(&udev->dev, "MT7603U: UDMA enabled (rx_agg=%d, UDMA_WLCFG_0=0x%08x, readback=0x%08x)\n",
                 rx_agg, val, rdbk);
    }
    return 0;
}

/*
 * Firmware download sequence (port of AndesMTLoadFwMethod1).
 * Returns 0 on success, negative errno otherwise.
 */
/*
 * Allocate the next need_wait command sequence number. Port of vendor
 * AndesGetCmdMsgSeq (mcu/andes_core.c): cmd_seq >= 0xf ? 1 : cmd_seq++,
 * where cmd_seq starts at 0 so the first need_wait command gets seq=1.
 * seq=0 is reserved for no-wait commands (e.g. CmdFwScatter).
 */
static u8 mt7603_next_cmd_seq(u8 *seq)
{
    *seq = (*seq >= 0xf) ? 1 : (u8)(*seq + 1);
    return *seq;
}

static int mt7603_download_firmware(struct usb_device *udev, const struct firmware *fw)
{
    u32 val, dl_len;
    u8 *cmd;
    u8 cmd_seq = 0;
    size_t cmd_len;
    size_t offset = 0;
    int ret;
    struct mt7603_rsp_session rsp;

    /* 0. Enable USB DMA bulk paths (vendor AndesMTUsbFwInit) */
    ret = mt7603_usb_enable_udma(udev, true);
    if (ret < 0) {
        dev_err(&udev->dev, "MT7603U: UDMA enable failed (%d)\n", ret);
        return ret;
    }

    /* 1. If RAM firmware already running, issue CmdRestartDLReq to jump back to ROM */
    ret = mt7603_usb_read_reg(udev, MT7603_TOP_MISC2, &val);
    if (ret < 0)
        return ret;

    if ((val & 0x02) == 0x02) {
        u8 restart_cmd[MT7603_FW_SCATTER_MAX_PAYLOAD + 32];
        size_t restart_len = 0;

        dev_info(&udev->dev, "MT7603U: firmware already running (TOP_MISC2=0x%x), issuing CmdRestartDLReq\n", val);
        ret = mt7603_rust_build_restart_dl_req(mt7603_next_cmd_seq(&cmd_seq),
                                               restart_cmd,
                                               sizeof(restart_cmd),
                                               &restart_len);
        if (ret == 0) {
            mt7603_restart_dl_rsp(udev, restart_cmd, restart_len);
        }
    }

    /* 2. Switch to bypass mode + force QID 8 for firmware download */
    ret = mt7603_usb_read_reg(udev, MT7603_SCH_REG4, &val);
    if (ret < 0)
        return ret;

    val &= ~MT7603_SCH_REG4_BYPASS_MODE_MASK;
    val |= MT7603_SCH_REG4_BYPASS_MODE_MASK; /* bypass(1) */
    val &= ~MT7603_SCH_REG4_FORCE_QID_MASK;
    val |= MT7603_SCH_REG4_FORCE_QID_USB;
    ret = mt7603_usb_write_reg(udev, MT7603_SCH_REG4, val);
    if (ret < 0)
        return ret;
    {
        u32 rdbk4 = 0;
        mt7603_usb_read_reg(udev, MT7603_SCH_REG4, &rdbk4);
        dev_info(&udev->dev, "MT7603U: SCH_REG4 bypass set (val=0x%08x, readback=0x%08x)\n", val, rdbk4);
    }

    /* Allocate the shared command frame buffer (max scatter payload + header). */
    cmd = kmalloc(MT7603_FW_SCATTER_MAX_PAYLOAD + 32, GFP_KERNEL);
    if (!cmd) {
        ret = -ENOMEM;
        goto restore;
    }

    /* 3. Wait for ROM code ready: bit0=1 && bit1=0 */

    /* 3. Wait for ROM code ready: bit0=1 && bit1=0 */
    ret = mt7603_poll_top_misc2(udev, 0x03, 0x01);
    if (ret < 0) {
        u32 misc2 = 0;
        mt7603_usb_read_reg(udev, MT7603_TOP_MISC2, &misc2);
        dev_err(&udev->dev, "MT7603U: ROM code not ready (TOP_MISC2=0x%x)\n", misc2);
        goto free_cmd;
    }

    /* 4. CmdAddressLenReq: dl_len = le32(fw tail) + 4 (CRC) */
    ret = mt7603_rust_fw_dl_len(fw->data, fw->size, &dl_len);
    if (ret < 0) {
        dev_err(&udev->dev, "MT7603U: invalid firmware download length\n");
        goto free_cmd;
    }

    ret = mt7603_rust_build_addr_len_req(MT7603_FW_CODE_START_ADDRESS1, dl_len,
                                         mt7603_next_cmd_seq(&cmd_seq),
                                         cmd, MT7603_FW_SCATTER_MAX_PAYLOAD + 32, &cmd_len);
    if (ret < 0) {
        dev_err(&udev->dev, "MT7603U: build_addr_len_req failed (%d)\n", ret);
        goto free_cmd;
    }
    ret = mt7603_rsp_submit(udev, MT7603_DATA_BULK_IN, &rsp);
    if (ret < 0)
        goto free_cmd;
    ret = mt7603_usb_send_cmd(udev, cmd, cmd_len);
    if (ret < 0) {
        dev_err(&udev->dev, "MT7603U: address/len req send failed (%d)\n", ret);
        goto free_cmd;
    }
    mt7603_rsp_wait(udev, &rsp);

    /* 5. CmdFwScatters: upload dl_len bytes (firmware body + CRC), NOT the whole
     * file. The final 36 bytes are the ILM/DLM trailer consumed only by
     * AndesMTLoadFwMethod2; Method1 (vendor) uploads exactly dl_len bytes. */
    if (dl_len > fw->size) {
        dev_err(&udev->dev, "MT7603U: download len %u exceeds firmware size %zu\n",
                dl_len, fw->size);
        ret = -EINVAL;
        goto free_cmd;
    }

    while (offset < (size_t)dl_len) {
        size_t chunk_len = (size_t)dl_len - offset;
        size_t out_len;

        if (chunk_len > MT7603_FW_SCATTER_MAX_PAYLOAD)
            chunk_len = MT7603_FW_SCATTER_MAX_PAYLOAD;

        ret = mt7603_rust_build_fw_scatter_frame(fw->data + offset, chunk_len,
                                                 cmd, MT7603_FW_SCATTER_MAX_PAYLOAD + 32, &out_len);
        if (ret < 0) {
            dev_err(&udev->dev, "MT7603U: build scatter frame failed (%d)\n", ret);
            goto free_cmd;
        }
        ret = mt7603_usb_send_cmd(udev, cmd, out_len);
        if (ret < 0) {
            dev_err(&udev->dev, "MT7603U: scatter send failed at offset %zu (%d)\n", offset, ret);
            goto free_cmd;
        }
        offset += chunk_len;
    }
    dev_info(&udev->dev, "MT7603U: firmware image uploaded (%u bytes, %zu chunks)\n",
             dl_len, ((size_t)dl_len + MT7603_FW_SCATTER_MAX_PAYLOAD - 1) / MT7603_FW_SCATTER_MAX_PAYLOAD);

    /* 6. CmdFwStartReq(override=1, entry=0x100000) */
    ret = mt7603_rust_build_fw_start_req(1, MT7603_FW_CODE_START_ADDRESS1,
                                         mt7603_next_cmd_seq(&cmd_seq),
                                         cmd, MT7603_FW_SCATTER_MAX_PAYLOAD + 32, &cmd_len);
    if (ret < 0) {
        dev_err(&udev->dev, "MT7603U: build_fw_start_req failed (%d)\n", ret);
        goto free_cmd;
    }
    ret = mt7603_rsp_submit(udev, MT7603_DATA_BULK_IN, &rsp);
    if (ret < 0)
        goto free_cmd;
    ret = mt7603_usb_send_cmd(udev, cmd, cmd_len);
    if (ret < 0) {
        dev_err(&udev->dev, "MT7603U: fw start req send failed (%d)\n", ret);
        goto free_cmd;
    }
    mt7603_rsp_wait(udev, &rsp);

    /* 7. Poll until RAM firmware running: bit1=1 */
    ret = mt7603_poll_top_misc2(udev, 0x02, 0x02);
    if (ret < 0) {
        dev_err(&udev->dev, "MT7603U: firmware loading failure (TOP_MISC2 bit1 not set)\n");
        goto free_cmd;
    }
    dev_info(&udev->dev, "MT7603U: firmware is running\n");
    ret = 0;

free_cmd:
    kfree(cmd);
restore:
    /* 8. Switch back to normal mode + pulse PSE reset bit */
    if (mt7603_usb_read_reg(udev, MT7603_SCH_REG4, &val) == 0) {
        val &= ~MT7603_SCH_REG4_BYPASS_MODE_MASK;
        val &= ~MT7603_SCH_REG4_FORCE_QID_MASK;
        mt7603_usb_write_reg(udev, MT7603_SCH_REG4, val);

        val |= MT7603_SCH_REG4_PSE_RESET_BIT;
        mt7603_usb_write_reg(udev, MT7603_SCH_REG4, val);
        val &= ~MT7603_SCH_REG4_PSE_RESET_BIT;
        mt7603_usb_write_reg(udev, MT7603_SCH_REG4, val);
    }
    return ret;
}

static int mt7603u_probe(struct usb_interface *intf, const struct usb_device_id *id)
{
    struct usb_device *udev = interface_to_usbdev(intf);
    struct ieee80211_hw *hw = NULL;
    const struct firmware *fw = NULL;
    struct reg_write_op ops[32];
    size_t count = 0;
    int ret;

    dev_info(&intf->dev, "MT7603U USB device detected (VID: 0x%04x, PID: 0x%04x)\n",
             id->idVendor, id->idProduct);

    // Step 1: Request and verify vendor firmware image
    ret = request_firmware(&fw, MT7603_FIRMWARE_NAME, &intf->dev);
    if (ret < 0) {
        dev_err(&intf->dev, "Failed to load firmware %s: %d\n", MT7603_FIRMWARE_NAME, ret);
        return ret;
    }

    ret = mt7603_rust_verify_firmware(fw->data, fw->size);
    if (ret < 0) {
        dev_err(&intf->dev, "Firmware %s header verification failed in Rust logic: %d\n", MT7603_FIRMWARE_NAME, ret);
        release_firmware(fw);
        return ret;
    }
    dev_info(&intf->dev, "Firmware %s verified successfully (%zu bytes, Andes N9 E2 image)\n",
             MT7603_FIRMWARE_NAME, fw->size);

    // Step 2: Download firmware to the chip (AndesMTLoadFwMethod1 sequence)
    ret = mt7603_download_firmware(udev, fw);
    release_firmware(fw);
    if (ret < 0) {
        dev_err(&intf->dev, "Firmware download failed: %d\n", ret);
        return ret;
    }

    // Step 3: Call Rust to get MAC init sequence
    ret = mt7603_rust_get_mac_init_sequence(ops, 32, &count);
    if (ret < 0) {
        dev_err(&intf->dev, "Failed to get MAC init sequence from Rust logic: %d\n", ret);
        return ret;
    }

    // Step 4: Execute register operations on USB hardware
    ret = mt7603_execute_reg_ops(udev, ops, count);
    if (ret < 0) {
        dev_warn(&intf->dev, "Hardware reg init notice (%d), proceeding with mac80211 registration...\n", ret);
    } else {
        dev_info(&intf->dev, "MT7603U MAC initialized successfully (%zu register ops)\n", count);
    }

    // Step 5: Register device into Linux mac80211 WiFi Subsystem
    ret = mt7603_register_mac80211(intf, &hw);
    if (ret < 0) {
        dev_err(&intf->dev, "Failed to register mac80211 hardware: %d\n", ret);
        return ret;
    }

    usb_set_intfdata(intf, hw);
    dev_info(&intf->dev, "MT7603U mac80211 hardware registered successfully (wlan interface ready)\n");
    return 0;
}

static void mt7603u_disconnect(struct usb_interface *intf)
{
    struct ieee80211_hw *hw = usb_get_intfdata(intf);
    dev_info(&intf->dev, "MT7603U USB device disconnecting\n");
    mt7603_unregister_mac80211(hw);
    usb_set_intfdata(intf, NULL);
    dev_info(&intf->dev, "MT7603U USB device disconnected & unregistered\n");
}

struct usb_driver mt7603u_usb_driver = {
    .name       = "mt7603u",
    .id_table   = mt7603u_device_table,
    .probe      = mt7603u_probe,
    .disconnect = mt7603u_disconnect,
};
