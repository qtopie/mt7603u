//! MCU Firmware & Command Packet Builder
//! Mapped Spec: `specs/modules/mcu.spec.md`
//!
//! Ports the vendor Andes MCU FW_TXD framing (mcu/andes_mt.c:
//! AndesMTFillCmdHeader) and the firmware download command sequence
//! (AndesMTLoadFwMethod1).

pub const FW_TXD_HDR_SIZE: usize = 12;
pub const USB_END_PADDING: usize = 4;
pub const MT7603_E2_FW_SIZE: usize = 74372;

// Command IDs (include/mcu/andes_mt.h)
pub const MT_TARGET_ADDRESS_LEN_REQ: u8 = 0x01;
pub const MT_FW_START_REQ: u8 = 0x02;
pub const MT_FW_SCATTER: u8 = 0xEE;
pub const MT_RESTART_DL_REQ: u8 = 0xEF;
pub const EXT_CID: u8 = 0xED;
pub const EXT_CMD_NA: u8 = 0;
pub const EXT_CMD_RADIO_ON_OFF_CTRL: u8 = 0x05;

// Packet type / set-query constants
pub const PKT_ID_CMD: u8 = 0xA0;
pub const CMD_SET: u8 = 1;
pub const CMD_QUERY: u8 = 0;
pub const CMD_NA: u8 = 3;
pub const EXT_CID_OPTION_NEED_ACK: u8 = 1;
pub const EXT_CID_OPTION_NO_NEED_ACK: u8 = 0;

// Priority queue IDs (include/mcu/andes_core.h / andes_mt.c)
pub const P1_Q0: u16 = 0x8000;
pub const FW_SCATTER_PQ_ID: u16 = 0xC000;

// Firmware download parameters
pub const FW_CODE_START_ADDRESS1: u32 = 0x100000;
pub const TARGET_ADDR_LEN_NEED_RSP: u32 = 0x8000_0000;
pub const MT_UPLOAD_FW_UNIT: usize = 4096;
/// sizeof(FW_TXD) in vendor; scatter payload cap = MT_UPLOAD_FW_UNIT - cmd_header_len
pub const FW_TXD_FULL_SIZE: usize = 32;
pub const FW_SCATTER_MAX_PAYLOAD: usize = MT_UPLOAD_FW_UNIT - FW_TXD_FULL_SIZE;

/// Builds an FW_TXD command header + payload.
///
/// `hdr_size` selects the TXD header size by firmware stage (see
/// `specs/modules/mcu.spec.md` §2.1):
/// - `FW_TXD_HDR_SIZE` (12) during firmware download
///   (`FW_NO_INIT`/`FW_DOWNLOAD`/`ROM_PATCH_DOWNLOAD`), matching vendor
///   `OS_PKT_HEAD_BUF_EXTEND(net_pkt, 12)`.
/// - `FW_TXD_FULL_SIZE` (32) at `FW_RUN_TIME` (firmware running), matching
///   vendor `OS_PKT_HEAD_BUF_EXTEND(net_pkt, sizeof(*fw_txd))` and mainline
///   `hdrlen = dev->mcu_running ? sizeof(struct mt7603_mcu_txd) : 12`.
///
/// In the 32-byte mode the 20-byte reserved block `au4D3toD7rev[5]`
/// (offset 12..32) is explicitly zeroed, matching mainline
/// `__mt76_mcu_msg_alloc`'s `memset(skb->head, 0, len)`.
///
/// `ext_cid_option` is computed per vendor logic: NEED_ACK only when
/// `cid == EXT_CID && set_query ∈ {CMD_SET, CMD_QUERY} && need_rsp`.
///
/// The parameters mirror the vendor `AndesMTFillCmdHeader` FW_TXD union
/// fields 1:1 (see `specs/modules/mcu.spec.md` §2), so they are intentionally
/// kept flat rather than grouped into a struct.
#[allow(clippy::too_many_arguments)]
pub fn build_fw_txd_frame(
    cid: u8,
    pq_id: u16,
    set_query: u8,
    ext_cid: u8,
    seq: u8,
    need_rsp: bool,
    hdr_size: usize,
    payload: &[u8],
    out_buf: &mut [u8],
) -> Result<usize, i32> {
    debug_assert!(hdr_size == FW_TXD_HDR_SIZE || hdr_size == FW_TXD_FULL_SIZE);

    let total_len = payload.len() + hdr_size;
    if out_buf.len() < total_len {
        return Err(-28); // -ENOSPC
    }

    // Zero the reserved block `au4D3toD7rev` (offset 12..hdr_size) with
    // volatile byte writes. A `slice::fill(0)` would make LLVM emit a
    // `memset` libcall through a GOT-indirect `call *memset@GOTPCREL(%rip)`,
    // which the kernel module loader cannot relocate (same issue as the
    // efuse payload, see `build_efuse_buffer_mode_cmd` docs) and oopses on
    // probe. Bytes 0..12 are set by explicit stores below.
    if hdr_size > FW_TXD_HDR_SIZE {
        for i in FW_TXD_HDR_SIZE..hdr_size {
            // SAFETY: i < hdr_size <= out_buf.len() (checked above).
            unsafe { core::ptr::write_volatile(out_buf.as_mut_ptr().add(i), 0u8) };
        }
    }

    let len = (total_len as u16).to_le_bytes();
    out_buf[0] = len[0];
    out_buf[1] = len[1];

    let pq = pq_id.to_le_bytes();
    out_buf[2] = pq[0];
    out_buf[3] = pq[1];

    out_buf[4] = cid;
    out_buf[5] = PKT_ID_CMD;
    out_buf[6] = set_query;
    out_buf[7] = seq;
    out_buf[8] = 0x00; // ucD2B0Rev
    out_buf[9] = ext_cid;
    out_buf[10] = 0x00; // ucD2B2Rev

    let need_ack =
        cid == EXT_CID && (set_query == CMD_SET || set_query == CMD_QUERY) && need_rsp && seq != 0;
    out_buf[11] = if need_ack {
        EXT_CID_OPTION_NEED_ACK
    } else {
        EXT_CID_OPTION_NO_NEED_ACK
    };

    crate::util::copy_bytes(&mut out_buf[hdr_size..total_len], payload);
    Ok(total_len)
}

/// Builds the target address/length request frame (cid=0x01).
/// Payload: [address, dl_len, data_mode] each little-endian u32.
///
/// `seq` is the need_wait command sequence number assigned by the caller
/// (vendor `AndesGetCmdMsgSeq`): the first need_wait command of the download
/// flow carries seq=1 (the counter starts at 0 and pre-increments, and 0 is
/// reserved for no-wait commands).
pub fn build_addr_len_req(
    address: u32,
    dl_len: u32,
    seq: u8,
    out_buf: &mut [u8],
) -> Result<usize, i32> {
    let mut payload = [0u8; 12];
    payload[0..4].copy_from_slice(&address.to_le_bytes());
    payload[4..8].copy_from_slice(&dl_len.to_le_bytes());
    payload[8..12].copy_from_slice(&TARGET_ADDR_LEN_NEED_RSP.to_le_bytes());

    // need_rsp=true; ext_cid field = EXT_CMD_NA (non-EXT command)
    // Download-phase command: 12-byte FW_TXD short header (FW_RUN_TIME not set).
    build_fw_txd_frame(
        MT_TARGET_ADDRESS_LEN_REQ,
        P1_Q0,
        CMD_NA,
        EXT_CMD_NA,
        seq,
        true,
        FW_TXD_HDR_SIZE,
        &payload,
        out_buf,
    )
}

/// Builds a firmware start request frame (cid=0x02).
/// Payload: [override, entry_address] each little-endian u32.
///
/// `seq` is the need_wait command sequence number assigned by the caller
/// (vendor `AndesGetCmdMsgSeq`): the second need_wait command of the download
/// flow carries seq=2.
pub fn build_fw_start_req(
    override_: u32,
    address: u32,
    seq: u8,
    out_buf: &mut [u8],
) -> Result<usize, i32> {
    let mut payload = [0u8; 8];
    payload[0..4].copy_from_slice(&override_.to_le_bytes());
    payload[4..8].copy_from_slice(&address.to_le_bytes());

    // need_rsp=true; ext_cid field = EXT_CMD_NA (non-EXT command)
    // Download-phase command: 12-byte FW_TXD short header (FW_RUN_TIME not set).
    build_fw_txd_frame(
        MT_FW_START_REQ,
        P1_Q0,
        CMD_NA,
        EXT_CMD_NA,
        seq,
        true,
        FW_TXD_HDR_SIZE,
        &payload,
        out_buf,
    )
}

/// Builds a restart download request frame (cid=0xEF).
///
/// Sent when the chip's RAM firmware is already running (e.g. re-probe / warm
/// restart) so the MCU resets its execution back to ROM code.
///
/// This is a runtime-phase command (the RAM firmware is already executing), so
/// it must use the 32-byte full FW_TXD header — vendor `AndesMTFillCmdHeader`
/// uses `sizeof(*fw_txd)` when `Ctl->Stage == FW_RUN_TIME`. A 12-byte short
/// header would not be parsed by the running firmware, leaving the ROM restart
/// never acknowledged and TOP_MISC2 stuck at RAM-running (0x03/0x02).
pub fn build_restart_dl_req(seq: u8, out_buf: &mut [u8]) -> Result<usize, i32> {
    build_fw_txd_frame(
        MT_RESTART_DL_REQ,
        P1_Q0,
        CMD_NA,
        EXT_CMD_NA,
        seq,
        true,
        FW_TXD_FULL_SIZE,
        &[],
        out_buf,
    )
}

pub const EXT_CMD_CHANNEL_SWITCH: u8 = 0x08;

/// Builds an EXT_CMD_CHANNEL_SWITCH command frame (cid=0xED, ext_cid=0x08).
/// Payload is 36 bytes (EXT_CMD_CHAN_SWITCH_T).
pub fn build_channel_switch_cmd(
    ctrl_ch: u8,
    central_ch: u8,
    bw: u8,
    tx_streams: u8,
    rx_streams: u8,
    seq: u8,
    out_buf: &mut [u8],
) -> Result<usize, i32> {
    let mut payload = [0u8; 36];
    payload[0] = ctrl_ch;
    payload[1] = central_ch;
    payload[2] = bw;
    payload[3] = tx_streams;
    payload[4] = rx_streams;
    payload[12..33].fill(0xFF); // aucTxPowerSKU

    // Runtime-phase command: 32-byte full FW_TXD header (FW_RUN_TIME).
    build_fw_txd_frame(
        EXT_CID,
        P1_Q0,
        CMD_SET,
        EXT_CMD_CHANNEL_SWITCH,
        seq,
        true,
        FW_TXD_FULL_SIZE,
        &payload,
        out_buf,
    )
}

pub const EXT_CMD_SET_TX_POWER_CTRL: u8 = 0x11;

/// EEPROM byte offsets used by the vendor `CmdSetTxPowerCtrl` (andes_mt.c:3608)
/// to fill the TX power control payload on eFuse-backed MT7603U devices.
/// All values come from `include/eeprom/mt_e2p_def.h`.
mod e2p_offsets {
    pub const NIC_CONFIGURE_1: usize = 0x36;
    pub const G_BAND_20_40_BW_PWR_DELTA: usize = 0x50;
    pub const TX0_G_BAND_TARGET_PWR: usize = 0x58;
    pub const TX0_G_BAND_CHL_PWR_DELTA_MID: usize = 0x5A;
    pub const TX1_G_BAND_TARGET_PWR: usize = 0x5E;
    pub const TX1_G_BAND_CHL_PWR_DELTA_MID: usize = 0x60;
    pub const TX_PWR_CCK_1_2M: usize = 0xA0;
    pub const STEP_NUM_NEG_7: usize = 0xC6;
}

/// Builds an EXT_CMD_SET_TX_POWER_CTRL command frame (cid=0xED, ext_cid=0x11).
///
/// Payload is 44 bytes (EXT_CMD_TX_POWER_CTRL_T), field layout matches the
/// vendor struct exactly:
///   [0]  ucCenterChannel
///   [1]  ucTSSIEnable       (NIC_CONFIGURE_1 bits 15:8)
///   [2]  ucTempCompEnable   (NIC_CONFIGURE_1 bits 7:0)
///   [3..4]  aucTargetPower[2]   (TX0/TX1_G_BAND_TARGET_PWR low bytes)
///   [5..18] aucRatePowerDelta[14] (TX_PWR_CCK_1_2M + i*2, LE words, 7 entries)
///   [19] ucBWPowerDelta     (G_BAND_20_40_BW_PWR_DELTA low byte)
///   [20..25] aucCHPowerDelta[6]  (TX0/TX1 target/high + CHL_PWR_DELTA_MID)
///   [26..42] aucTempCompPower[17] (STEP_NUM_NEG_7 + i*2, 9 entries w/ high bytes)
///   [43] ucReserved
///
/// Values are read from the EEPROM image the same way the vendor does via
/// `RT28xx_EEPROM_READ16`. Unknown/out-of-range reads fall back to 0xff so a
/// partially-filled dummy EEPROM still produces a deterministic payload.
pub fn build_tx_power_ctrl_cmd(
    eeprom: &[u8],
    central_ch: u8,
    seq: u8,
    out_buf: &mut [u8],
) -> Result<usize, i32> {
    use e2p_offsets::*;

    let read16 = |addr: usize| -> u16 {
        let lo = eeprom.get(addr).copied().unwrap_or(0xff) as u16;
        let hi = eeprom.get(addr + 1).copied().unwrap_or(0xff) as u16;
        lo | (hi << 8)
    };

    let mut payload = [0u8; 44];

    payload[0] = central_ch;
    let nic_conf1 = read16(NIC_CONFIGURE_1);
    payload[1] = (nic_conf1 >> 8) as u8; // ucTSSIEnable
    payload[2] = nic_conf1 as u8; // ucTempCompEnable

    payload[3] = read16(TX0_G_BAND_TARGET_PWR) as u8; // aucTargetPower[0]
    payload[4] = read16(TX1_G_BAND_TARGET_PWR) as u8; // aucTargetPower[1]

    for i in 0..7usize {
        let v = read16(TX_PWR_CCK_1_2M + i * 2);
        payload[5 + i * 2] = v as u8;
        payload[6 + i * 2] = (v >> 8) as u8;
    }

    payload[19] = read16(G_BAND_20_40_BW_PWR_DELTA) as u8; // ucBWPowerDelta

    payload[20] = (read16(TX0_G_BAND_TARGET_PWR) >> 8) as u8; // CHPwrDelta[0]
    payload[21] = read16(TX0_G_BAND_CHL_PWR_DELTA_MID) as u8; // CHPwrDelta[1]
    payload[22] = (read16(TX0_G_BAND_CHL_PWR_DELTA_MID) >> 8) as u8; // CHPwrDelta[2]
    payload[23] = (read16(TX1_G_BAND_TARGET_PWR) >> 8) as u8; // CHPwrDelta[3]
    payload[24] = read16(TX1_G_BAND_CHL_PWR_DELTA_MID) as u8; // CHPwrDelta[4]
    payload[25] = (read16(TX1_G_BAND_CHL_PWR_DELTA_MID) >> 8) as u8; // CHPwrDelta[5]

    let mut j = 26;
    for i in 0..9usize {
        let v = read16(STEP_NUM_NEG_7 + i * 2);
        payload[j] = v as u8;
        j += 1;
        if i != 8 {
            payload[j] = (v >> 8) as u8;
            j += 1;
        }
    }

    // Runtime-phase command: 32-byte full FW_TXD header (FW_RUN_TIME).
    build_fw_txd_frame(
        EXT_CID,
        P1_Q0,
        CMD_SET,
        EXT_CMD_SET_TX_POWER_CTRL,
        seq,
        true,
        FW_TXD_FULL_SIZE,
        &payload,
        out_buf,
    )
}

pub const EXT_CMD_EFUSE_BUFFER_MODE: u8 = 0x21;

/// Builds an EXT_CMD_EFUSE_BUFFER_MODE command frame (cid=0xED, ext_cid=0x21).
///
/// Mirrors the vendor MT7603U contract `CmdEfusBufferModeSet` (andes_mt.c):
/// for an eFuse-backed chip (`EEPROM_EFUSE`) the driver only sends
/// `ucSourceMode = EEPROM_MODE_EFUSE(0)`, `ucCount = 0` and zeroes the whole
/// `EXT_CMD_EFUSE_BUFFER_MODE_T` payload (4-byte header + `EFUSE_CONTENT_BUFFER_SIZE`
/// (0xf0=240) `BIN_CONTENT_T` entries). The firmware then reads the on-chip
/// eFuse itself for BBP/RF calibration.
///
/// Payload size = sizeof(EXT_CMD_EFUSE_BUFFER_MODE_T) = 4 + 240*4 = 964 bytes.
/// Do NOT push EEPROM buffer entries: that path (`bufferModeFieldSet` /
/// `CmdFillEeprom`) is only used for EEPROM_FLASH devices and the openwrt mt76
/// PCIe (MT7603E) firmware — this USB chip runs the vendor v1.14 e2 firmware,
/// which refuses >0xf0 buffer entries.
///
/// NOTE: the payload array is initialized through `MaybeUninit` + volatile
/// writes. `[0u8; 964]` would make LLVM emit a `memset` libcall through a
/// GOT-indirect `call *memset@GOTPCREL(%rip)`, which the kernel module loader
/// cannot relocate (see `util::copy_bytes` docs) and oopses on probe.
///
/// `eeprom` is accepted for API compatibility but ignored: with the eFuse
/// source mode the calibration content is not sent over USB.
pub fn build_efuse_buffer_mode_cmd(
    eeprom: &[u8],
    seq: u8,
    out_buf: &mut [u8],
) -> Result<usize, i32> {
    const EFUSE_CONTENT_BUFFER_SIZE: usize = 0xf0; // vendor limit (240 entries)
    const PAYLOAD_SIZE: usize = 4 + EFUSE_CONTENT_BUFFER_SIZE * 4; // 964

    let _ = eeprom; // eFuse mode: firmware reads calibration from on-chip eFuse

    let mut payload = [core::mem::MaybeUninit::<u8>::uninit(); PAYLOAD_SIZE];
    for slot in payload.iter_mut() {
        // SAFETY: slot is a valid MaybeUninit<u8> in the array.
        unsafe { core::ptr::write_volatile(slot.as_mut_ptr(), 0u8) };
    }

    payload[0].write(0); // ucSourceMode = EEPROM_MODE_EFUSE
    payload[1].write(0); // ucCount = 0 (no buffer entries)

    // SAFETY: every element has been initialized via the volatile loop above.
    let payload_bytes =
        unsafe { core::slice::from_raw_parts(payload.as_ptr() as *const u8, PAYLOAD_SIZE) };

    // Runtime-phase command: 32-byte full FW_TXD header (FW_RUN_TIME).
    build_fw_txd_frame(
        EXT_CID,
        P1_Q0,
        CMD_SET,
        EXT_CMD_EFUSE_BUFFER_MODE,
        seq,
        true,
        FW_TXD_FULL_SIZE,
        payload_bytes,
        out_buf,
    )
}

pub const CMD_CH_PRIVILEGE: u8 = 0x20;

/// Builds a CMD_CH_PRIVILEGE command frame (cid=0x20).
/// Payload is 16 bytes (CMD_CH_PRIVILEGE_T).
pub fn build_ch_privilege_cmd(channel: u8, seq: u8, out_buf: &mut [u8]) -> Result<usize, i32> {
    let mut payload = [0u8; 16];
    payload[2] = 0; // ucAction = CMD_CH_PRIV_ACTION_REQ
    payload[3] = channel; // ucPrimaryChannel
    payload[4] = 0; // ucRfSco = SCN
    payload[5] = 0; // ucRfBand = G_BAND (2.4G)
    payload[6] = 0; // ucRfChannelWidth = 20/40
    payload[9] = 0; // ucReqType = JOIN

    // Runtime-phase command: 32-byte full FW_TXD header (FW_RUN_TIME).
    // need_rsp=false (cmd seq=0 per flow), so no ACK expected.
    build_fw_txd_frame(
        CMD_CH_PRIVILEGE,
        P1_Q0,
        CMD_SET,
        EXT_CMD_NA,
        seq,
        false,
        FW_TXD_FULL_SIZE,
        &payload,
        out_buf,
    )
}

/// Builds a firmware scatter frame (cid=0xEE) from one chunk.
/// Chunk payload must be ≤ `FW_SCATTER_MAX_PAYLOAD` (4064).
/// seq=0 (no need_wait); need_rsp=false.
pub fn build_fw_scatter_frame(chunk: &[u8], out_buf: &mut [u8]) -> Result<usize, i32> {
    if chunk.len() > FW_SCATTER_MAX_PAYLOAD {
        return Err(-22); // -EINVAL
    }
    // Download-phase command: 12-byte FW_TXD short header (FW_RUN_TIME not set).
    build_fw_txd_frame(
        MT_FW_SCATTER,
        FW_SCATTER_PQ_ID,
        CMD_NA,
        EXT_CMD_NA,
        0,
        false,
        FW_TXD_HDR_SIZE,
        chunk,
        out_buf,
    )
}

/// Computes the download length: le32(fw[fw_len-4..fw_len]) + 4 (CRC).
/// Returns None if firmware is shorter than 4 bytes.
pub fn fw_dl_len(fw_buf: &[u8]) -> Option<u32> {
    let tail = fw_buf.get(fw_buf.len().checked_sub(4)?..)?;
    let n = u32::from_le_bytes([tail[0], tail[1], tail[2], tail[3]]);
    Some(n + 4)
}

pub fn verify_firmware(fw_buf: &[u8]) -> Result<(), i32> {
    if fw_buf.len() < 1024 {
        return Err(-22); // -EINVAL
    }

    // Verify Andes N9 machine code instruction signature at offset 0
    // Expected header start: 0x46 0x00 0x01 0x00
    if fw_buf[0] != 0x46 || fw_buf[1] != 0x00 || fw_buf[2] != 0x01 || fw_buf[3] != 0x00 {
        return Err(-22); // -EINVAL: Invalid Firmware Header
    }

    Ok(())
}

/// Builds an EXT_CMD_RADIO_ON_OFF_CTRL command frame (cid=0xED, ext_cid=0x05).
/// Payload is 4 bytes (EXT_CMD_RADIO_ON_OFF_CTRL_T { ucWiFiRadioCtrl: 1(ON)/2(OFF), aucReserve: [0; 3] }).
pub fn build_radio_on_off_cmd(on: bool, seq: u8, out_buf: &mut [u8]) -> Result<usize, i32> {
    let mut payload = [0u8; 4];
    payload[0] = if on { 1 } else { 2 };

    // Runtime-phase command: 32-byte full FW_TXD header (FW_RUN_TIME).
    build_fw_txd_frame(
        EXT_CID,
        P1_Q0,
        CMD_SET,
        EXT_CMD_RADIO_ON_OFF_CTRL,
        seq,
        true,
        FW_TXD_FULL_SIZE,
        &payload,
        out_buf,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_radio_on_off_cmd() {
        let mut out = [0u8; 64];
        let res = build_radio_on_off_cmd(true, 1, &mut out);
        assert!(res.is_ok());
        let len = res.unwrap();
        assert_eq!(len, 36); // 32 Header + 4 Payload

        assert_eq!(out[4], EXT_CID);
        assert_eq!(out[6], CMD_SET);
        assert_eq!(out[7], 1); // seq
        assert_eq!(out[9], EXT_CMD_RADIO_ON_OFF_CTRL);
        assert_eq!(out[11], EXT_CID_OPTION_NEED_ACK);
        // reserved 20B block zeroed in 32B header mode
        assert_eq!(&out[12..32], &[0u8; 20]);
        assert_eq!(out[32], 1); // ON
        assert_eq!(out[33..36], [0, 0, 0]);
    }

    #[test]
    fn test_build_fw_txd_frame() {
        let payload = [0xAAu8; 12];
        let mut out = [0u8; 64];
        let res = build_fw_txd_frame(
            MT_TARGET_ADDRESS_LEN_REQ,
            P1_Q0,
            CMD_NA,
            EXT_CMD_NA,
            1,
            true,
            FW_TXD_HDR_SIZE,
            &payload,
            &mut out,
        );
        assert!(res.is_ok());
        let len = res.unwrap();
        assert_eq!(len, 24);
        // length field (LE16)
        assert_eq!(out[0], 24);
        assert_eq!(out[1], 0);
        // pq_id (LE16)
        assert_eq!(out[2], 0x00);
        assert_eq!(out[3], 0x80);
        // cid / pkt_type / set_query / seq
        assert_eq!(out[4], MT_TARGET_ADDRESS_LEN_REQ);
        assert_eq!(out[5], PKT_ID_CMD);
        assert_eq!(out[6], CMD_NA);
        assert_eq!(out[7], 1);
        // ext_cid, reserved, ext_cid_option
        assert_eq!(out[9], EXT_CMD_NA);
        assert_eq!(out[11], EXT_CID_OPTION_NO_NEED_ACK); // non-EXT command
        assert_eq!(&out[12..24], &payload);
    }

    #[test]
    fn test_build_fw_txd_frame_runtime_32b_header() {
        // FW_RUN_TIME commands use the full 32-byte FW_TXD header.
        let payload = [0xBBu8; 36];
        let mut out = [0u8; 128];
        let res = build_fw_txd_frame(
            EXT_CID,
            P1_Q0,
            CMD_SET,
            EXT_CMD_CHANNEL_SWITCH,
            1,
            true,
            FW_TXD_FULL_SIZE,
            &payload,
            &mut out,
        );
        assert!(res.is_ok());
        let len = res.unwrap();
        assert_eq!(len, 68); // 32 Header + 36 Payload
                             // length field (LE16)
        assert_eq!(out[0], 68);
        assert_eq!(out[1], 0);
        // reserved 20B block zeroed
        assert_eq!(&out[12..32], &[0u8; 20]);
        // payload lands at offset 32
        assert_eq!(&out[32..68], &payload);
    }

    #[test]
    fn test_build_fw_txd_frame_ext_need_ack() {
        // EXT_CID with CMD_SET + need_rsp => NEED_ACK
        let mut out = [0u8; 32];
        let res = build_fw_txd_frame(
            EXT_CID,
            P1_Q0,
            CMD_SET,
            0x12,
            3,
            true,
            FW_TXD_HDR_SIZE,
            &[],
            &mut out,
        );
        assert!(res.is_ok());
        assert_eq!(out[11], EXT_CID_OPTION_NEED_ACK);
        assert_eq!(out[9], 0x12);
    }

    #[test]
    fn test_build_fw_txd_frame_overflow() {
        let payload = [0xAAu8; 64];
        let mut out = [0u8; 32];
        let res = build_fw_txd_frame(
            MT_TARGET_ADDRESS_LEN_REQ,
            P1_Q0,
            CMD_NA,
            EXT_CMD_NA,
            1,
            true,
            FW_TXD_HDR_SIZE,
            &payload,
            &mut out,
        );
        assert_eq!(res, Err(-28));
    }

    #[test]
    fn test_build_fw_txd_frame_overflow_32b() {
        // 32B header mode must also respect max_out_len (header + payload).
        let payload = [0xAAu8; 4];
        let mut out = [0u8; 34];
        let res = build_fw_txd_frame(
            EXT_CID,
            P1_Q0,
            CMD_SET,
            EXT_CMD_RADIO_ON_OFF_CTRL,
            1,
            true,
            FW_TXD_FULL_SIZE,
            &payload,
            &mut out,
        );
        assert_eq!(res, Err(-28)); // needs 36, only 34 available
    }

    #[test]
    fn test_build_addr_len_req() {
        let mut out = [0u8; 64];
        let res = build_addr_len_req(FW_CODE_START_ADDRESS1, 4, 1, &mut out);
        assert!(res.is_ok());
        let len = res.unwrap();
        assert_eq!(len, 24);
        assert_eq!(out[4], MT_TARGET_ADDRESS_LEN_REQ);
        assert_eq!(out[7], 1); // first need_wait command seq
        assert_eq!(&out[12..16], &0x100000u32.to_le_bytes());
        assert_eq!(&out[16..20], &4u32.to_le_bytes());
        assert_eq!(&out[20..24], &TARGET_ADDR_LEN_NEED_RSP.to_le_bytes());
    }

    #[test]
    fn test_build_fw_start_req() {
        let mut out = [0u8; 64];
        let res = build_fw_start_req(1, FW_CODE_START_ADDRESS1, 2, &mut out);
        assert!(res.is_ok());
        let len = res.unwrap();
        assert_eq!(len, 20);
        assert_eq!(out[4], MT_FW_START_REQ);
        assert_eq!(out[7], 2); // second need_wait command seq
        assert_eq!(&out[12..16], &1u32.to_le_bytes());
        assert_eq!(&out[16..20], &0x100000u32.to_le_bytes());
    }

    #[test]
    fn test_build_restart_dl_req() {
        let mut out = [0u8; 64];
        let res = build_restart_dl_req(1, &mut out);
        assert!(res.is_ok());
        let len = res.unwrap();
        assert_eq!(len, 32); // 32-byte full runtime header, no payload
        assert_eq!(out[4], MT_RESTART_DL_REQ);
        assert_eq!(out[5], PKT_ID_CMD);
        assert_eq!(out[6], CMD_NA);
        assert_eq!(out[7], 1);
        assert_eq!(out[9], EXT_CMD_NA);
        assert_eq!(out[11], EXT_CID_OPTION_NO_NEED_ACK); // non-EXT command
    }

    #[test]
    fn test_build_fw_scatter_frame() {
        let chunk = [0x46u8, 0x00, 0x01, 0x00];
        let mut out = [0u8; 64];
        let res = build_fw_scatter_frame(&chunk, &mut out);
        assert!(res.is_ok());
        let len = res.unwrap();
        assert_eq!(len, 16);
        assert_eq!(out[4], MT_FW_SCATTER);
        assert_eq!(out[2], 0x00);
        assert_eq!(out[3], 0xC0);
        assert_eq!(out[7], 0);
        assert_eq!(&out[12..16], &chunk);
    }

    #[test]
    fn test_fw_scatter_payload_cap() {
        let chunk = [0x00u8; FW_SCATTER_MAX_PAYLOAD + 1];
        let mut out = [0u8; FW_SCATTER_MAX_PAYLOAD + FW_TXD_HDR_SIZE + 8];
        let res = build_fw_scatter_frame(&chunk, &mut out);
        assert_eq!(res, Err(-22));
    }

    #[test]
    fn test_fw_dl_len() {
        let mut fw = [0u8; 8];
        fw[4] = 0x20; // little-endian 0x20 = 32
        let dl = fw_dl_len(&fw).unwrap();
        assert_eq!(dl, 36); // 32 + 4 CRC
        assert_eq!(fw_dl_len(&[]), None);
        assert_eq!(fw_dl_len(&[0u8; 2]), None);
    }

    #[test]
    fn test_verify_real_e2_firmware() {
        let fw_bytes = include_bytes!("../../../harness/fixtures/mt7603u_e2.bin");
        assert_eq!(fw_bytes.len(), MT7603_E2_FW_SIZE);

        let res = verify_firmware(fw_bytes);
        assert!(res.is_ok());
    }

    #[test]
    fn test_verify_corrupted_firmware() {
        let bad_bytes = [0x00u8; 64];
        let res = verify_firmware(&bad_bytes);
        assert_eq!(res, Err(-22));
    }

    #[test]
    fn test_dl_len_of_real_firmware() {
        let fw_bytes = include_bytes!("../../../harness/fixtures/mt7603u_e2.bin");
        let dl = fw_dl_len(fw_bytes).unwrap();
        assert!(dl > 0);
        assert!(dl < MT_UPLOAD_FW_UNIT as u32 * 20); // sanity: within scatterable range
    }

    #[test]
    fn test_build_channel_switch_cmd() {
        let mut out = [0u8; 128];
        let res = build_channel_switch_cmd(6, 6, 0, 2, 2, 1, &mut out);
        assert!(res.is_ok());
        let len = res.unwrap();
        assert_eq!(len, 68); // 32 Header + 36 Payload

        assert_eq!(out[4], EXT_CID);
        assert_eq!(out[6], CMD_SET);
        assert_eq!(out[7], 1); // seq
        assert_eq!(out[9], EXT_CMD_CHANNEL_SWITCH);
        assert_eq!(out[11], EXT_CID_OPTION_NEED_ACK);
        // reserved 20B block zeroed in 32B header mode
        assert_eq!(&out[12..32], &[0u8; 20]);
        assert_eq!(out[32], 6); // ctrl_ch
        assert_eq!(out[33], 6); // central_ch
        assert_eq!(out[34], 0); // bw
        assert_eq!(out[35], 2); // tx_streams
        assert_eq!(out[36], 2); // rx_streams
    }

    #[test]
    fn test_build_ch_privilege_cmd() {
        let mut out = [0u8; 64];
        let res = build_ch_privilege_cmd(6, 0, &mut out);
        assert!(res.is_ok());
        let len = res.unwrap();
        assert_eq!(len, 48); // 32 Header + 16 Payload

        assert_eq!(out[4], CMD_CH_PRIVILEGE);
        assert_eq!(out[6], CMD_SET);
        // reserved 20B block zeroed in 32B header mode
        assert_eq!(&out[12..32], &[0u8; 20]);
        assert_eq!(out[35], 6); // ucPrimaryChannel (payload offset 3)
    }

    #[test]
    fn test_build_efuse_buffer_mode_cmd() {
        // eeprom image: 1024 bytes of 0xff with a marker at the MAC-adjacent
        // calibration field 0x034 and at 0x056 (TX power group).
        let mut eeprom = [0xffu8; 1024];
        eeprom[0x034] = 0x12;
        eeprom[0x035] = 0x34;
        eeprom[0x056] = 0xab;

        let mut out = [0u8; 2048];
        let res = build_efuse_buffer_mode_cmd(&eeprom, 3, &mut out);
        assert!(res.is_ok());
        let len = res.unwrap();
        // 32 header + sizeof(EXT_CMD_EFUSE_BUFFER_MODE_T) =
        // 32 + (4 hdr + 240*4 content) = 996
        assert_eq!(len, 996);

        // FW_TXD framing
        assert_eq!(out[4], EXT_CID);
        assert_eq!(out[6], CMD_SET);
        assert_eq!(out[7], 3); // seq
        assert_eq!(out[9], EXT_CMD_EFUSE_BUFFER_MODE);
        assert_eq!(out[11], EXT_CID_OPTION_NEED_ACK);
        // reserved 20B block zeroed in 32B header mode
        assert_eq!(&out[12..32], &[0u8; 20]);

        // Payload header: vendor eFuse contract (CmdEfusBufferModeSet).
        assert_eq!(out[32], 0); // ucSourceMode = EEPROM_MODE_EFUSE
        assert_eq!(out[33], 0); // ucCount = 0 (firmware reads on-chip eFuse)
        assert_eq!(out[34], 0); // reserved
        assert_eq!(out[35], 0); // reserved

        // The EEPROM payload is NOT pushed in eFuse mode: all 240 buffer
        // entries stay zeroed.
        for i in 0..240usize {
            let base = 32 + 4 + i * 4;
            assert_eq!(out[base..base + 4], [0u8, 0, 0, 0], "entry {i}");
        }
    }

    #[test]
    fn test_build_tx_power_ctrl_cmd() {
        // eeprom image with marker values at the vendor field offsets.
        let mut eeprom = [0xffu8; 1024];
        eeprom[0x36] = 0x78; // NIC_CONFIGURE_1 lo -> ucTempCompEnable
        eeprom[0x37] = 0x01; // NIC_CONFIGURE_1 hi -> ucTSSIEnable
        eeprom[0x58] = 0x10; // TX0_G_BAND_TARGET_PWR lo
        eeprom[0x59] = 0x20; // TX0_G_BAND_TARGET_PWR hi -> CHPwrDelta[0]
        eeprom[0x5e] = 0x30; // TX1_G_BAND_TARGET_PWR lo
        eeprom[0x5f] = 0x40; // TX1_G_BAND_TARGET_PWR hi -> CHPwrDelta[3]
        eeprom[0x5a] = 0x50; // TX0 CHL_PWR_DELTA_MID lo -> CHPwrDelta[1]
        eeprom[0x5b] = 0x60; // TX0 CHL_PWR_DELTA_MID hi -> CHPwrDelta[2]
        eeprom[0x60] = 0x70; // TX1 CHL_PWR_DELTA_MID lo -> CHPwrDelta[4]
        eeprom[0x61] = 0x80; // TX1 CHL_PWR_DELTA_MID hi -> CHPwrDelta[5]
        eeprom[0x50] = 0x55; // G_BAND_20_40_BW_PWR_DELTA lo
        eeprom[0xa0] = 0x01; // TX_PWR_CCK_1_2M lo
        eeprom[0xa1] = 0x02; // TX_PWR_CCK_1_2M hi
        eeprom[0xc6] = 0xaa; // STEP_NUM_NEG_7 lo

        let mut out = [0u8; 128];
        let res = build_tx_power_ctrl_cmd(&eeprom, 6, 4, &mut out);
        assert!(res.is_ok());
        let len = res.unwrap();
        assert_eq!(len, 32 + 44); // 32 header + 44 payload

        // Framing
        assert_eq!(out[4], EXT_CID);
        assert_eq!(out[6], CMD_SET);
        assert_eq!(out[7], 4); // seq
        assert_eq!(out[9], EXT_CMD_SET_TX_POWER_CTRL);
        // reserved 20B block zeroed in 32B header mode
        assert_eq!(&out[12..32], &[0u8; 20]);

        // Payload
        assert_eq!(out[32], 6); // ucCenterChannel
        assert_eq!(out[33], 0x01); // ucTSSIEnable = NIC_CONFIGURE_1 >> 8
        assert_eq!(out[34], 0x78); // ucTempCompEnable = NIC_CONFIGURE_1 & 0xff
        assert_eq!(out[35], 0x10); // aucTargetPower[0]
        assert_eq!(out[36], 0x30); // aucTargetPower[1]

        // aucRatePowerDelta[0..1] from TX_PWR_CCK_1_2M LE word
        assert_eq!(out[37], 0x01);
        assert_eq!(out[38], 0x02);
        // remaining rate deltas use 0xff fallback (unset EEPROM bytes)
        assert_eq!(out[39], 0xff);

        assert_eq!(out[51], 0x55); // ucBWPowerDelta

        assert_eq!(out[52], 0x20); // CHPwrDelta[0]
        assert_eq!(out[53], 0x50); // CHPwrDelta[1]
        assert_eq!(out[54], 0x60); // CHPwrDelta[2]
        assert_eq!(out[55], 0x40); // CHPwrDelta[3]
        assert_eq!(out[56], 0x70); // CHPwrDelta[4]
        assert_eq!(out[57], 0x80); // CHPwrDelta[5]

        assert_eq!(out[58], 0xaa); // TempCompPower[0]
        assert_eq!(out[59], 0xff); // TempCompPower[1] hi byte fallback
        assert_eq!(out[75], 0); // ucReserved
    }
}
