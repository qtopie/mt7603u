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
pub const FC_FRP: u32 = 0x0000_8138;
pub const MT_TXTIME_THD_0: u32 = 0x0000_4500;
pub const MT_TXTIME_THD_1: u32 = 0x0000_4504;
pub const MT_TXTIME_THD_2: u32 = 0x0000_4508;
pub const MT_TXTIME_THD_3: u32 = 0x0000_450C;
pub const MT_TXTIME_THD_4: u32 = 0x0000_4510;
pub const MT_TXTIME_THD_5: u32 = 0x0000_4514;
pub const MT_TXTIME_THD_6: u32 = 0x0000_4518;
pub const MT_TXTIME_THD_7: u32 = 0x0000_451C;
pub const MT_TXTIME_THD_8: u32 = 0x0000_4520;
pub const MT_TXTIME_THD_9: u32 = 0x0000_4524;
pub const MT_TXTIME_THD_10: u32 = 0x0000_4528;
pub const MT_TXTIME_THD_11: u32 = 0x0000_452C;
pub const MT_TXTIME_THD_12: u32 = 0x0000_4530;
pub const MT_TXTIME_THD_13: u32 = 0x0000_4534;
pub const MT_TXTIME_THD_14: u32 = 0x0000_4538;
pub const MT_TXTIME_THD_15: u32 = 0x0000_453C;
pub const MT_PAGE_CNT_0: u32 = 0x0000_4540;
pub const MT_PAGE_CNT_1: u32 = 0x0000_4544;
pub const MT_PAGE_CNT_2: u32 = 0x0000_4548;
pub const MT_PAGE_CNT_3: u32 = 0x0000_454C;
pub const MT_PAGE_CNT_4: u32 = 0x0000_4550;
pub const MT_PAGE_CNT_5: u32 = 0x0000_4554;
pub const MT_PAGE_CNT_7: u32 = 0x0000_455C;
pub const MT_PAGE_CNT_8: u32 = 0x0000_4560;
pub const MT_QUEUE_PRIORITY_1: u32 = 0x0000_4580;
pub const MT_QUEUE_PRIORITY_2: u32 = 0x0000_4584;
pub const MT_SCH_REG_1: u32 = 0x0000_4588;
pub const MT_SCH_REG_2: u32 = 0x0000_458C;
pub const SCH_REG4: u32 = 0x0000_4594;
pub const MT_GROUP_THD_0: u32 = 0x0000_4598;
pub const MT_GROUP_THD_1: u32 = 0x0000_459C;
pub const MT_GROUP_THD_2: u32 = 0x0000_45A0;
pub const MT_BMAP_0: u32 = 0x0000_45B0;
pub const MT_BMAP_1: u32 = 0x0000_45B4;
pub const MT_HIGH_PRIORITY_1: u32 = 0x0000_45BC;
pub const MT_HIGH_PRIORITY_2: u32 = 0x0000_45C0;
pub const MT_PRIORITY_MASK: u32 = 0x0000_45C4;
pub const MT_RSV_MAX_THD: u32 = 0x0000_45C8;
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
        // AsicDMASchedulerInit(pAd, DMA_SCH_LMAC) (hw_ctrl/cmm_asic_mt.c:6200-6372)
        // FC_P0 (0x8120) = 0x0288_0288 -> page_cnt = 0x288, page_size = 128
        RegWriteOp {
            addr: FC_FRP,
            val: 0x0000_4037,
        },
        RegWriteOp {
            addr: MT_HIGH_PRIORITY_1,
            val: 0x5555_5555,
        },
        RegWriteOp {
            addr: MT_HIGH_PRIORITY_2,
            val: 0x7855_5555,
        },
        RegWriteOp {
            addr: MT_QUEUE_PRIORITY_1,
            val: 0x2B1A_096E,
        },
        RegWriteOp {
            addr: MT_QUEUE_PRIORITY_2,
            val: 0x785F_4D3C,
        },
        RegWriteOp {
            addr: MT_PRIORITY_MASK,
            val: 0xFFFF_FFFF,
        },
        RegWriteOp {
            addr: MT_SCH_REG_1,
            val: 0x2000_0288,
        },
        RegWriteOp {
            addr: MT_SCH_REG_2,
            val: 0x0000_000E,
        },
        RegWriteOp {
            addr: MT_PAGE_CNT_0,
            val: 0x0000_000E,
        },
        RegWriteOp {
            addr: MT_PAGE_CNT_1,
            val: 0x0000_000E,
        },
        RegWriteOp {
            addr: MT_PAGE_CNT_2,
            val: 0x0000_000E,
        },
        RegWriteOp {
            addr: MT_PAGE_CNT_3,
            val: 0x0000_000E,
        },
        RegWriteOp {
            addr: MT_PAGE_CNT_4,
            val: 0x0000_000E,
        },
        RegWriteOp {
            addr: MT_PAGE_CNT_5,
            val: 0x0000_0020,
        },
        RegWriteOp {
            addr: MT_PAGE_CNT_7,
            val: 0x0000_000E,
        },
        RegWriteOp {
            addr: MT_PAGE_CNT_8,
            val: 0x0000_0032,
        },
        RegWriteOp {
            addr: MT_RSV_MAX_THD,
            val: 0x0000_0288,
        },
        RegWriteOp {
            addr: MT_GROUP_THD_0,
            val: 0x0000_025A,
        },
        RegWriteOp {
            addr: MT_GROUP_THD_1,
            val: 0x0000_000E,
        },
        RegWriteOp {
            addr: MT_BMAP_0,
            val: 0x0080_FF5F,
        },
        RegWriteOp {
            addr: MT_GROUP_THD_2,
            val: 0x0000_0020,
        },
        RegWriteOp {
            addr: MT_BMAP_1,
            val: 0x0000_0020,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_0,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_1,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_2,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_3,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_4,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_5,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_6,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_7,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_8,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_9,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_10,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_11,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_12,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_13,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_14,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: MT_TXTIME_THD_15,
            val: 0x000F_FFFF,
        },
        RegWriteOp {
            addr: SCH_REG4,
            val: 0x0000_0040,
        }, // Normal DMA scheduler mode + bit 6 (cmm_asic_mt.c:6367)
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
        let mut ops = [RegWriteOp { addr: 0, val: 0 }; 128];
        let res = build_mac_init_sequence(&mut ops);
        assert!(res.is_ok());
        let count = res.unwrap();
        assert!(count > 32);
        assert_eq!(ops[0].addr, USB_DMA_CFG);
        assert!(ops.iter().any(|op| op.addr == AGG_AWSCR));
        assert!(ops.iter().any(|op| op.addr == RMAC_RMACDR));
        assert!(ops
            .iter()
            .any(|op| op.addr == FC_FRP && op.val == 0x0000_4037));
        assert!(ops
            .iter()
            .any(|op| op.addr == MT_SCH_REG_1 && op.val == 0x2000_0288));
        assert!(ops
            .iter()
            .any(|op| op.addr == MT_SCH_REG_2 && op.val == 0x0000_000E));
        assert!(ops
            .iter()
            .any(|op| op.addr == MT_GROUP_THD_0 && op.val == 0x0000_025A));
        assert!(ops
            .iter()
            .any(|op| op.addr == MT_BMAP_0 && op.val == 0x0080_FF5F));
        assert!(ops
            .iter()
            .any(|op| op.addr == SCH_REG4 && op.val == 0x0000_0040));
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
