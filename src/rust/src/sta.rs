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
}
