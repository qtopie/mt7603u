//! Station (STA) Mode Operations
//! Mapped Spec: `specs/modules/sta.spec.md`

use crate::ffi::StaBssInfo;

pub fn build_probe_request(
    ssid: &[u8],
    src_mac: &[u8; 6],
    out_buf: &mut [u8],
) -> Result<usize, i32> {
    // 802.11 Management Header (24 bytes) + SSID IE (2 + ssid.len()) + Supported Rates IE (10 bytes)
    let total_len = 24 + 2 + ssid.len() + 10;
    if out_buf.len() < total_len {
        return Err(-28); // -ENOSPC
    }

    out_buf[..total_len].fill(0);

    // Frame Control: 0x0040 (Probe Request, Type 0 Subtype 4)
    out_buf[0] = 0x40;
    out_buf[1] = 0x00;

    // Address 1: Destination (Broadcast FF:FF:FF:FF:FF:FF)
    out_buf[4..10].fill(0xFF);

    // Address 2: Source MAC
    out_buf[10..16].copy_from_slice(src_mac);

    // Address 3: BSSID (Broadcast FF:FF:FF:FF:FF:FF)
    out_buf[16..22].fill(0xFF);

    // Sequence Control (Offset 22..24)
    out_buf[22] = 0x00;
    out_buf[23] = 0x00;

    // Element 0: SSID IE
    let mut pos = 24;
    out_buf[pos] = 0; // Element ID: SSID
    out_buf[pos + 1] = ssid.len() as u8;
    crate::util::copy_bytes(&mut out_buf[pos + 2..pos + 2 + ssid.len()], ssid);
    pos += 2 + ssid.len();

    // Element 1: Supported Rates IE
    out_buf[pos] = 1; // Element ID: Supported Rates
    out_buf[pos + 1] = 8; // Length: 8 rates
    crate::util::copy_bytes(
        &mut out_buf[pos + 2..pos + 10],
        &[0x82, 0x84, 0x8b, 0x96, 0x24, 0x30, 0x48, 0x6c],
    );

    Ok(total_len)
}

pub fn parse_beacon(frame_buf: &[u8]) -> Result<StaBssInfo, i32> {
    // Minimum 802.11 Mgmt Header (24 bytes) + Beacon fixed fields (12 bytes)
    if frame_buf.len() < 36 {
        return Err(-22); // -EINVAL
    }

    let mut info = StaBssInfo::default();

    // Extract BSSID (Address 3 at offset 16)
    info.bssid.copy_from_slice(&frame_buf[16..22]);

    // Capability Information (Offset 34..36)
    info.capability = u16::from_le_bytes([frame_buf[34], frame_buf[35]]);

    // Parse Information Elements (starting at offset 36)
    let mut pos = 36;
    while pos + 2 <= frame_buf.len() {
        let ie_id = frame_buf[pos];
        let ie_len = frame_buf[pos + 1] as usize;

        if ie_len == 0 || pos + 2 + ie_len > frame_buf.len() {
            break;
        }

        let ie_payload = &frame_buf[pos + 2..pos + 2 + ie_len];

        if ie_id == 0 && info.ssid_len == 0 {
            // SSID IE
            let copy_len = usize::min(ie_payload.len(), 32);
            crate::util::copy_bytes(&mut info.ssid[..copy_len], &ie_payload[..copy_len]);
            info.ssid_len = copy_len as u8;
        } else if ie_id == 3 && ie_len >= 1 {
            // DS Parameter Set (Channel)
            info.channel = ie_payload[0];
        }

        pos += 2 + ie_len;
    }

    Ok(info)
}

pub const WTBL1_BASE: u32 = 0x0002_8000;
pub const WTBL1_ENTRY_SIZE: u32 = 0x14; // 20 bytes
pub const WTBL1OR: u32 = 0x0002_A300;

pub const WTBL3_KEY_BASE: u32 = 0x0004_2000; // WTBL key SRAM (host remap) base, stride 64 B/WCID
pub const WTBL3_ENTRY_SIZE: u32 = 0x40; // 64 bytes per WCID key entry

/// Compute the WTBL2/3/4 PSE page Fragment-ID / Entry-ID for a given WCID.
/// Ported verbatim from vendor `mt_wtbl_get_entry234` (`mac/mt_mac.c:1789`)
/// and the base/fid arithmetic in `mt_wtbl_init` (`mac/mt_mac.c:1819-1843`).
/// The cross-links written into WTBL1 DW3/DW4 let the MAC locate the per-WCID
/// key (WTBL3) when encrypting/decrypting; without correct FID/EID the key is
/// never found and the frame is dropped or sent in clear.
fn wtbl_entry234(wcid: u32) -> (u32, u32, u32, u32, u32, u32) {
    const PAGE_SIZE: u32 = 128; // MT_PSE_PAGE_SIZE
    let ecnt2: u32 = PAGE_SIZE / 64; // WTBL2 entry size = 64 -> 2 per page
    let ecnt3: u32 = PAGE_SIZE / 64; // WTBL3 entry size = 64 -> 2 per page
    let ecnt4: u32 = PAGE_SIZE / 32; // WTBL4 entry size = 32 -> 4 per page
    let page_cnt2: u32 = 128_u32.div_ceil(ecnt2);
    let page_cnt3: u32 = 128_u32.div_ceil(ecnt3);
    let base_fid2: u32 = 0;
    let base_fid3: u32 = base_fid2 + page_cnt2;
    let base_fid4: u32 = base_fid3 + page_cnt3;

    let page_off2 = wcid / ecnt2;
    let elem_off2 = wcid % ecnt2;
    let page_off3 = wcid / ecnt3;
    let elem_off3 = wcid % ecnt3;
    let page_off4 = wcid / ecnt4;
    let elem_off4 = wcid % ecnt4;

    let fid2 = base_fid2 + page_off2;
    let eid2 = elem_off2;
    let fid3 = base_fid3 + page_off3;
    let eid3 = elem_off3 * 2; // vendor: idx==2 uses element_offset*2
    let fid4 = base_fid4 + page_off4;
    let eid4 = elem_off4;
    (fid2, eid2, fid3, eid3, fid4, eid4)
}

/// WTBL1 DW3/DW4 cross-link words for a WCID (little-endian field packing).
fn wtbl_dw3_dw4(wcid: u32) -> (u32, u32) {
    let (fid2, eid2, fid3, eid3, fid4, eid4) = wtbl_entry234(wcid);
    // DW3: wtbl2_fid[10:0] | wtbl2_eid[15:11] | wtbl4_fid[26:16]
    let dw3 = fid2 | (eid2 << 11) | (fid4 << 16);
    // DW4: wtbl3_fid[10:0] | wtbl3_eid[16:11] | wtbl4_eid[22:17]
    let dw4 = fid3 | (eid3 << 11) | (eid4 << 17);
    (dw3, dw4)
}

pub fn build_wtbl_sta_sequence(
    bssid: &[u8; 6],
    out_ops: &mut [crate::ffi::RegWriteOp],
) -> Result<usize, i32> {
    if out_ops.len() < 12 {
        return Err(-28); // -ENOSPC
    }

    // Entry 0 (Broadcast/Multicast default entry at 0x28000):
    // DW0: rv=1 (bit 28), rc_a2=1 (bit 29), rc_a1=1 (bit 22), muar_idx=0x0E (bits 16..21), addr_4=0xFF, addr_5=0xFF
    let dw0_mcast = (1 << 29) | (1 << 28) | (1 << 22) | (0x0E << 16) | (0xFF << 8) | 0xFF;
    let dw1_mcast = 0xFFFF_FFFF;
    let dw2_mcast = 0x0000_0000; // WTBL_CIPHER_NONE, adm=0 (key installed later)
    let (dw3_mcast, dw4_mcast) = wtbl_dw3_dw4(0);

    // Entry 1 (Associated AP unicast entry at 0x28014):
    // DW0: rv=1 (bit 28), rc_a2=1 (bit 29), addr_4=bssid[4], addr_5=bssid[5]
    let dw0_ap = (1 << 29) | (1 << 28) | ((bssid[5] as u32) << 8) | (bssid[4] as u32);
    let dw1_ap = u32::from_le_bytes([bssid[0], bssid[1], bssid[2], bssid[3]]);
    // DW2: qos=1 (bit 27), ht=1 (bit 28), baf_en=1 (bit 20), dyn_bw=1 (bit 21).
    // adm/cipher_suit are written by set_key once the PTK is installed.
    let dw2_ap = (1 << 28) | (1 << 27) | (1 << 21) | (1 << 20);
    let (dw3_ap, dw4_ap) = wtbl_dw3_dw4(1);

    out_ops[0] = crate::ffi::RegWriteOp {
        addr: WTBL1_BASE,
        val: dw0_mcast,
    };
    out_ops[1] = crate::ffi::RegWriteOp {
        addr: WTBL1_BASE + 0x04,
        val: dw1_mcast,
    };
    out_ops[2] = crate::ffi::RegWriteOp {
        addr: WTBL1_BASE + 0x08,
        val: dw2_mcast,
    };
    out_ops[3] = crate::ffi::RegWriteOp {
        addr: WTBL1_BASE + 0x0C,
        val: dw3_mcast,
    };
    out_ops[4] = crate::ffi::RegWriteOp {
        addr: WTBL1_BASE + 0x10,
        val: dw4_mcast,
    };

    out_ops[5] = crate::ffi::RegWriteOp {
        addr: WTBL1_BASE + WTBL1_ENTRY_SIZE,
        val: dw0_ap,
    };
    out_ops[6] = crate::ffi::RegWriteOp {
        addr: WTBL1_BASE + WTBL1_ENTRY_SIZE + 0x04,
        val: dw1_ap,
    };
    out_ops[7] = crate::ffi::RegWriteOp {
        addr: WTBL1_BASE + WTBL1_ENTRY_SIZE + 0x08,
        val: dw2_ap,
    };
    out_ops[8] = crate::ffi::RegWriteOp {
        addr: WTBL1_BASE + WTBL1_ENTRY_SIZE + 0x0C,
        val: dw3_ap,
    };
    out_ops[9] = crate::ffi::RegWriteOp {
        addr: WTBL1_BASE + WTBL1_ENTRY_SIZE + 0x10,
        val: dw4_ap,
    };

    // Trigger WTBL1 hardware table flush (PSM_W_FLAG bit 31)
    out_ops[10] = crate::ffi::RegWriteOp {
        addr: WTBL1OR,
        val: 0x8000_0000,
    };
    // Clear PSM_W_FLAG after trigger
    out_ops[11] = crate::ffi::RegWriteOp {
        addr: WTBL1OR,
        val: 0x0000_0000,
    };

    Ok(12)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_probe_request() {
        let ssid = b"WiFi-Test";
        let src_mac = [0x00, 0x0C, 0x43, 0x76, 0x03, 0x01];
        let mut out_buf = [0u8; 128];

        let res = build_probe_request(ssid, &src_mac, &mut out_buf);
        assert!(res.is_ok());
        let len = res.unwrap();
        assert!(len >= 36);

        // Assert Frame Control (0x0040)
        assert_eq!(out_buf[0], 0x40);
        assert_eq!(out_buf[1], 0x00);
        // Assert Destination Broadcast
        assert_eq!(&out_buf[4..10], &[0xFF; 6]);
        // Assert Source MAC
        assert_eq!(&out_buf[10..16], &src_mac);
        // Assert SSID IE
        assert_eq!(out_buf[24], 0); // Element ID
        assert_eq!(out_buf[25], 9); // SSID Length
        assert_eq!(&out_buf[26..35], b"WiFi-Test");
    }

    #[test]
    fn test_parse_beacon_frame() {
        let mut frame = [0u8; 64];
        // Address 3 (BSSID) at offset 16
        frame[16..22].copy_from_slice(&[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        // Capability at offset 34
        frame[34] = 0x01;
        frame[35] = 0x00;

        // IE 0: SSID "Home-AP"
        frame[36] = 0; // IE ID SSID
        frame[37] = 7; // Len 7
        frame[38..45].copy_from_slice(b"Home-AP");

        // IE 3: Channel 6
        frame[45] = 3; // IE ID Channel
        frame[46] = 1;
        frame[47] = 6;

        let res = parse_beacon(&frame);
        assert!(res.is_ok());
        let info = res.unwrap();
        assert_eq!(info.bssid, [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        assert_eq!(&info.ssid[..7], b"Home-AP");
        assert_eq!(info.ssid_len, 7);
        assert_eq!(info.channel, 6);
    }

    #[test]
    fn test_wtbl_entry234_crosslinks() {
        // Lock in vendor mt_wtbl_get_entry234 / mt_wtbl_init derived FID/EID.
        // WCID 0
        assert_eq!(wtbl_entry234(0), (0, 0, 64, 0, 128, 0));
        assert_eq!(wtbl_dw3_dw4(0), (0x0080_0000, 0x0000_0040));
        // WCID 1
        assert_eq!(wtbl_entry234(1), (0, 1, 64, 2, 128, 1));
        assert_eq!(wtbl_dw3_dw4(1), (0x0080_0800, 0x0002_1040));
        // Spot-check a higher WCID to confirm page arithmetic.
        assert_eq!(wtbl_entry234(2), (1, 0, 65, 0, 128, 2));
        assert_eq!(wtbl_dw3_dw4(2), (0x0080_0001, 0x0004_0041));
        assert_eq!(wtbl_entry234(3), (1, 1, 65, 2, 128, 3));
    }

    #[test]
    fn test_build_wtbl_sta_sequence() {
        let bssid = [0xFC, 0x34, 0x97, 0x19, 0x0E, 0x01];
        let mut ops = [crate::ffi::RegWriteOp::default(); 16];

        let res = build_wtbl_sta_sequence(&bssid, &mut ops);
        assert!(res.is_ok());
        let written = res.unwrap();
        assert_eq!(written, 12);

        // Entry 0 Broadcast (0x28000) — cross-links via wtbl_dw3_dw4(0)
        assert_eq!(ops[0].addr, 0x0002_8000);
        assert_eq!(
            ops[0].val,
            (1 << 29) | (1 << 28) | (1 << 22) | (0x0E << 16) | 0xFFFF
        );
        assert_eq!(ops[1].addr, 0x0002_8004);
        assert_eq!(ops[1].val, 0xFFFF_FFFF);
        assert_eq!(ops[2].addr, 0x0002_8008);
        assert_eq!(ops[2].val, 0x0000_0000);
        assert_eq!(ops[3].addr, 0x0002_800C);
        assert_eq!(ops[3].val, 0x0080_0000); // wtbl_dw3_dw4(0).0
        assert_eq!(ops[4].addr, 0x0002_8010);
        assert_eq!(ops[4].val, 0x0000_0040); // wtbl_dw3_dw4(0).1

        // Entry 1 AP BSSID (0x28014) — cross-links via wtbl_dw3_dw4(1)
        assert_eq!(ops[5].addr, 0x0002_8014);
        assert_eq!(ops[5].val, (1 << 29) | (1 << 28) | (0x01 << 8) | 0x0E);
        assert_eq!(ops[6].addr, 0x0002_8018);
        assert_eq!(ops[6].val, 0x1997_34FC);
        assert_eq!(ops[7].addr, 0x0002_801C);
        assert_eq!(ops[7].val, 0x1830_0000); // qos|ht|baf_en|dyn_bw
        assert_eq!(ops[8].addr, 0x0002_8020);
        assert_eq!(ops[8].val, 0x0080_0800); // wtbl_dw3_dw4(1).0
        assert_eq!(ops[9].addr, 0x0002_8024);
        assert_eq!(ops[9].val, 0x0002_1040); // wtbl_dw3_dw4(1).1

        // Flush WTBL1OR (0x2A300)
        assert_eq!(ops[10].addr, 0x0002_A300);
        assert_eq!(ops[10].val, 0x8000_0000);
        assert_eq!(ops[11].addr, 0x0002_A300);
        assert_eq!(ops[11].val, 0x0000_0000);
    }
}
