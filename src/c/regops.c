/* MT7603U Register I/O via USB Control Requests */
#include <linux/module.h>
#include <linux/usb.h>
#include <linux/delay.h>
#include "mt7603u_rust.h"

/*
 * Vendor request encoding (common/mtusb_io.c):
 *   READ  = 0x63, WRITE = 0x66
 *   wValue = addr[31:16], wIndex = addr[15:0]
 * Address must first be mapped via mt_physical_addr_map.
 */
#define MT_VEND_REQ_READ  0x63
#define MT_VEND_REQ_WRITE 0x66

int mt7603_usb_read_reg(struct usb_device *udev, u32 addr, u32 *val)
{
    int ret;
    u32 mapped;
    u32 *buf = kmalloc(sizeof(u32), GFP_KERNEL);
    if (!buf) return -ENOMEM;

    mapped = mt7603_rust_map_register_addr(addr);

    ret = usb_control_msg(udev, usb_rcvctrlpipe(udev, 0),
                          MT_VEND_REQ_READ,
                          USB_DIR_IN | USB_TYPE_VENDOR | USB_RECIP_DEVICE,
                          mapped >> 16, mapped & 0xffff,
                          buf, sizeof(u32), 1000);
    if (ret >= 0) {
        *val = le32_to_cpu(*buf);
        ret = 0;
    }
    kfree(buf);
    return ret;
}

int mt7603_usb_write_reg(struct usb_device *udev, u32 addr, u32 val)
{
    int ret;
    u32 mapped;
    u32 *buf = kmalloc(sizeof(u32), GFP_KERNEL);
    if (!buf) return -ENOMEM;

    mapped = mt7603_rust_map_register_addr(addr);

    *buf = cpu_to_le32(val);
    ret = usb_control_msg(udev, usb_sndctrlpipe(udev, 0),
                          MT_VEND_REQ_WRITE,
                          USB_DIR_OUT | USB_TYPE_VENDOR | USB_RECIP_DEVICE,
                          mapped >> 16, mapped & 0xffff,
                          buf, sizeof(u32), 1000);
    kfree(buf);
    return ret < 0 ? ret : 0;
}

/*
 * UDMA_WLCFG_0 (0x50029018) access. The vendor USB_CFG_READ/WRITE path
 * (mtusb_cfg_read/mtusb_cfg_write) issues the vendor request with the RAW
 * 0x50029018 address, NOT through mt_physical_addr_map. Routing this through
 * mt7603_usb_read_reg/write_reg would remap 0x50029018 -> 0xeffe9018 (WTBL
 * region) and write the wrong location, leaving UDMA bulk paths disabled and
 * the ROM unable to deliver command responses.
 */
int mt7603_usb_cfg_read(struct usb_device *udev, u32 addr, u32 *val)
{
    int ret;
    u32 *buf = kmalloc(sizeof(u32), GFP_KERNEL);
    if (!buf) return -ENOMEM;

    ret = usb_control_msg(udev, usb_rcvctrlpipe(udev, 0),
                          MT_VEND_REQ_READ,
                          USB_DIR_IN | USB_TYPE_VENDOR | USB_RECIP_DEVICE,
                          addr >> 16, addr & 0xffff,
                          buf, sizeof(u32), 1000);
    if (ret >= 0) {
        *val = le32_to_cpu(*buf);
        ret = 0;
    }
    kfree(buf);
    return ret;
}

int mt7603_usb_cfg_write(struct usb_device *udev, u32 addr, u32 val)
{
    int ret;
    u32 *buf = kmalloc(sizeof(u32), GFP_KERNEL);
    if (!buf) return -ENOMEM;

    *buf = cpu_to_le32(val);
    ret = usb_control_msg(udev, usb_sndctrlpipe(udev, 0),
                          MT_VEND_REQ_WRITE,
                          USB_DIR_OUT | USB_TYPE_VENDOR | USB_RECIP_DEVICE,
                          addr >> 16, addr & 0xffff,
                          buf, sizeof(u32), 1000);
    kfree(buf);
    return ret < 0 ? ret : 0;
}

int mt7603_execute_reg_ops(struct usb_device *udev, const struct reg_write_op *ops, size_t count)
{
    size_t i;
    int ret;
    for (i = 0; i < count; i++) {
        ret = mt7603_usb_write_reg(udev, ops[i].addr, ops[i].val);
        if (ret < 0) return ret;
    }
    return 0;
}

/*
 * eFuse EEPROM access (openwrt mt7603_efuse_read).
 * MT_EFUSE_BASE = 0x81070000 maps through physical_addr_map as identity.
 */
#define MT_EFUSE_BASE_CTRL     0x81070000
#define MT_EFUSE_CTRL          0x81070008
#define MT_EFUSE_RDATA(_n)     (0x81070030U + 4U * (_n))
#define MT_EFUSE_CTRL_AIN      (0x3ffU << 16)
#define MT_EFUSE_CTRL_MODE     (0x3U << 6)
#define MT_EFUSE_CTRL_KICK     (1U << 30)
#define MT_EFUSE_CTRL_AOUT     (0x3fU)
#define MT_EFUSE_CTRL_VALID    (1U << 29)
#define MT_EFUSE_BASE_CTRL_EMPTY (1U << 30)

int mt7603_efuse_read_block(struct usb_device *udev, u16 addr, u8 *data)
{
    u32 val;
    int i, ret;

    ret = mt7603_usb_read_reg(udev, MT_EFUSE_CTRL, &val);
    if (ret) return ret;
    val &= ~(MT_EFUSE_CTRL_AIN | MT_EFUSE_CTRL_MODE);
    val |= ((u32)(addr & ~0xf) << 16) | MT_EFUSE_CTRL_KICK;
    ret = mt7603_usb_write_reg(udev, MT_EFUSE_CTRL, val);
    if (ret) return ret;

    for (i = 0; i < 100; i++) {
        usleep_range(2, 5);
        ret = mt7603_usb_read_reg(udev, MT_EFUSE_CTRL, &val);
        if (ret) return ret;
        if (!(val & MT_EFUSE_CTRL_KICK)) break;
    }
    if (val & MT_EFUSE_CTRL_KICK) return -ETIMEDOUT;

    usleep_range(2, 5);
    ret = mt7603_usb_read_reg(udev, MT_EFUSE_CTRL, &val);
    if (ret) return ret;
    if ((val & MT_EFUSE_CTRL_AOUT) == MT_EFUSE_CTRL_AOUT ||
        !(val & MT_EFUSE_CTRL_VALID)) {
        memset(data, 0xff, 16);
        return 0;
    }

    for (i = 0; i < 4; i++) {
        ret = mt7603_usb_read_reg(udev, MT_EFUSE_RDATA(i), &val);
        if (ret) return ret;
        data[4 * i]     = val & 0xff;
        data[4 * i + 1] = (val >> 8) & 0xff;
        data[4 * i + 2] = (val >> 16) & 0xff;
        data[4 * i + 3] = (val >> 24) & 0xff;
    }
    return 0;
}

/* Reads the full 1024-byte eFuse EEPROM image into buf.
 * Returns -ENODATA if the eFuse bank is empty. */
int mt7603_efuse_read_all(struct usb_device *udev, u8 *buf, size_t len)
{
    size_t i;
    int ret;
    u32 val;

    ret = mt7603_usb_read_reg(udev, MT_EFUSE_BASE_CTRL, &val);
    if (ret) return ret;
    if (val & MT_EFUSE_BASE_CTRL_EMPTY) return -ENODATA;

    for (i = 0; i + 16 <= len; i += 16) {
        ret = mt7603_efuse_read_block(udev, i, buf + i);
        if (ret) return ret;
    }
    return 0;
}
