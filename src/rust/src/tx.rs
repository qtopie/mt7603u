//! TX Frame Transcoding & 32-byte Long TXD (TMAC_TXD_L) Construction
//! Mapped Spec: `specs/modules/rx_tx.spec.md` §2

use crate::ffi::TxParams;

/// MT7603 uses the long TXD format: `TXWISize = sizeof(TMAC_TXD_L) = 32`
/// (`chips/mt7603.c:1199`; `mac_info.Length = SrcBufLen - 32`, `cmm_data.c:1969-1972`).
pub const TXWI_SIZE: usize = 32;

// Vendor constants (`include/mac/mac_mt/mt_mac.h`)
const P_IDX_LMAC: u32 = 0;
const TMI_HDR_FT_NOR_80211: u32 = 0x2;
const TMI_FT_LONG: u32 = 0x1;
const MT_TX_SHORT_RETRY: u32 = 0x0f;
const TMI_DAS_FROM_MPDU: u32 = 0;
const TMI_BSN_CFG_BY_SW: u32 = 0x1;
const TMI_PM_BIT_CFG_BY_HW: u32 = 0x0;
const TMI_HDR_PAD_MODE_TAIL: u32 = 0;

// Rate PHY modes (`rtmp_comm.h:301-305`)
const MODE_CCK: u32 = 0;
const MODE_OFDM: u32 = 1;
const MODE_HTMIX: u32 = 2;
const MODE_HTGF: u32 = 3;
#[cfg(test)]
const SHORT_PREAMBLE: u32 = 0;
const LONG_PREAMBLE: u32 = 1;
const TMI_TX_RATE_BIT_MODE: u32 = 6;
const TMI_TX_RATE_BIT_NSS: u32 = 9;
const TMI_TX_RATE_BIT_STBC: u32 = 11;
const TMI_TX_RATE_MASK_NSS: u32 = 0x3;

/// `tmi_rate_map_cck_lp` / `tmi_rate_map_cck_sp` / `tmi_rate_map_ofdm`
/// (`mac/mt_mac.c:686-712`).
fn tx_rate_to_tmi(rate_mode: u32, mcs: u32, stbc: u32, nss: u32, preamble: u32) -> u32 {
    match rate_mode {
        MODE_CCK => {
            let mcs_id = if preamble == LONG_PREAMBLE {
                [0, 1, 2, 3][mcs as usize]
            } else {
                [5, 5, 6, 7][mcs as usize]
            };
            (MODE_CCK << TMI_TX_RATE_BIT_MODE) | mcs_id
        }
        MODE_OFDM => {
            let mcs_id = [11, 15, 10, 14, 9, 13, 8, 12][mcs as usize];
            (MODE_OFDM << TMI_TX_RATE_BIT_MODE) | mcs_id
        }
        MODE_HTMIX | MODE_HTGF => {
            (stbc << TMI_TX_RATE_BIT_STBC)
                | (((nss - 1) & TMI_TX_RATE_MASK_NSS) << TMI_TX_RATE_BIT_NSS)
                | (rate_mode << TMI_TX_RATE_BIT_MODE)
                | mcs
        }
        _ => 0,
    }
}

pub fn build_txwi(params: &TxParams, out_buf: &mut [u8]) -> Result<usize, i32> {
    if out_buf.len() < TXWI_SIZE {
        return Err(-28); // -ENOSPC
    }
    if params.pkt_len as u32 + TXWI_SIZE as u32 > 0xffff {
        return Err(-22); // -EINVAL: tx_byte_cnt is 16 bits
    }

    out_buf[..TXWI_SIZE].fill(0);

    let hdr_len = params.hdr_len as u32;
    if hdr_len & 1 != 0 {
        return Err(-22); // -EINVAL: TMI_HDR_INFO_2_VAL requires even hdr_len
    }

    let hdr_pad_len = (4 - (hdr_len & 0x03)) & 0x03;
    let hdr_pad = (TMI_HDR_PAD_MODE_TAIL << 2) | (hdr_pad_len & 0x03);

    // ---- DWORD 0 ----
    // [15:0] tx_byte_cnt = txd_size + Length + hdr_pad_len (= 32 + pkt_len + pad, vendor mt_mac.c:1139)
    // [16:22] eth_type_offset (0), [23] ip_sum (0), [24] ut_sum (0),
    // [25] UNxV (0), [26] UTxB (0), [30:27] q_idx, [31] p_idx (P_IDX_LMAC)
    let dw0 = ((params.pkt_len as u32) + hdr_pad_len + TXWI_SIZE as u32)
        | ((params.queue as u32 & 0x0f) << 27)
        | (P_IDX_LMAC << 31);

    // ---- DWORD 1 ----
    // [7:0] wlan_idx, [12:8] hdr_info = hdr_len>>1, [14:13] hdr_format = NOR_80211(2),
    // [15] ft = LONG(1), [18:16] hdr_pad, [19] no_ack, [22:20] tid (0),
    // [23] protect_frm (0), [31:26] own_mac (0)
    let dw1 = (params.pid as u32 & 0xff)
        | (((hdr_len >> 1) & 0x1f) << 8)
        | (TMI_HDR_FT_NOR_80211 << 13)
        | (TMI_FT_LONG << 15)
        | ((hdr_pad & 0x07) << 16)
        | ((params.no_ack as u32 & 0x1) << 19);

    // ---- DWORD 2 ----
    // [3:0] sub_type, [5:4] frm_type, [6..9] ndp/ndpa/sounding/rts (0), [10] bc_mc_pkt,
    // [11] bip (0), [12] duration (0), [13] htc_vld (0), [15:14] frag (0),
    // [23:16] max_tx_time (0), [28:24] pwr_offset (0), [29] ba_disable (1),
    // [30] timing_measure (0), [31] fix_rate (1)
    let dw2 = (params.sub_type as u32 & 0x0f)
        | ((params.frm_type as u32 & 0x03) << 4)
        | ((params.is_bm as u32 & 0x1) << 10)
        | (1 << 29)
        | (1 << 31);

    // ---- DWORD 3 ----
    // [10:6] tx_cnt (0), [15:11] remain_tx_cnt = MT_TX_SHORT_RETRY(0x0f),
    // [27:16] sn (0), [30] pn_vld (0), [31] sn_vld (0)
    let dw3 = (MT_TX_SHORT_RETRY & 0x1f) << 11;

    // ---- DWORD 4 ---- pn_low = 0

    // ---- DWORD 5 ----
    // [7:0] pid (0), [8] tx_status_fmt (0), [9] tx_status_2_mcu (0),
    // [10] tx_status_2_host (0), [11] da_select = TMI_DAS_FROM_MPDU(0),
    // [12] bar_sn_ctrl = TMI_BSN_CFG_BY_SW(1), [13] pwr_mgmt = TMI_PM_BIT_CFG_BY_HW(0),
    // [31:16] pn_high (0)
    let dw5 = (TMI_DAS_FROM_MPDU << 11) | (TMI_BSN_CFG_BY_SW << 12) | (TMI_PM_BIT_CFG_BY_HW << 13);

    // ---- DWORD 6 ----
    // [0] fix_rate_mode = TMI_FIX_RATE_BY_TXD(0), [1] rsv (0), [7:2] ant_id (0),
    // [10:8] bw = (1<<2)|BW, [11] spe_en (1), [14:12] ant_pri (0), [15] dyn_bw (0),
    // [16] ETxBF (0), [17] ITxBF (0), [29:18] tx_rate, [30] ldpc (0), [31] gi (0)
    let tx_rate = tx_rate_to_tmi(
        params.rate_mode as u32,
        params.rate_mcs as u32,
        0,
        1,
        params.preamble as u32,
    );
    let bw_field = (1 << 2) | (params.bw as u32 & 0x3);
    let dw6 = (bw_field << 8) | (1 << 11) | ((tx_rate & 0x0fff) << 18);

    // ---- DWORD 7 ---- sch_tx_time = 0, sw_field = 0

    out_buf[0..4].copy_from_slice(&dw0.to_le_bytes());
    out_buf[4..8].copy_from_slice(&dw1.to_le_bytes());
    out_buf[8..12].copy_from_slice(&dw2.to_le_bytes());
    out_buf[12..16].copy_from_slice(&dw3.to_le_bytes());
    out_buf[20..24].copy_from_slice(&dw5.to_le_bytes());
    out_buf[24..28].copy_from_slice(&dw6.to_le_bytes());

    Ok(TXWI_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe_req_params(pkt_len: u16) -> TxParams {
        TxParams {
            rate_idx: 0,
            pid: 0,      // wlan_idx = 0 (unassociated broadcast)
            queue: 0x04, // Q_IDX_AC4
            hdr_len: 24,
            frm_type: 0, // mgmt
            sub_type: 4, // probe request
            no_ack: 1,
            is_bm: 1,
            rate_mode: 0, // MODE_CCK
            rate_mcs: 0,  // CCK 1M
            preamble: 1,  // LONG_PREAMBLE
            bw: 0,        // BW_20
            pkt_len,
        }
    }

    #[test]
    fn test_build_txwi() {
        let params = probe_req_params(211);
        let mut buf = [0u8; 32];
        let res = build_txwi(&params, &mut buf);
        assert_eq!(res, Ok(32));

        // DW0: tx_byte_cnt = 32 + 211 = 243, q_idx = 0x04<<27, p_idx = 0
        let dw0 = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        assert_eq!(dw0 & 0xffff, 243, "tx_byte_cnt");
        assert_eq!((dw0 >> 27) & 0xf, 0x04, "q_idx");
        assert_eq!((dw0 >> 31) & 0x1, 0, "p_idx");

        // DW1: wlan_idx=0, hdr_info=12, hdr_format=2, ft=1, no_ack=1
        let dw1 = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        assert_eq!(dw1 & 0xff, 0, "wlan_idx");
        assert_eq!((dw1 >> 8) & 0x1f, 12, "hdr_info = 24>>1");
        assert_eq!((dw1 >> 13) & 0x3, 2, "hdr_format = NOR_80211");
        assert_eq!((dw1 >> 15) & 0x1, 1, "ft = LONG");
        assert_eq!((dw1 >> 19) & 0x1, 1, "no_ack");

        // DW2: sub_type=4, frm_type=0, bc_mc_pkt=1, ba_disable=1, fix_rate=1
        let dw2 = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
        assert_eq!(dw2 & 0xf, 4, "sub_type");
        assert_eq!((dw2 >> 4) & 0x3, 0, "frm_type");
        assert_eq!((dw2 >> 10) & 0x1, 1, "bc_mc_pkt");
        assert_eq!((dw2 >> 29) & 0x1, 1, "ba_disable");
        assert_eq!((dw2 >> 31) & 0x1, 1, "fix_rate");

        // DW3: remain_tx_cnt = 0x0f<<11
        let dw3 = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
        assert_eq!((dw3 >> 11) & 0x1f, 0x0f, "remain_tx_cnt");

        // DW5: bar_sn_ctrl = 1<<12
        let dw5 = u32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]);
        assert_eq!((dw5 >> 12) & 0x1, 1, "bar_sn_ctrl");

        // DW6: bw=(1<<2)|BW_20=0x4<<8, spe_en=1<<11, tx_rate=0<<18
        let dw6 = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
        assert_eq!((dw6 >> 8) & 0x7, 0x4, "bw");
        assert_eq!((dw6 >> 11) & 0x1, 1, "spe_en");
        assert_eq!((dw6 >> 18) & 0xfff, 0, "tx_rate = CCK 1M LP");

        // DW7: zero
        assert_eq!(&buf[28..32], &[0u8; 4]);

        // DW4: zero
        assert_eq!(&buf[16..20], &[0u8; 4]);
    }

    #[test]
    fn test_build_txwi_ofdm_rate() {
        let params = TxParams {
            rate_mode: 1, // MODE_OFDM
            rate_mcs: 4,  // 24M
            ..probe_req_params(211)
        };
        let mut buf = [0u8; 32];
        assert_eq!(build_txwi(&params, &mut buf), Ok(32));
        let dw6 = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
        // tmi ofdm[4] = TMI_TX_RATE_OFDM_24M = 9; (1<<6)|9 = 73
        assert_eq!((dw6 >> 18) & 0xfff, 73, "OFDM 24M");
    }

    #[test]
    fn test_build_txwi_enospc() {
        let params = probe_req_params(10);
        let mut buf = [0u8; 16];
        assert_eq!(build_txwi(&params, &mut buf), Err(-28));
    }

    #[test]
    fn test_build_txwi_odd_hdr_len() {
        let params = TxParams {
            hdr_len: 23,
            ..probe_req_params(10)
        };
        let mut buf = [0u8; 32];
        assert_eq!(build_txwi(&params, &mut buf), Err(-22));
    }

    #[test]
    fn test_build_txwi_qos_data_pad() {
        let params = TxParams {
            rate_idx: 0,
            pid: 1,      // wlan_idx = 1
            queue: 0x00, // Q_IDX_AC0
            hdr_len: 26, // QoS data header
            frm_type: 2, // data
            sub_type: 8, // QoS data
            no_ack: 0,
            is_bm: 0,
            rate_mode: 0, // MODE_CCK
            rate_mcs: 0,  // CCK 1M
            preamble: 1,  // LONG_PREAMBLE
            bw: 0,        // BW_20
            pkt_len: 155,
        };
        let mut buf = [0u8; 32];
        assert_eq!(build_txwi(&params, &mut buf), Ok(32));

        // DW0: tx_byte_cnt = 32 + 2(pad) + 155 = 189
        let dw0 = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        assert_eq!(dw0 & 0xffff, 189, "tx_byte_cnt");

        // DW1: wlan_idx=1, hdr_info=13 (26>>1), hdr_pad=2
        let dw1 = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        assert_eq!(dw1 & 0xff, 1, "wlan_idx");
        assert_eq!((dw1 >> 8) & 0x1f, 13, "hdr_info");
        assert_eq!((dw1 >> 16) & 0x7, 2, "hdr_pad");

        // DW2: sub_type=8, frm_type=2, bc_mc_pkt=0, ba_disable=1, fix_rate=1
        let dw2 = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
        assert_eq!(dw2 & 0xf, 8, "sub_type");
        assert_eq!((dw2 >> 4) & 0x3, 2, "frm_type");
        assert_eq!((dw2 >> 10) & 0x1, 0, "bc_mc_pkt");
        assert_eq!((dw2 >> 29) & 0x1, 1, "ba_disable");
        assert_eq!((dw2 >> 31) & 0x1, 1, "fix_rate");
    }

    #[test]
    fn test_tx_rate_to_tmi_mapping() {
        // CCK 1M/2M/5.5M/11M long preamble
        assert_eq!(tx_rate_to_tmi(MODE_CCK, 0, 0, 1, LONG_PREAMBLE), 0);
        assert_eq!(tx_rate_to_tmi(MODE_CCK, 1, 0, 1, LONG_PREAMBLE), 1);
        assert_eq!(tx_rate_to_tmi(MODE_CCK, 2, 0, 1, LONG_PREAMBLE), 2);
        assert_eq!(tx_rate_to_tmi(MODE_CCK, 3, 0, 1, LONG_PREAMBLE), 3);
        // CCK short preamble: 1M=5
        assert_eq!(tx_rate_to_tmi(MODE_CCK, 0, 0, 1, SHORT_PREAMBLE), 5);
        // OFDM 6M = 11
        assert_eq!(
            tx_rate_to_tmi(MODE_OFDM, 0, 0, 1, LONG_PREAMBLE),
            (1 << 6) | 11
        );
    }
}
