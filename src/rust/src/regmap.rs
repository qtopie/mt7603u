//! MT7603U Register Address Mapping
//!
//! Port of vendor driver `mt_physical_addr_map` (common/mt_io.c).
//! USB vendor requests must use the *mapped* global physical address:
//! `wValue = addr[31:16]`, `wIndex = addr[15:0]`.

/// MAC CR ranges: (global base, hif segment start, segment length).
/// Source: common/mt_io.c `mt_mac_cr_range`.
const MAC_CR_RANGES: [(u32, u32, u32); 17] = [
    (0x6000_0000, 0x20000, 0x200),  // WF_CFG
    (0x6010_0000, 0x21000, 0x200),  // WF_TRB
    (0x6011_0000, 0x21200, 0x200),  // WF_AGG
    (0x6012_0000, 0x21400, 0x200),  // WF_ARB
    (0x6013_0000, 0x21600, 0x200),  // WF_TMAC
    (0x6014_0000, 0x21800, 0x200),  // WF_RMAC
    (0x6015_0000, 0x21a00, 0x200),  // WF_SEC
    (0x6016_0000, 0x21c00, 0x200),  // WF_DMA
    (0x6017_0000, 0x21e00, 0x200),  // WF_CFGOFF
    (0x6018_0000, 0x22000, 0x1000), // WF_PF
    (0x6019_0000, 0x23000, 0x200),  // WF_WTBLOFF
    (0x601a_0000, 0x23200, 0x200),  // WF_ETBF
    (0x6030_0000, 0x24000, 0x400),  // WF_LPON
    (0x6031_0000, 0x24400, 0x200),  // WF_INT
    (0x6032_0000, 0x28000, 0x4000), // WF_WTBLON
    (0x6033_0000, 0x2c000, 0x200),  // WF_MIB
    (0x6040_0000, 0x2d000, 0x200),  // WF_AON
];

/// Maps a HIF (kernel-facing) register address to the global physical
/// address required for USB vendor requests.
///
/// Behavior mirrors vendor `mt_physical_addr_map` (common/mt_io.c:84).
pub fn physical_addr_map(addr: u32) -> u32 {
    match addr {
        a if a < 0x2000 => 0x8002_0000 + a,            // TOP_CFG
        a if a < 0x4000 => 0x8000_0000 + a - 0x2000,   // MCU_CFG
        a if a < 0x8000 => 0x5000_0000 + a - 0x4000,   // PDMA_CFG
        a if a < 0x10000 => 0xa000_0000 + a - 0x8000,  // PSE_CFG
        a if a < 0x20000 => 0x6020_0000 + a - 0x10000, // WF_PHY
        a if a < 0x40000 => mac_cr_range_map(a),
        a if a < 0x80000 => 0xa500_0000 + a - 0x40000, // WTBL
        a if (0xc0000..0xc0100).contains(&a) => 0x800c_0000 + a - 0xc0000, // PSE Client
        a => a,
    }
}

/// Look up the MAC CR range table for a HIF address in [0x20000, 0x40000).
/// Returns the mapped global address, or the input unchanged if not found
/// (vendor logs "unknow addr range" in that case).
fn mac_cr_range_map(addr: u32) -> u32 {
    for &(base, seg_start, seg_len) in MAC_CR_RANGES.iter() {
        if addr >= seg_start && addr < seg_start + seg_len {
            return base + (addr - seg_start);
        }
    }
    addr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_top_cfg_region() {
        // TOP_MISC2 (0x1134) -> 0x80021134
        assert_eq!(physical_addr_map(0x1134), 0x8002_1134);
        assert_eq!(physical_addr_map(0x0000), 0x8002_0000);
        assert_eq!(physical_addr_map(0x1fff), 0x8002_1fff);
    }

    #[test]
    fn maps_mcu_cfg_region() {
        assert_eq!(physical_addr_map(0x2000), 0x8000_0000);
        assert_eq!(physical_addr_map(0x2abc), 0x8000_0abc);
        assert_eq!(physical_addr_map(0x3fff), 0x8000_1fff);
    }

    #[test]
    fn maps_pdma_cfg_region() {
        // SCH_REG4 (0x4594) -> 0x50000594
        assert_eq!(physical_addr_map(0x4594), 0x5000_0594);
        assert_eq!(physical_addr_map(0x4000), 0x5000_0000);
        assert_eq!(physical_addr_map(0x7fff), 0x5000_3fff);
    }

    #[test]
    fn maps_pse_cfg_region() {
        assert_eq!(physical_addr_map(0x8000), 0xa000_0000);
        assert_eq!(physical_addr_map(0xffff), 0xa000_7fff);
    }

    #[test]
    fn maps_wf_phy_region() {
        assert_eq!(physical_addr_map(0x10000), 0x6020_0000);
        assert_eq!(physical_addr_map(0x1ffff), 0x6020_ffff);
    }

    #[test]
    fn maps_mac_cr_ranges() {
        // WF_TMAC segment start
        assert_eq!(physical_addr_map(0x21600), 0x6013_0000);
        // WF_LPON
        assert_eq!(physical_addr_map(0x24000), 0x6030_0000);
        // WF_AON last range
        assert_eq!(physical_addr_map(0x2d000), 0x6040_0000);
        // Inside WF_WTBLON (len 0x4000)
        assert_eq!(physical_addr_map(0x28000), 0x6032_0000);
        assert_eq!(physical_addr_map(0x2bfff), 0x6032_3fff);
    }

    #[test]
    fn maps_wtbl_region() {
        assert_eq!(physical_addr_map(0x40000), 0xa500_0000);
        assert_eq!(physical_addr_map(0x7ffff), 0xa503_ffff);
    }

    #[test]
    fn maps_pse_client_region() {
        assert_eq!(physical_addr_map(0xc0000), 0x800c_0000);
        assert_eq!(physical_addr_map(0xc00ff), 0x800c_00ff);
    }

    #[test]
    fn passthrough_unknown_region() {
        assert_eq!(physical_addr_map(0xdead_beef), 0xdead_beef);
    }
}
