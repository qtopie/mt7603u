//! MAC Initialization & Channel Sequence Generator
//! Mapped Spec: `specs/modules/mac.spec.md`

use crate::ffi::RegWriteOp;

// Address constants from specs/schemas/registers.md and include/mac/mac_mt/*.h
pub const TMAC_TCR: u32 = 0x0002_1600;
pub const TMAC_CDTR: u32 = 0x0002_1608;
pub const TMAC_RRCR: u32 = 0x0002_160C;
pub const TMAC_TRCR: u32 = 0x0002_1614;
pub const ARB_SCR: u32 = 0x0002_1480;
pub const ARB_RQCR: u32 = 0x0002_1470;
pub const ARB_TQCR0: u32 = 0x0002_1500;
pub const RMAC_RFCR: u32 = 0x0002_1800;
pub const RMAC_OMA0R0: u32 = 0x0002_1824;
pub const RMAC_OMA0R1: u32 = 0x0002_1828;
pub const RMAC_RMACDR: u32 = 0x0002_1878;
pub const RMAC_RMCR: u32 = 0x0002_1880;
pub const RMAC_MAXMINLEN: u32 = 0x0002_1898;
pub const RMAC_RFCR1: u32 = 0x0002_18A4;
pub const AGG_AWSCR: u32 = 0x0002_1248;
pub const AGG_AWSCR1: u32 = 0x0002_124C;
pub const AGG_AALCR: u32 = 0x0002_1250;
pub const AGG_AALCR1: u32 = 0x0002_1254;
pub const DMA_DCR1: u32 = 0x0002_1C04;
pub const DMA_RCFR0: u32 = 0x0002_1C70;
pub const DMA_VCFR0: u32 = 0x0002_1C7C;
pub const WTBL_OFF_RMVTCR: u32 = 0x0002_3008;
pub const USB_DMA_CFG: u32 = 0x0002_4000;
pub const SCH_REG4: u32 = 0x0000_4594;
pub const PSE_CLIENT_TX_PAD_DW2: u32 = 0x000C_0040;
pub const PSE_CLIENT_TX_PAD_DW3: u32 = 0x000C_0044;
pub const PSE_CLIENT_TX_PAD_DW4: u32 = 0x000C_0048;
pub const PSE_CLIENT_TX_PAD_DW5: u32 = 0x000C_004C;
pub const PSE_CLIENT_TX_PAD_DW6: u32 = 0x000C_0050;
pub const PSE_CLIENT_RXINF: u32 = 0x000C_0068;

pub fn build_mac_init_sequence(out_ops: &mut [RegWriteOp]) -> Result<usize, i32> {
    let static_seq = [
        RegWriteOp {
            addr: USB_DMA_CFG,
            val: 0x0018_0000,
        }, // Enable RxBulkEn & TxBulkEn
        RegWriteOp {
            addr: SCH_REG4,
            val: 0x0000_0000,
        }, // Normal LMAC prediction mode (exit bypass mode)
        RegWriteOp {
            addr: PSE_CLIENT_RXINF,
            val: 0x0000_0007,
        }, // Enable RX Group 1, 2, 3 to HIF port
        RegWriteOp {
            addr: PSE_CLIENT_TX_PAD_DW2,
            val: 0x0000_0000,
        },
        RegWriteOp {
            addr: PSE_CLIENT_TX_PAD_DW3,
            val: 0x0000_0001,
        }, // remain_tx_cnt = 1
        RegWriteOp {
            addr: PSE_CLIENT_TX_PAD_DW4,
            val: 0x0000_0000,
        },
        RegWriteOp {
            addr: PSE_CLIENT_TX_PAD_DW5,
            val: 0x0000_0020,
        }, // PID_DATA_AMPDU
        RegWriteOp {
            addr: PSE_CLIENT_TX_PAD_DW6,
            val: 0x0000_0000,
        },
        RegWriteOp {
            addr: TMAC_TCR,
            val: 0x0004_0001,
        }, // Enable TX & RX_RIFS_MODE (bit 18)
        RegWriteOp {
            addr: TMAC_CDTR,
            val: 0x0030_00E7,
        }, // CCK timing patch
        RegWriteOp {
            addr: TMAC_RRCR,
            val: 0x0000_0004,
        }, // Rate retry control
        RegWriteOp {
            addr: TMAC_TRCR,
            val: 0x8000_0000,
        }, // Throughput patch
        RegWriteOp {
            addr: ARB_SCR,
            val: 0x0000_0000,
        }, // Clear TXDIS & RXDIS
        RegWriteOp {
            addr: ARB_TQCR0,
            val: 0xFFFF_FFFF,
        }, // Enable all TX queues
        RegWriteOp {
            addr: ARB_RQCR,
            val: 0x0000_000F,
        }, // ARB_RQCR_RX_START | RXV_START | RXV_R_EN | RXV_T_EN (0x0F)
        RegWriteOp {
            addr: RMAC_RMACDR,
            val: 0x4000_0000,
        }, // SELECT_RXMAXLEN_20BIT
        RegWriteOp {
            addr: RMAC_MAXMINLEN,
            val: 0x0E01_9000,
        }, // Min 14 bytes (0x0E << 24) + Max 102400 (0x19000)
        RegWriteOp {
            addr: RMAC_RFCR,
            val: 0x0000_0002,
        }, // DROP_FCS_ERROR_FRAME (0x2), promiscuous disabled for HW Auto-ACK
        RegWriteOp {
            addr: RMAC_RFCR1,
            val: 0x0000_0000,
        },
        RegWriteOp {
            addr: RMAC_RMCR,
            val: 0x00F0_0000,
        }, // Enable RX Stream 0 & 1, Disable SMPS (0x00F00000)
        RegWriteOp {
            addr: AGG_AWSCR,
            val: 0x0000_0040,
        },
        RegWriteOp {
            addr: AGG_AWSCR1,
            val: 0x2A15_1410,
        },
        RegWriteOp {
            addr: AGG_AALCR,
            val: 0x0000_0010,
        },
        RegWriteOp {
            addr: AGG_AALCR1,
            val: 0x1515_1515,
        },
        RegWriteOp {
            addr: DMA_RCFR0,
            val: 0xC021_0000,
        }, // Route all RX packets to HIF
        RegWriteOp {
            addr: DMA_VCFR0,
            val: 0x0000_2000,
        }, // RxRing 1
        RegWriteOp {
            addr: DMA_DCR1,
            val: 0x0000_3800,
        }, // RXSM Groups 1, 2, 3 enable
        RegWriteOp {
            addr: WTBL_OFF_RMVTCR,
            val: 0x0080_0000,
        }, // RX_MV_MODE enable
        RegWriteOp {
            addr: PSE_CLIENT_RXINF,
            val: 0x0000_0007,
        }, // RXSH_GROUP1_EN | RXSH_GROUP2_EN | RXSH_GROUP3_EN → route all RX groups to HIF
    ];

    if out_ops.len() < static_seq.len() {
        return Err(-28); // -ENOSPC
    }

    out_ops[..static_seq.len()].copy_from_slice(&static_seq);
    Ok(static_seq.len())
}

pub fn build_own_mac_sequence(mac: &[u8; 6], out_ops: &mut [RegWriteOp]) -> Result<usize, i32> {
    if out_ops.len() < 2 {
        return Err(-28); // -ENOSPC
    }

    let val0 = (mac[0] as u32)
        | ((mac[1] as u32) << 8)
        | ((mac[2] as u32) << 16)
        | ((mac[3] as u32) << 24);
    let val1 = (mac[4] as u32) | ((mac[5] as u32) << 8) | (1 << 16); // 1 << 16 = ENABLE_OWN_MAC

    out_ops[0] = RegWriteOp {
        addr: RMAC_OMA0R0,
        val: val0,
    };
    out_ops[1] = RegWriteOp {
        addr: RMAC_OMA0R1,
        val: val1,
    };

    Ok(2)
}

pub const RMAC_CHFREQ: u32 = 0x0002_1890;

pub fn build_channel_sequence(
    channel: u8,
    _bw: u8,
    out_ops: &mut [RegWriteOp],
) -> Result<usize, i32> {
    if !(1..=14).contains(&channel) {
        return Err(-22); // -EINVAL
    }

    let seq = [
        RegWriteOp {
            addr: RMAC_CHFREQ,
            val: 1,
        },
        RegWriteOp {
            addr: RMAC_RMCR,
            val: 0x00F0_0000,
        }, // Ensure RX stream 0 & 1 active
        RegWriteOp {
            addr: ARB_RQCR,
            val: 0x0000_000F,
        }, // RX start + RXV enable
        RegWriteOp {
            addr: ARB_SCR,
            val: 0x0000_0000,
        }, // Clear TX/RX disable
    ];

    if out_ops.len() < seq.len() {
        return Err(-28); // -ENOSPC
    }

    out_ops[..seq.len()].copy_from_slice(&seq);
    Ok(seq.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mac_init_sequence() {
        let mut ops = [RegWriteOp { addr: 0, val: 0 }; 32];
        let res = build_mac_init_sequence(&mut ops);
        assert!(res.is_ok());
        let count = res.unwrap();
        assert!(count > 0);
        assert_eq!(ops[0].addr, USB_DMA_CFG);
        assert!(ops.iter().any(|op| op.addr == AGG_AWSCR));
        assert!(ops.iter().any(|op| op.addr == RMAC_RMACDR));
    }

    #[test]
    fn test_mac_init_buffer_overflow() {
        let mut ops = [RegWriteOp { addr: 0, val: 0 }; 2];
        let res = build_mac_init_sequence(&mut ops);
        assert_eq!(res, Err(-28));
    }

    #[test]
    fn test_channel_switch_sequence() {
        let mut ops = [RegWriteOp { addr: 0, val: 0 }; 16];
        let res = build_channel_sequence(6, 0, &mut ops);
        assert!(res.is_ok());
        assert_eq!(ops[0].addr, RMAC_CHFREQ);
        assert_eq!(ops[0].val, 1);
    }

    #[test]
    fn test_own_mac_sequence() {
        let mac = [0x00, 0x0C, 0x43, 0x76, 0x03, 0x01];
        let mut ops = [RegWriteOp { addr: 0, val: 0 }; 4];
        let res = build_own_mac_sequence(&mac, &mut ops);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 2);
        assert_eq!(ops[0].addr, RMAC_OMA0R0);
        assert_eq!(ops[0].val, 0x76430C00);
        assert_eq!(ops[1].addr, RMAC_OMA0R1);
        assert_eq!(ops[1].val, 0x00010103); // [4]=0x03, [5]=0x01, bit 16=1
    }
}
