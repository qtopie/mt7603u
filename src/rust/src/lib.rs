//! MT7603U Rust Pure Logic Static Library
//! C ABI FFI Layer for Linux Kernel Driver Glue

#![cfg_attr(all(not(test), not(feature = "user-runner")), no_std)]

#[cfg(test)]
extern crate std;

#[cfg(all(not(test), not(feature = "user-runner")))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[cfg(all(not(test), not(feature = "user-runner")))]
#[no_mangle]
pub extern "C" fn rust_eh_personality() {}

pub mod ap;
pub mod eeprom;
pub mod ffi;
pub mod mac;
pub mod mcu;
pub mod regmap;
pub mod rx;
pub mod sta;
pub mod tx;
pub mod util;

use ffi::{EepromData, RegWriteOp, RxInfo, TxParams};

/// Maps a HIF register address to the global physical address used by
/// USB vendor requests. Port of vendor `mt_physical_addr_map`.
#[no_mangle]
pub extern "C" fn mt7603_rust_map_register_addr(addr: u32) -> u32 {
    regmap::physical_addr_map(addr)
}

/// Parses EEPROM binary buffer.
///
/// # Safety
/// Caller must ensure `buf` points to valid memory of at least `len` bytes,
/// and `out` points to a writeable `EepromData` struct.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_parse_eeprom(
    buf: *const u8,
    len: usize,
    out: *mut EepromData,
) -> i32 {
    if buf.is_null() || out.is_null() {
        return -22; // -EINVAL
    }
    let slice = core::slice::from_raw_parts(buf, len);
    match eeprom::parse_eeprom(slice) {
        Ok(data) => {
            *out = data;
            0
        }
        Err(err) => err,
    }
}

/// Generates MAC initialization register write operations.
///
/// # Safety
/// Caller must ensure `ops_buf` points to a writeable array of size `max_ops`,
/// and `out_count` points to a valid `usize`.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_get_mac_init_sequence(
    ops_buf: *mut RegWriteOp,
    max_ops: usize,
    out_count: *mut usize,
) -> i32 {
    if ops_buf.is_null() || out_count.is_null() {
        return -22; // -EINVAL
    }
    let slice = core::slice::from_raw_parts_mut(ops_buf, max_ops);
    match mac::build_mac_init_sequence(slice) {
        Ok(count) => {
            *out_count = count;
            0
        }
        Err(err) => err,
    }
}

/// Generates channel switching register write operations.
///
/// # Safety
/// Caller must ensure `ops_buf` points to a writeable array of size `max_ops`,
/// and `out_count` points to a valid `usize`.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_get_channel_sequence(
    channel: u8,
    bw: u8,
    ops_buf: *mut RegWriteOp,
    max_ops: usize,
    out_count: *mut usize,
) -> i32 {
    if ops_buf.is_null() || out_count.is_null() {
        return -22; // -EINVAL
    }
    let slice = core::slice::from_raw_parts_mut(ops_buf, max_ops);
    match mac::build_channel_sequence(channel, bw, slice) {
        Ok(count) => {
            *out_count = count;
            0
        }
        Err(err) => err,
    }
}

/// Generates Own MAC address register write operations.
///
/// # Safety
/// Caller must ensure `mac` points to a 6-byte array,
/// `ops_buf` points to a writeable array of size `max_ops` (>= 2),
/// and `out_count` points to a valid `usize`.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_own_mac_sequence(
    mac: *const u8,
    ops_buf: *mut RegWriteOp,
    max_ops: usize,
    out_count: *mut usize,
) -> i32 {
    if mac.is_null() || ops_buf.is_null() || out_count.is_null() {
        return -22; // -EINVAL
    }
    let mac_slice = core::slice::from_raw_parts(mac, 6);
    let mut mac_arr = [0u8; 6];
    mac_arr.copy_from_slice(mac_slice);
    let slice = core::slice::from_raw_parts_mut(ops_buf, max_ops);
    match mac::build_own_mac_sequence(&mac_arr, slice) {
        Ok(count) => {
            *out_count = count;
            0
        }
        Err(err) => err,
    }
}

/// Builds WTBL1 sequence for STA mode (Entry 0 Broadcast + Entry 1 AP BSSID).
///
/// # Safety
/// Caller must ensure `bssid` points to a 6-byte array,
/// `ops_buf` points to a writeable array of size `max_ops` (>= 6),
/// and `out_count` points to a valid `usize`.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_wtbl_sta_sequence(
    bssid: *const u8,
    ops_buf: *mut RegWriteOp,
    max_ops: usize,
    out_count: *mut usize,
) -> i32 {
    if bssid.is_null() || ops_buf.is_null() || out_count.is_null() {
        return -22; // -EINVAL
    }
    let bssid_slice = core::slice::from_raw_parts(bssid, 6);
    let mut bssid_arr = [0u8; 6];
    bssid_arr.copy_from_slice(bssid_slice);
    let slice = core::slice::from_raw_parts_mut(ops_buf, max_ops);
    match sta::build_wtbl_sta_sequence(&bssid_arr, slice) {
        Ok(count) => {
            *out_count = count;
            0
        }
        Err(err) => err,
    }
}

/// Parses an RX packet buffer.
///
/// # Safety
/// Caller must ensure `data` points to valid memory of at least `len` bytes,
/// and `out` points to a writeable `RxInfo` struct.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_parse_rx_frame(
    data: *const u8,
    len: usize,
    rssi_offset: i8,
    out: *mut RxInfo,
) -> i32 {
    if data.is_null() || out.is_null() {
        return -22; // -EINVAL
    }
    let slice = core::slice::from_raw_parts(data, len);
    match rx::parse_rx_frame(slice, rssi_offset) {
        Ok(info) => {
            *out = info;
            0
        }
        Err(err) => err,
    }
}

/// Constructs a TxWI header buffer.
///
/// # Safety
/// Caller must ensure `params` points to a valid `TxParams` struct,
/// and `txwi_buf` points to a writeable memory buffer of at least `txwi_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_txwi(
    params: *const TxParams,
    txwi_buf: *mut u8,
    txwi_len: usize,
) -> i32 {
    if params.is_null() || txwi_buf.is_null() {
        return -22; // -EINVAL
    }
    let p = &*params;
    let slice = core::slice::from_raw_parts_mut(txwi_buf, txwi_len);
    match tx::build_txwi(p, slice) {
        Ok(_) => 0,
        Err(err) => err,
    }
}

/// Constructs a target address/length request frame (cid=0x01) for
/// firmware download.
///
/// # Safety
/// Caller must ensure `out_buf` points to writeable memory of at least
/// `max_out_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_addr_len_req(
    address: u32,
    dl_len: u32,
    seq: u8,
    out_buf: *mut u8,
    max_out_len: usize,
    out_written: *mut usize,
) -> i32 {
    if out_buf.is_null() || out_written.is_null() {
        return -22; // -EINVAL
    }
    let slice = core::slice::from_raw_parts_mut(out_buf, max_out_len);
    match mcu::build_addr_len_req(address, dl_len, seq, slice) {
        Ok(written) => {
            *out_written = written;
            0
        }
        Err(err) => err,
    }
}

/// Constructs a firmware start request frame (cid=0x02) for firmware download.
///
/// # Safety
/// Caller must ensure `out_buf` points to writeable memory of at least
/// `max_out_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_fw_start_req(
    override_flag: u32,
    address: u32,
    seq: u8,
    out_buf: *mut u8,
    max_out_len: usize,
    out_written: *mut usize,
) -> i32 {
    if out_buf.is_null() || out_written.is_null() {
        return -22; // -EINVAL
    }
    let slice = core::slice::from_raw_parts_mut(out_buf, max_out_len);
    match mcu::build_fw_start_req(override_flag, address, seq, slice) {
        Ok(written) => {
            *out_written = written;
            0
        }
        Err(err) => err,
    }
}

/// Constructs a restart download request frame (cid=0xEF) for firmware download.
///
/// # Safety
/// Caller must ensure `out_buf` points to writeable memory of at least
/// `max_out_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_restart_dl_req(
    seq: u8,
    out_buf: *mut u8,
    max_out_len: usize,
    out_written: *mut usize,
) -> i32 {
    if out_buf.is_null() || out_written.is_null() {
        return -22; // -EINVAL
    }
    let slice = core::slice::from_raw_parts_mut(out_buf, max_out_len);
    match mcu::build_restart_dl_req(seq, slice) {
        Ok(written) => {
            *out_written = written;
            0
        }
        Err(err) => err,
    }
}

/// Constructs a firmware scatter frame (cid=0xEE) from one chunk.
/// Chunk payload must be ≤ FW_SCATTER_MAX_PAYLOAD (4064).
///
/// # Safety
/// Caller must ensure `chunk` points to valid memory of at least `chunk_len`
/// bytes, and `out_buf` points to writeable memory of `max_out_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_fw_scatter_frame(
    chunk: *const u8,
    chunk_len: usize,
    out_buf: *mut u8,
    max_out_len: usize,
    out_written: *mut usize,
) -> i32 {
    if chunk.is_null() || out_buf.is_null() || out_written.is_null() {
        return -22; // -EINVAL
    }
    let c = core::slice::from_raw_parts(chunk, chunk_len);
    let slice = core::slice::from_raw_parts_mut(out_buf, max_out_len);
    match mcu::build_fw_scatter_frame(c, slice) {
        Ok(written) => {
            *out_written = written;
            0
        }
        Err(err) => err,
    }
}

/// Computes the firmware download length: le32(fw tail 4B) + 4 (CRC).
/// Returns 0 on success and writes the length to `out`, or negative errno.
///
/// # Safety
/// Caller must ensure `fw_buf` points to valid memory of at least `fw_len`
/// bytes, and `out` points to a valid u32.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_fw_dl_len(
    fw_buf: *const u8,
    fw_len: usize,
    out: *mut u32,
) -> i32 {
    if fw_buf.is_null() || out.is_null() {
        return -22; // -EINVAL
    }
    let slice = core::slice::from_raw_parts(fw_buf, fw_len);
    match mcu::fw_dl_len(slice) {
        Some(len) => {
            *out = len;
            0
        }
        None => -22, // -EINVAL: fw too short
    }
}

/// Verifies MT7603U firmware binary image header integrity.
///
/// # Safety
/// Caller must ensure `fw_buf` points to valid memory of at least `fw_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_verify_firmware(fw_buf: *const u8, fw_len: usize) -> i32 {
    if fw_buf.is_null() {
        return -22; // -EINVAL
    }
    let slice = core::slice::from_raw_parts(fw_buf, fw_len);
    match mcu::verify_firmware(slice) {
        Ok(_) => 0,
        Err(err) => err,
    }
}

/// Constructs a 802.11 Probe Request management frame.
///
/// # Safety
/// Caller must ensure `ssid` points to valid memory of `ssid_len` bytes,
/// `src_mac` points to a 6-byte MAC array, and `out_buf` points to writeable memory of `max_out_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_probe_req(
    ssid: *const u8,
    ssid_len: usize,
    src_mac: *const u8,
    out_buf: *mut u8,
    max_out_len: usize,
    out_written: *mut usize,
) -> i32 {
    if src_mac.is_null() || out_buf.is_null() || out_written.is_null() {
        return -22; // -EINVAL
    }
    let ssid_slice = if ssid.is_null() || ssid_len == 0 {
        &[]
    } else {
        core::slice::from_raw_parts(ssid, ssid_len)
    };
    let mac = &*(src_mac as *const [u8; 6]);
    let slice = core::slice::from_raw_parts_mut(out_buf, max_out_len);
    match sta::build_probe_request(ssid_slice, mac, slice) {
        Ok(written) => {
            *out_written = written;
            0
        }
        Err(err) => err,
    }
}

/// Parses an 802.11 Beacon / Probe Response frame.
///
/// # Safety
/// Caller must ensure `frame_buf` points to valid memory of `frame_len` bytes,
/// and `out_info` points to a writeable `StaBssInfo` struct.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_parse_beacon(
    frame_buf: *const u8,
    frame_len: usize,
    out_info: *mut ffi::StaBssInfo,
) -> i32 {
    if frame_buf.is_null() || out_info.is_null() {
        return -22; // -EINVAL
    }
    let slice = core::slice::from_raw_parts(frame_buf, frame_len);
    match sta::parse_beacon(slice) {
        Ok(info) => {
            *out_info = info;
            0
        }
        Err(err) => err,
    }
}

/// Constructs a 802.11 Beacon broadcast frame for AP hotspot mode.
///
/// # Safety
/// Caller must ensure `ssid` points to valid memory of `ssid_len` bytes,
/// `bssid` points to a 6-byte BSSID array, and `out_buf` points to writeable memory of `max_out_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_beacon(
    ssid: *const u8,
    ssid_len: usize,
    bssid: *const u8,
    channel: u8,
    out_buf: *mut u8,
    max_out_len: usize,
    out_written: *mut usize,
) -> i32 {
    if bssid.is_null() || out_buf.is_null() || out_written.is_null() {
        return -22; // -EINVAL
    }
    let ssid_slice = if ssid.is_null() || ssid_len == 0 {
        &[]
    } else {
        core::slice::from_raw_parts(ssid, ssid_len)
    };
    let mac = &*(bssid as *const [u8; 6]);
    let slice = core::slice::from_raw_parts_mut(out_buf, max_out_len);
    match ap::build_beacon(ssid_slice, mac, channel, slice) {
        Ok(written) => {
            *out_written = written;
            0
        }
        Err(err) => err,
    }
}

/// Constructs a 802.11 Association Response frame for a STA client.
///
/// # Safety
/// Caller must ensure `sta_mac` points to a 6-byte STA MAC array, `bssid` points to a 6-byte BSSID array,
/// and `out_buf` points to writeable memory of `max_out_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_assoc_resp(
    sta_mac: *const u8,
    bssid: *const u8,
    aid: u16,
    status_code: u16,
    out_buf: *mut u8,
    max_out_len: usize,
    out_written: *mut usize,
) -> i32 {
    if sta_mac.is_null() || bssid.is_null() || out_buf.is_null() || out_written.is_null() {
        return -22; // -EINVAL
    }
    let mac_sta = &*(sta_mac as *const [u8; 6]);
    let mac_bssid = &*(bssid as *const [u8; 6]);
    let slice = core::slice::from_raw_parts_mut(out_buf, max_out_len);
    match ap::build_assoc_resp(mac_sta, mac_bssid, aid, status_code, slice) {
        Ok(written) => {
            *out_written = written;
            0
        }
        Err(err) => err,
    }
}

/// Parses an 802.11 Association Request frame from a STA client.
///
/// # Safety
/// Caller must ensure `frame_buf` points to valid memory of at least `frame_len` bytes,
/// and `out_sta_mac` points to a 6-byte writeable memory.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_parse_assoc_req(
    frame_buf: *const u8,
    frame_len: usize,
    out_sta_mac: *mut u8,
    out_cap: *mut u16,
    out_listen: *mut u16,
) -> i32 {
    if frame_buf.is_null() || out_sta_mac.is_null() || out_cap.is_null() || out_listen.is_null() {
        return -22; // -EINVAL
    }
    let slice = core::slice::from_raw_parts(frame_buf, frame_len);
    match ap::parse_assoc_req(slice) {
        Ok((sta_mac, cap, listen)) => {
            core::ptr::copy_nonoverlapping(sta_mac.as_ptr(), out_sta_mac, 6);
            *out_cap = cap;
            *out_listen = listen;
            0
        }
        Err(err) => err,
    }
}

/// Constructs a channel switch MCU command frame (cid=0xED, ext_cid=0x08).
///
/// # Safety
/// Caller must ensure `out_buf` points to writeable memory of at least `max_out_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_chan_switch_cmd(
    ctrl_ch: u8,
    central_ch: u8,
    bw: u8,
    tx_streams: u8,
    rx_streams: u8,
    seq: u8,
    out_buf: *mut u8,
    max_out_len: usize,
    out_written: *mut usize,
) -> i32 {
    if out_buf.is_null() || out_written.is_null() {
        return -22; // -EINVAL
    }
    let slice = core::slice::from_raw_parts_mut(out_buf, max_out_len);
    match mcu::build_channel_switch_cmd(ctrl_ch, central_ch, bw, tx_streams, rx_streams, seq, slice)
    {
        Ok(written) => {
            *out_written = written;
            0
        }
        Err(err) => err,
    }
}

/// Constructs an EXT_CMD_SET_TX_POWER_CTRL (0x11) MCU command frame with the
/// TX power control fields derived from the EEPROM image (vendor eFuse path).
///
/// # Safety
/// Caller must ensure `out_buf` points to writeable memory of at least `max_out_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_tx_power_ctrl_cmd(
    eeprom: *const u8,
    eeprom_len: usize,
    central_ch: u8,
    seq: u8,
    out_buf: *mut u8,
    max_out_len: usize,
    out_written: *mut usize,
) -> i32 {
    if eeprom.is_null() || out_buf.is_null() || out_written.is_null() {
        return -22; // -EINVAL
    }
    let eeprom = core::slice::from_raw_parts(eeprom, eeprom_len);
    let slice = core::slice::from_raw_parts_mut(out_buf, max_out_len);
    match mcu::build_tx_power_ctrl_cmd(eeprom, central_ch, seq, slice) {
        Ok(written) => {
            *out_written = written;
            0
        }
        Err(err) => err,
    }
}

/// Constructs an EXT_CMD_EFUSE_BUFFER_MODE (0x21) MCU command frame that
/// pushes the EEPROM calibration data to the firmware.
///
/// # Safety
/// Caller must ensure `out_buf` points to writeable memory of at least `max_out_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_efuse_buffer_mode_cmd(
    eeprom: *const u8,
    eeprom_len: usize,
    seq: u8,
    out_buf: *mut u8,
    max_out_len: usize,
    out_written: *mut usize,
) -> i32 {
    if eeprom.is_null() || out_buf.is_null() || out_written.is_null() {
        return -22; // -EINVAL
    }
    let eeprom = core::slice::from_raw_parts(eeprom, eeprom_len);
    let slice = core::slice::from_raw_parts_mut(out_buf, max_out_len);
    match mcu::build_efuse_buffer_mode_cmd(eeprom, seq, slice) {
        Ok(written) => {
            *out_written = written;
            0
        }
        Err(err) => err,
    }
}

/// Constructs a Radio On/Off MCU command frame (cid=0xED, ext_cid=0x05).
///
/// # Safety
/// Caller must ensure `out_buf` points to writeable memory of at least `max_out_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_radio_on_off_cmd(
    on: bool,
    seq: u8,
    out_buf: *mut u8,
    max_out_len: usize,
    out_written: *mut usize,
) -> i32 {
    if out_buf.is_null() || out_written.is_null() {
        return -22; // -EINVAL
    }
    let slice = core::slice::from_raw_parts_mut(out_buf, max_out_len);
    match mcu::build_radio_on_off_cmd(on, seq, slice) {
        Ok(written) => {
            *out_written = written;
            0
        }
        Err(err) => err,
    }
}

/// Constructs a Channel Privilege MCU command frame (cid=0x20).
///
/// # Safety
/// Caller must ensure `out_buf` points to writeable memory of at least `max_out_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn mt7603_rust_build_ch_privilege_cmd(
    channel: u8,
    seq: u8,
    out_buf: *mut u8,
    max_out_len: usize,
    out_written: *mut usize,
) -> i32 {
    if out_buf.is_null() || out_written.is_null() {
        return -22; // -EINVAL
    }
    let slice = core::slice::from_raw_parts_mut(out_buf, max_out_len);
    match mcu::build_ch_privilege_cmd(channel, seq, slice) {
        Ok(written) => {
            *out_written = written;
            0
        }
        Err(err) => err,
    }
}
