//! FFI Type Definitions & C ABI Layout Rules
//! Corresponds to `specs/schemas/ffi_types.md`

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RegWriteOp {
    pub addr: u32,
    pub val: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EepromData {
    pub mac_addr: [u8; 6],
    pub tx_power_2g: [u8; 14],
    pub nic_config: u8,
    pub country_code: [u8; 2],
    pub eeprom_version: u16,
    /// Signed RSSI calibration offset from EEPROM 0x46, clamped to [-10,10].
    pub rssi_offset_2g: i8,
    pub is_valid: u8,
}

impl Default for EepromData {
    fn default() -> Self {
        Self {
            mac_addr: [0x00, 0x0C, 0x43, 0x76, 0x03, 0x01],
            tx_power_2g: [0; 14],
            nic_config: 0,
            country_code: *b"US",
            eeprom_version: 0x7603,
            rssi_offset_2g: 0,
            is_valid: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RxInfo {
    pub pkt_len: u16,
    pub hdr_len: u16,
    pub rssi: i8,
    pub channel: u8,
    pub rate: u8,
    pub is_beacon: u8,
    pub is_data: u8,
    pub is_crc_error: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxParams {
    /// mac80211 rate index / TMI mcs (legacy, see rate_mode/rate_mcs).
    pub rate_idx: u8,
    /// Used as TXD wlan_idx (WCID). 0 for unassociated broadcast.
    pub pid: u8,
    /// TXD q_idx (Q_IDX_AC4 = 0x04 for mgmt frames on MT_MAC).
    pub queue: u8,
    /// 802.11 header length (24 for probe request).
    pub hdr_len: u8,
    /// 802.11 frame control type (0=mgmt, 1=ctl, 2=data).
    pub frm_type: u8,
    /// 802.11 frame control subtype (probe request = 4).
    pub sub_type: u8,
    /// 1 if no ACK required (probe request).
    pub no_ack: u8,
    /// 1 if broadcast/multicast (probe request Addr1 = ff:ff:ff:ff:ff:ff).
    pub is_bm: u8,
    /// Rate PHY mode: MODE_CCK(0) / MODE_OFDM(1) / MODE_HTMIX(2) / MODE_HTGF(3).
    pub rate_mode: u8,
    /// Rate MCS index (CCK: 0=1M, 1=2M, 2=5.5M, 3=11M).
    pub rate_mcs: u8,
    /// Preamble: SHORT_PREAMBLE(0) / LONG_PREAMBLE(1).
    pub preamble: u8,
    /// Bandwidth: BW_20(0) / BW_40(1).
    pub bw: u8,
    /// 802.11 frame length in bytes (excluding TXD).
    pub pkt_len: u16,
    /// 1 if the frame must be 802.11-protected (CCMP-encrypted by hardware).
    /// Maps to TMAC_TXD_1 `protect_frm` (bit 23). Set from
    /// `IEEE80211_TX_CTL_PROTECTED` by the C TX path.
    pub protect_frm: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StaBssInfo {
    pub bssid: [u8; 6],
    pub ssid: [u8; 32],
    pub ssid_len: u8,
    pub channel: u8,
    pub rssi: i8,
    pub capability: u16,
}
