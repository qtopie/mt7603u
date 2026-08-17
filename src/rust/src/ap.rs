//! Access Point (AP) Mode Operations
//! Mapped Spec: `specs/modules/ap.spec.md`

pub fn build_beacon(
    ssid: &[u8],
    bssid: &[u8; 6],
    channel: u8,
    out_buf: &mut [u8],
) -> Result<usize, i32> {
    // Header (24 bytes) + Fixed Fields (12 bytes: Timestamp 8B, Beacon Interval 2B, Capability 2B)
    // + SSID IE (2 + ssid.len()) + Supported Rates IE (10B) + DS Channel IE (3B)
    let total_len = 24 + 12 + 2 + ssid.len() + 10 + 3;
    if out_buf.len() < total_len {
        return Err(-28); // -ENOSPC
    }

    out_buf[..total_len].fill(0);

    // Frame Control: 0x0080 (Beacon, Type 0 Subtype 8)
    out_buf[0] = 0x80;
    out_buf[1] = 0x00;

    // Address 1: Destination Broadcast (FF:FF:FF:FF:FF:FF)
    out_buf[4..10].fill(0xFF);

    // Address 2: Source MAC (BSSID)
    out_buf[10..16].copy_from_slice(bssid);

    // Address 3: BSSID
    out_buf[16..22].copy_from_slice(bssid);

    // Fixed Fields (Offset 24..36)
    // Timestamp: 8 bytes (0)
    // Beacon Interval: 100 TU (0x0064)
    out_buf[32] = 0x64;
    out_buf[33] = 0x00;
    // Capability Information: ESS (0x0001)
    out_buf[34] = 0x01;
    out_buf[35] = 0x00;

    // Element 0: SSID IE
    let mut pos = 36;
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
    pos += 10;

    // Element 3: DS Parameter Set (Channel)
    out_buf[pos] = 3; // Element ID: DS Parameter Set
    out_buf[pos + 1] = 1;
    out_buf[pos + 2] = channel;

    Ok(total_len)
}

pub fn build_assoc_resp(
    sta_mac: &[u8; 6],
    bssid: &[u8; 6],
    aid: u16,
    status_code: u16,
    out_buf: &mut [u8],
) -> Result<usize, i32> {
    // Header (24 bytes) + Fixed Fields (6 bytes: Capability 2B, Status Code 2B, AID 2B) + Supported Rates IE (10B)
    let total_len = 24 + 6 + 10;
    if out_buf.len() < total_len {
        return Err(-28); // -ENOSPC
    }

    out_buf[..total_len].fill(0);

    // Frame Control: 0x0010 (Association Response, Type 0 Subtype 1)
    out_buf[0] = 0x10;
    out_buf[1] = 0x00;

    // Address 1: Destination STA MAC
    out_buf[4..10].copy_from_slice(sta_mac);

    // Address 2: Source BSSID
    out_buf[10..16].copy_from_slice(bssid);

    // Address 3: BSSID
    out_buf[16..22].copy_from_slice(bssid);

    // Fixed Fields (Offset 24..30)
    // Capability: ESS (0x0001)
    out_buf[24] = 0x01;
    out_buf[25] = 0x00;

    // Status Code (0 = Success)
    let status_bytes = status_code.to_le_bytes();
    out_buf[26] = status_bytes[0];
    out_buf[27] = status_bytes[1];

    // Association ID (AID, e.g. 0xC001 for AID 1)
    let aid_val = 0xC000 | (aid & 0x3FFF);
    let aid_bytes = aid_val.to_le_bytes();
    out_buf[28] = aid_bytes[0];
    out_buf[29] = aid_bytes[1];

    // Element 1: Supported Rates IE
    let pos = 30;
    out_buf[pos] = 1;
    out_buf[pos + 1] = 8;
    out_buf[pos + 2..pos + 10].copy_from_slice(&[0x82, 0x84, 0x8b, 0x96, 0x24, 0x30, 0x48, 0x6c]);

    Ok(total_len)
}

pub fn parse_assoc_req(frame_buf: &[u8]) -> Result<([u8; 6], u16, u16), i32> {
    if frame_buf.len() < 28 {
        return Err(-22); // -EINVAL
    }

    let fc = u16::from_le_bytes([frame_buf[0], frame_buf[1]]);
    if (fc & 0x00FC) != 0x0000 && (fc & 0x00FC) != 0x0020 {
        return Err(-22);
    }

    let mut sta_mac = [0u8; 6];
    sta_mac.copy_from_slice(&frame_buf[10..16]);

    let capability = u16::from_le_bytes([frame_buf[24], frame_buf[25]]);
    let listen_interval = u16::from_le_bytes([frame_buf[26], frame_buf[27]]);

    Ok((sta_mac, capability, listen_interval))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_beacon_frame() {
        let ssid = b"MT7603U-Hotspot";
        let bssid = [0x00, 0x0C, 0x43, 0x76, 0x03, 0x01];
        let mut out_buf = [0u8; 128];

        let res = build_beacon(ssid, &bssid, 6, &mut out_buf);
        assert!(res.is_ok());
        let len = res.unwrap();
        assert!(len >= 48);

        // Frame Control: 0x0080
        assert_eq!(out_buf[0], 0x80);
        assert_eq!(out_buf[1], 0x00);
        // Destination Broadcast
        assert_eq!(&out_buf[4..10], &[0xFF; 6]);
        // BSSID
        assert_eq!(&out_buf[10..16], &bssid);
        // SSID IE
        assert_eq!(out_buf[36], 0); // Element ID SSID
        assert_eq!(out_buf[37], 15);
        assert_eq!(&out_buf[38..53], b"MT7603U-Hotspot");
    }

    #[test]
    fn test_build_assoc_resp() {
        let sta_mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let bssid = [0x00, 0x0C, 0x43, 0x76, 0x03, 0x01];
        let mut out_buf = [0u8; 128];

        let res = build_assoc_resp(&sta_mac, &bssid, 1, 0, &mut out_buf);
        assert!(res.is_ok());
        let len = res.unwrap();
        assert_eq!(len, 40);

        // Frame Control: 0x0010
        assert_eq!(out_buf[0], 0x10);
        assert_eq!(out_buf[1], 0x00);
        // Destination STA MAC
        assert_eq!(&out_buf[4..10], &sta_mac);
        // Status Code 0 (Success)
        assert_eq!(out_buf[26], 0);
        assert_eq!(out_buf[27], 0);
    }

    #[test]
    fn test_parse_assoc_req() {
        let mut frame = [0u8; 32];
        frame[0] = 0x00; // Assoc Req FC
        frame[1] = 0x00;
        frame[10..16].copy_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        frame[24] = 0x01; // Capability
        frame[25] = 0x00;
        frame[26] = 10; // Listen Interval
        frame[27] = 0;

        let res = parse_assoc_req(&frame);
        assert!(res.is_ok());
        let (sta_mac, cap, listen) = res.unwrap();
        assert_eq!(sta_mac, [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        assert_eq!(cap, 0x0001);
        assert_eq!(listen, 10);
    }
}
