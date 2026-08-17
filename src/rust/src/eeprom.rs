//! EEPROM Binary Parsing & Calibration
//! Mapped Spec: `specs/modules/eeprom.spec.md`

use crate::ffi::EepromData;

pub fn parse_eeprom(buf: &[u8]) -> Result<EepromData, i32> {
    if buf.len() < 256 {
        return Err(-22); // -EINVAL
    }

    let mut data = EepromData::default();

    // Check MAC address at offset 0x04 or 0x24
    let mac_offset = 0x04;
    let mac_bytes = &buf[mac_offset..mac_offset + 6];

    // Check if MAC is invalid (all 0x00 or all 0xFF)
    let is_all_zero = mac_bytes.iter().all(|&b| b == 0x00);
    let is_all_ff = mac_bytes.iter().all(|&b| b == 0xFF);

    if is_all_zero || is_all_ff {
        // Fallback MAC
        data.mac_addr = [0x00, 0x0C, 0x43, 0x76, 0x03, 0x01];
    } else {
        data.mac_addr.copy_from_slice(mac_bytes);
    }

    // Extract TX power for 14 2.4G channels (offset 0x50)
    if buf.len() >= 0x50 + 14 {
        data.tx_power_2g.copy_from_slice(&buf[0x50..0x50 + 14]);
    }

    // RSSI calibration offset at EEPROM 0x46 (vendor `EEPROM_RSSI_BG_OFFSET`,
    // common/eeprom.c:122-123). Clamp to [-10,10] else 0 (common/eeprom.c:261-262).
    if buf.len() >= 0x47 {
        let offset = buf[0x46] as i8;
        data.rssi_offset_2g = if (-10..=10).contains(&offset) {
            offset
        } else {
            0
        };
    }

    data.is_valid = 1;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_eeprom() {
        let mut buf = [0u8; 512];
        buf[0x04..0x0A].copy_from_slice(&[0x00, 0x0C, 0x43, 0x76, 0x03, 0x01]);

        let result = parse_eeprom(&buf);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.mac_addr, [0x00, 0x0C, 0x43, 0x76, 0x03, 0x01]);
        assert_eq!(data.is_valid, 1);
    }

    #[test]
    fn test_parse_invalid_buffer() {
        let buf = [0u8; 128];
        let result = parse_eeprom(&buf);
        assert_eq!(result, Err(-22));
    }

    #[test]
    fn test_parse_fallback_mac() {
        let buf = [0xFFu8; 512];
        let result = parse_eeprom(&buf);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.mac_addr, [0x00, 0x0C, 0x43, 0x76, 0x03, 0x01]);
        assert_eq!(data.is_valid, 1);
    }

    #[test]
    fn test_parse_rssi_offset() {
        let mut buf = [0u8; 512];
        buf[0x04..0x0A].copy_from_slice(&[0x00, 0x0C, 0x43, 0x76, 0x03, 0x01]);
        buf[0x46] = 0x02;
        buf[0x47] = 0x01;

        let data = parse_eeprom(&buf).unwrap();
        assert_eq!(data.rssi_offset_2g, 2);
        assert_eq!(data.is_valid, 1);
    }

    #[test]
    fn test_parse_rssi_offset_clamp() {
        let mut buf = [0u8; 512];
        buf[0x04..0x0A].copy_from_slice(&[0x00, 0x0C, 0x43, 0x76, 0x03, 0x01]);
        buf[0x46] = 0x30; // 48, out of [-10,10] -> clamp to 0

        let data = parse_eeprom(&buf).unwrap();
        assert_eq!(data.rssi_offset_2g, 0);

        buf[0x46] = 0xF6; // -10, in range
        let data = parse_eeprom(&buf).unwrap();
        assert_eq!(data.rssi_offset_2g, -10);
    }
}
