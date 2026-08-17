/* MT7603U C Skeleton / Rust FFI Header */
#ifndef _MT7603U_RUST_H_
#define _MT7603U_RUST_H_

#include <linux/types.h>

struct reg_write_op {
    uint32_t addr;
    uint32_t val;
};

struct mt7603_eeprom_data {
    uint8_t mac_addr[6];
    uint8_t tx_power_2g[14];
    uint8_t nic_config;
    uint8_t country_code[2];
    uint16_t eeprom_version;
    int8_t rssi_offset_2g;  /* EEPROM 0x46 signed, clamped to [-10,10] */
    uint8_t is_valid;
};

struct mt7603_rx_info {
    uint16_t pkt_len;
    uint16_t hdr_len;
    int8_t rssi;
    uint8_t channel;
    uint8_t rate;
    uint8_t is_beacon;
    uint8_t is_data;
    uint8_t is_crc_error;
};

struct mt7603_tx_params {
    uint8_t rate_idx;   /* legacy mac80211 rate idx / TMI mcs */
    uint8_t pid;        /* TXD wlan_idx (WCID), 0 for unassociated broadcast */
    uint8_t queue;      /* TXD q_idx (Q_IDX_AC4 = 0x04 for mgmt on MT_MAC) */
    uint8_t hdr_len;    /* 802.11 header length (24 for probe request) */
    uint8_t frm_type;   /* FC type: 0=mgmt, 1=ctl, 2=data */
    uint8_t sub_type;   /* FC subtype (probe request = 4) */
    uint8_t no_ack;     /* 1 if no ACK required (probe request) */
    uint8_t is_bm;      /* 1 if broadcast/multicast */
    uint8_t rate_mode;  /* MODE_CCK(0)/MODE_OFDM(1)/MODE_HTMIX(2)/MODE_HTGF(3) */
    uint8_t rate_mcs;   /* CCK: 0=1M,1=2M,2=5.5M,3=11M */
    uint8_t preamble;   /* SHORT_PREAMBLE(0)/LONG_PREAMBLE(1) */
    uint8_t bw;         /* BW_20(0)/BW_40(1) */
    uint16_t pkt_len;   /* 802.11 frame length excluding TXD */
};

struct mt7603_sta_bss_info {
    uint8_t bssid[6];
    uint8_t ssid[32];
    uint8_t ssid_len;
    uint8_t channel;
    int8_t rssi;
    uint16_t capability;
};

/* Exported FFI Functions from Rust Static Library */
extern int mt7603_rust_parse_eeprom(const uint8_t *buf, size_t len, struct mt7603_eeprom_data *out);
extern int mt7603_rust_get_mac_init_sequence(struct reg_write_op *ops_buf, size_t max_ops, size_t *out_count);
extern int mt7603_rust_build_own_mac_sequence(const uint8_t *mac, struct reg_write_op *ops_buf, size_t max_ops, size_t *out_count);
extern int mt7603_rust_get_channel_sequence(uint8_t channel, uint8_t bw, struct reg_write_op *ops_buf, size_t max_ops, size_t *out_count);
extern int mt7603_rust_parse_rx_frame(const uint8_t *data, size_t len, int8_t rssi_offset, struct mt7603_rx_info *out);
extern int mt7603_rust_build_txwi(const struct mt7603_tx_params *params, uint8_t *txwi_buf, size_t txwi_len);
extern uint32_t mt7603_rust_map_register_addr(uint32_t addr);
extern int mt7603_rust_build_addr_len_req(uint32_t address, uint32_t dl_len, uint8_t seq, uint8_t *out_buf, size_t max_out_len, size_t *out_written);
extern int mt7603_rust_build_fw_start_req(uint32_t override_flag, uint32_t address, uint8_t seq, uint8_t *out_buf, size_t max_out_len, size_t *out_written);
extern int mt7603_rust_build_restart_dl_req(uint8_t seq, uint8_t *out_buf, size_t max_out_len, size_t *out_written);
extern int mt7603_rust_build_fw_scatter_frame(const uint8_t *chunk, size_t chunk_len, uint8_t *out_buf, size_t max_out_len, size_t *out_written);
extern int mt7603_rust_fw_dl_len(const uint8_t *fw_buf, size_t fw_len, uint32_t *out);
extern int mt7603_rust_verify_firmware(const uint8_t *fw_buf, size_t fw_len);
extern int mt7603_rust_build_probe_req(const uint8_t *ssid, size_t ssid_len, const uint8_t *src_mac, uint8_t *out_buf, size_t max_out_len, size_t *out_written);
extern int mt7603_rust_parse_beacon(const uint8_t *frame_buf, size_t frame_len, struct mt7603_sta_bss_info *out_info);
extern int mt7603_rust_build_beacon(const uint8_t *ssid, size_t ssid_len, const uint8_t *bssid, uint8_t channel, uint8_t *out_buf, size_t max_out_len, size_t *out_written);
extern int mt7603_rust_build_assoc_resp(const uint8_t *sta_mac, const uint8_t *bssid, uint16_t aid, uint16_t status_code, uint8_t *out_buf, size_t max_out_len, size_t *out_written);
extern int mt7603_rust_parse_assoc_req(const uint8_t *frame_buf, size_t frame_len, uint8_t *out_sta_mac, uint16_t *out_cap, uint16_t *out_listen);
extern int mt7603_rust_build_chan_switch_cmd(uint8_t ctrl_ch, uint8_t central_ch, uint8_t bw, uint8_t tx_streams, uint8_t rx_streams, uint8_t seq, uint8_t *out_buf, size_t max_out_len, size_t *out_written);
extern int mt7603_rust_build_tx_power_ctrl_cmd(const uint8_t *eeprom, size_t eeprom_len, uint8_t central_ch, uint8_t seq, uint8_t *out_buf, size_t max_out_len, size_t *out_written);
extern int mt7603_rust_build_ch_privilege_cmd(uint8_t channel, uint8_t seq, uint8_t *out_buf, size_t max_out_len, size_t *out_written);
extern int mt7603_rust_build_radio_on_off_cmd(bool on, uint8_t seq, uint8_t *out_buf, size_t max_out_len, size_t *out_written);
extern int mt7603_rust_build_efuse_buffer_mode_cmd(const uint8_t *eeprom, size_t eeprom_len, uint8_t seq, uint8_t *out_buf, size_t max_out_len, size_t *out_written);

int mt7603_usb_send_cmd(struct usb_device *udev, const uint8_t *frame, size_t frame_len);
int mt7603_efuse_read_block(struct usb_device *udev, uint16_t addr, uint8_t *data);
int mt7603_efuse_read_all(struct usb_device *udev, uint8_t *buf, size_t len);

/* C Internal RegOps Prototypes */
struct usb_device;
struct usb_interface;
struct ieee80211_hw;

int mt7603_usb_read_reg(struct usb_device *udev, uint32_t addr, uint32_t *val);
int mt7603_usb_write_reg(struct usb_device *udev, uint32_t addr, uint32_t val);
int mt7603_usb_cfg_read(struct usb_device *udev, uint32_t addr, uint32_t *val);
int mt7603_usb_cfg_write(struct usb_device *udev, uint32_t addr, uint32_t val);
int mt7603_usb_enable_udma(struct usb_device *udev, bool rx_agg);
int mt7603_execute_reg_ops(struct usb_device *udev, const struct reg_write_op *ops, size_t count);

int mt7603_register_mac80211(struct usb_interface *intf, struct ieee80211_hw **out_hw);
void mt7603_unregister_mac80211(struct ieee80211_hw *hw);

#endif /* _MT7603U_RUST_H_ */
