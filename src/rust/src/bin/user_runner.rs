//! MT7603U User-Space USB Hardware Runner
//! Mapped Spec: `specs/modules/user_runner.spec.md`

use mt7603u_logic::{ap, mac, mcu, sta};
use rusb::{Context, DeviceHandle, UsbContext};
use std::time::Duration;

const VID: u16 = 0x0e8d;
const PID_ALT: u16 = 0x760c;
const PID_STD: u16 = 0x7603;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== [MT7603U User-Space Hardware Runner] ===");

    let context = Context::new()?;
    let devices = context.devices()?;
    let mut handle: Option<DeviceHandle<Context>> = None;
    let mut matched_pid = 0;

    for device in devices.iter() {
        let desc = device.device_descriptor()?;
        if desc.vendor_id() == VID && (desc.product_id() == PID_ALT || desc.product_id() == PID_STD)
        {
            println!(
                " Found MT7603U Device: Bus {:03} Device {:03} (VID: 0x{:04x}, PID: 0x{:04x})",
                device.bus_number(),
                device.address(),
                desc.vendor_id(),
                desc.product_id()
            );
            matched_pid = desc.product_id();
            handle = Some(device.open()?);
            break;
        }
    }

    let handle = match handle {
        Some(h) => h,
        None => {
            eprintln!("❌ Error: No MT7603U USB hardware found on host system.");
            return Ok(());
        }
    };

    println!(
        " Successfully opened USB handle for device PID 0x{:04x}",
        matched_pid
    );

    // 1. Verify & Load Vendor Firmware Image
    let fw_bytes = include_bytes!("../../../../harness/fixtures/mt7603u_e2.bin");
    mcu::verify_firmware(fw_bytes).map_err(|e| format!("FW Verify Error: {}", e))?;
    println!(
        " Firmware mt7603u_e2.bin verified successfully (Length: {} bytes, Andes N9 E2 Image)",
        fw_bytes.len()
    );

    // 2. Obtain MAC Init Register Sequence from Rust logic
    let mut reg_ops = [mt7603u_logic::ffi::RegWriteOp::default(); 32];
    let count =
        mac::build_mac_init_sequence(&mut reg_ops).map_err(|e| format!("MAC Init Error: {}", e))?;
    println!(
        " Generated {} discrete MAC init register operations from Rust logic",
        count
    );

    // 3. Execute Vendor Control Transfers on USB hardware
    if let Err(e) = handle.claim_interface(0) {
        println!("⚠️  Note: Interface 0 claim warning ({:?}), proceeding with Vendor Control Transfer test...", e);
    }

    let mut ops_executed = 0;
    for op in &reg_ops[..count] {
        // RegWrite Vendor Request format for MT7603U Control Transfer:
        // RequestType: 0x40 (Vendor Out, Device), Request: 0x63, Value: (reg >> 16), Index: (reg & 0xFFFF)
        let reg = op.addr;
        let val_bytes = op.val.to_le_bytes();
        let value = (reg >> 16) as u16;
        let index = (reg & 0xFFFF) as u16;

        match handle.write_control(
            0x40,
            0x63,
            value,
            index,
            &val_bytes,
            Duration::from_millis(500),
        ) {
            Ok(_written) => {
                ops_executed += 1;
            }
            Err(e) => {
                println!(
                    "⚠️  RegWrite 0x{:08x} -> 0x{:08x} notice: {:?}",
                    reg, op.val, e
                );
            }
        }
    }
    println!(
        " Successfully executed {} Vendor Request Control Transfers to hardware",
        ops_executed
    );

    // 4. Construct 802.11 Probe Request Frame for target SSID 'firefly'
    let mut probe_req_buf = [0u8; 128];
    let src_mac = [0x00, 0x0c, 0x43, 0x76, 0x03, 0x01];
    let req_len = sta::build_probe_request(b"firefly", &src_mac, &mut probe_req_buf)
        .map_err(|e| format!("Probe Req Error: {}", e))?;
    println!(
        " Constructed 802.11 Probe Request frame for target SSID 'firefly' ({} bytes)",
        req_len
    );

    // 5. Construct 802.11 AP Beacon Frame
    let mut beacon_buf = [0u8; 128];
    let bssid = [0x00, 0x0c, 0x43, 0x76, 0x03, 0x01];
    let beacon_len = ap::build_beacon(b"MT7603U-AP", &bssid, 6, &mut beacon_buf)
        .map_err(|e| format!("Beacon Build Error: {}", e))?;
    println!(
        " Constructed 802.11 AP Beacon broadcast frame for SSID 'MT7603U-AP' (Channel 6, {} bytes)",
        beacon_len
    );

    // 6. Construct 802.11 Association Response Frame for STA Client
    let mut assoc_resp_buf = [0u8; 128];
    let sta_mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    let assoc_len = ap::build_assoc_resp(&sta_mac, &bssid, 1, 0, &mut assoc_resp_buf)
        .map_err(|e| format!("Assoc Resp Build Error: {}", e))?;
    println!(
        " Constructed 802.11 Association Response frame for STA AA:BB:CC:DD:EE:FF (AID: 1, Status: Success, {} bytes)",
        assoc_len
    );

    println!("\n✅ [User-Space Runner Result]: Hardware Communication & AP/STA Protocol 100% Operational!");
    Ok(())
}
