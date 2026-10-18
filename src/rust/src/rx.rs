//! RX Frame Parsing & Validation
//! Mapped Spec: `specs/modules/rx_tx.spec.md`

use crate::ffi::RxInfo;

pub const RMAC_RXD_MIN_SIZE: usize = 16;

pub fn parse_rx_frame(buf: &[u8], rssi_offset: i8) -> Result<RxInfo, i32> {
    if buf.len() < RMAC_RXD_MIN_SIZE {
        return Err(-22); // -EINVAL
    }

    let mut info = RxInfo::default();

    // MT-MAC RMAC_RXD DW0
    let rx_byte_cnt = u16::from_le_bytes([buf[0], buf[1]]);
    let grp_vld = (buf[3] >> 1) & 0x0F;
    let pkt_type = (buf[3] >> 5) & 0x07;

    // Vendor `parse_rx_packet_type` (cmm_data.c) accepts only
    // RMAC_RX_PKT_TYPE_RX_NORMAL (0x02) and RMAC_RX_PKT_TYPE_RX_DUP_RFB
    // (0x03) as 802.11 data frames. All other types (0x00 TXS, 0x01 TXRXV,
    // 0x04 TMR, 0x05 RETRIEVE, 0x07 EVENT) are not pass-to-upper frames.
    if pkt_type != 0x02 && pkt_type != 0x03 {
        info.pkt_len = 0;
        return Ok(info);
    }

    // Calculate RMAC descriptor header length based on Group Valid flags
    let mut rmac_info_len = RMAC_RXD_MIN_SIZE;
    if (grp_vld & 0x08) != 0 {
        rmac_info_len += 16; // Group 4
    }
    if (grp_vld & 0x01) != 0 {
        rmac_info_len += 16; // Group 1 (RxStatus)
    }
    if (grp_vld & 0x02) != 0 {
        rmac_info_len += 8; // Group 2 (RxTimestamp)
    }
    if (grp_vld & 0x04) != 0 {
        rmac_info_len += 24; // Group 3 (RxVector)
    }

    // DW1 bit 22 is hdr_offset (byte 6 bit 6)
    let hdr_offset = (buf[6] >> 6) & 0x01;
    if hdr_offset == 1 {
        rmac_info_len += 2;
    }

    // DW2 bit 17 is fcs_err (byte 10 bit 1)
    let fcs_err = (buf[10] >> 1) & 0x01;
    info.is_crc_error = fcs_err;

    info.hdr_len = rmac_info_len as u16;

    if buf.len() < rmac_info_len {
        return Err(-22); // -EINVAL
    }

    // RSSI extraction from Group 3 (RxVector). Vendor `ParseRxVPacket`
    // (common/cmm_data.c:373) reads IBRssi0 = RXV1_4TH_CYCLE byte 0, which sits
    // at Group3 + 12. Conversion: dBm = IBRssi0 + rssi_offset (vendor
    // `ConvertToRssi`, common/cmm_sync.c:502; MT7603 lan_gain = 0).
    // IBRssi0 == 0 or absent Group3 => rssi = 0 (unknown, cfg80211 no-signal).
    if (grp_vld & 0x04) != 0 {
        let mut grp3_off = RMAC_RXD_MIN_SIZE;
        if (grp_vld & 0x08) != 0 {
            grp3_off += 16; // Group 4
        }
        if (grp_vld & 0x01) != 0 {
            grp3_off += 16; // Group 1 (RxStatus)
        }
        if (grp_vld & 0x02) != 0 {
            grp3_off += 8; // Group 2 (RxTimestamp)
        }
        let ibrssi0 = buf[grp3_off + 12] as i8;
        if ibrssi0 != 0 {
            info.rssi = ibrssi0.saturating_add(rssi_offset);
        }

        // Extract RXV1_1ST_CYCLE (Group3 + 0) rate/MCS/BW/SGI for Radiotap (`mt_mac.h:611-625`)
        let rxv1_dw0 = u32::from_le_bytes([
            buf[grp3_off],
            buf[grp3_off + 1],
            buf[grp3_off + 2],
            buf[grp3_off + 3],
        ]);
        let tx_rate = (rxv1_dw0 & 0x7f) as u8;
        let tx_mode = ((rxv1_dw0 >> 12) & 0x07) as u8;
        let fr_mode = ((rxv1_dw0 >> 15) & 0x03) as u8;
        let short_gi = ((rxv1_dw0 >> 19) & 0x01) as u8;

        let mut rate_enc: u8 = 0;
        if fr_mode == 1 {
            rate_enc |= 0x40; // BW_40
        }
        if short_gi != 0 {
            rate_enc |= 0x20; // Short GI
        }
        if tx_mode >= 2 {
            // HTMIX (2) or HTGF (3): MCS index in bits [4:0]
            rate_enc |= 0x80 | (tx_rate & 0x0f);
        } else if tx_mode == 1 {
            // OFDM: map TMI rate code to mt7603_rates_2ghz index (4..11)
            let ofdm_idx = match tx_rate & 0x0f {
                11 => 4,  // 6M
                15 => 5,  // 9M
                10 => 6,  // 12M
                14 => 7,  // 18M
                9 => 8,   // 24M
                13 => 9,  // 36M
                8 => 10,  // 48M
                12 => 11, // 54M
                _ => 4,
            };
            rate_enc |= ofdm_idx;
        } else {
            // CCK: map TMI rate code (0..3 LP, 4..7 SP) to index (0..3)
            rate_enc |= tx_rate & 0x03;
        }
        info.rate = rate_enc;
    }

    let total_len = if (rx_byte_cnt as usize) <= buf.len() && rx_byte_cnt > 0 {
        rx_byte_cnt as usize
    } else {
        buf.len()
    };

    let mpdu_len = total_len.saturating_sub(rmac_info_len);
    info.pkt_len = mpdu_len as u16;

    // DW1 bits 8..15: ch_freq (if set)
    info.channel = buf[5];

    // DW1 bit 4 is beacon_mc, bit 5 is beacon_uc
    let beacon_mc = (buf[4] >> 4) & 0x01;
    let beacon_uc = (buf[4] >> 5) & 0x01;

    // Frame classification
    if mpdu_len >= 2 {
        let frame_ctrl = buf[rmac_info_len];
        let frame_type = (frame_ctrl >> 2) & 0x03;
        let frame_subtype = (frame_ctrl >> 4) & 0x0F;

        if (frame_type == 0 && frame_subtype == 8) || beacon_mc != 0 || beacon_uc != 0 {
            info.is_beacon = 1;
        } else if frame_type == 2 {
            info.is_data = 1;
        }
    }

    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_rx_frame() {
        let mut buf = [0u8; 128];
        buf[0] = 128; // rx_byte_cnt = 128
        buf[1] = 0;
        buf[3] = (0x02 << 5) | (0x07 << 1); // pkt_type = RX_NORMAL(0x02), grp_vld = 7
        buf[5] = 6; // Ch 6
        buf[64] = 0x80; // Beacon frame FC at offset 64

        let res = parse_rx_frame(&buf, 0);
        assert!(res.is_ok());
        let info = res.unwrap();
        assert_eq!(info.hdr_len, 64);
        assert_eq!(info.pkt_len, 64);
        assert_eq!(info.channel, 6);
        assert_eq!(info.is_beacon, 1);
        assert_eq!(info.is_crc_error, 0);
    }

    #[test]
    fn test_parse_truncated_rx_frame() {
        let buf = [0u8; 8];
        let res = parse_rx_frame(&buf, 0);
        assert_eq!(res, Err(-22));
    }

    #[test]
    fn test_parse_rx_rssi() {
        // grp_vld = 0b0111 (G1+G2+G3): header = 16+16+8+24 = 64, Group3 offset = 40
        let mut buf = [0u8; 128];
        buf[0] = 128; // rx_byte_cnt = 128
        buf[3] = (0x02 << 5) | (0x07 << 1); // pkt_type = RX_NORMAL, grp_vld = 7
        buf[5] = 6; // Ch 6
        buf[52] = 0x9A; // IBRssi0 at Group3+12 = 52 -> -102
        buf[64] = 0x80; // Beacon FC at offset 64

        let info = parse_rx_frame(&buf, 0).unwrap();
        assert_eq!(info.rssi, -102);
        let info = parse_rx_frame(&buf, 2).unwrap();
        assert_eq!(info.rssi, -100);
    }

    #[test]
    fn test_parse_rx_rssi_unknown() {
        // grp_vld = 0b0011 (G1+G2 only, no Group3): rssi must be 0 (unknown)
        let mut buf = [0u8; 128];
        buf[0] = 128;
        buf[3] = (0x02 << 5) | (0x03 << 1);
        buf[5] = 6;
        buf[40] = 0x80; // Beacon FC at header end 40
        let info = parse_rx_frame(&buf, 0).unwrap();
        assert_eq!(info.rssi, 0);

        // Group3 present but IBRssi0 == 0: rssi must be 0 (unknown)
        let mut buf2 = [0u8; 128];
        buf2[0] = 128;
        buf2[3] = (0x02 << 5) | (0x07 << 1);
        buf2[5] = 6;
        buf2[52] = 0x00; // IBRssi0 = 0
        buf2[64] = 0x80;
        let info = parse_rx_frame(&buf2, 0).unwrap();
        assert_eq!(info.rssi, 0);
    }

    #[test]
    fn test_parse_rx_eapol_data_frame() {
        // grp_vld = 7 -> Header is 16 + 16 (G1) + 8 (G2) + 24 (G3) = 64 bytes
        let mut buf = [0u8; 128];
        buf[0] = 128; // rx_byte_cnt = 128
        buf[1] = 0;
        buf[3] = (0x02 << 5) | (0x07 << 1); // pkt_type = RX_NORMAL(0x02), grp_vld = 7
        buf[5] = 6; // Ch 6
        buf[64] = 0x08; // Data frame FC (Type = 2, Subtype = 0) at offset 64
        buf[65] = 0x00;

        let res = parse_rx_frame(&buf, 0);
        assert!(res.is_ok());
        let info = res.unwrap();
        assert_eq!(info.hdr_len, 64);
        assert_eq!(info.pkt_len, 64);
        assert_eq!(info.channel, 6);
        assert_eq!(info.is_beacon, 0);
        assert_eq!(info.is_data, 1);
        assert_eq!(info.is_crc_error, 0);
    }

    #[test]
    fn test_parse_rx_radiotap_rate() {
        // Group3 offset = 40 when grp_vld = 7
        let mut buf = [0u8; 128];
        buf[0] = 128;
        buf[3] = (0x02 << 5) | (0x07 << 1);
        // RXV1_1ST_CYCLE: TxRate=15 (MCS15), TxMode=2 (HTMIX), FrMode=1 (BW40), HtShortGi=1
        let rxv1_dw0: u32 = 15 | (2 << 12) | (1 << 15) | (1 << 19);
        buf[40..44].copy_from_slice(&rxv1_dw0.to_le_bytes());
        buf[64] = 0x08;

        let info = parse_rx_frame(&buf, 0).unwrap();
        assert_eq!(info.rate, 0x80 | 0x40 | 0x20 | 15);
    }
}
