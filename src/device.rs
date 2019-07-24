use libcontainer::errors::*;
use std::fs::{self, DirEntry, File};
use std::io::Write;

#[cfg(any(
    target_arch = "x86_64",
    target_arch = "x86",
    target_arch = "powerpc64le",
    target_arch = "s390x"
))]
const ROOT_BUS_PATH: &'static str = "/devices/pci0000:00";
#[cfg(target_arch = "arm")]
const ROOT_BUS_PATH: &'static str = "/devices/platform/4010000000.pcie/pci0000:00";

const SYSFS_DIR: &'static str = "/sys";

const SYS_BUS_PREFIX: &'static str = "/sys/bus/pci/devices";
const PCI_BUS_RESCAN_FILE: &'static str = "/sys/bus/pci/rescan";
const SYSTEM_DEV_PATH: &'static str = "/dev";

// SCSI const

// Here in "0:0", the first number is the SCSI host number because
// only one SCSI controller has been plugged, while the second number
// is always 0.
const SCSI_HOST_CHANNEL: &'static str = "0:0:";
const SYS_CLASS_PREFIX: &'static str = "/sys/class";
const SCSI_DISK_PREFIX: &'static str = "/sys/class/scsi_disk/0:0:";
const SCSI_BLOCK_SUFFIX: &'static str = "block";
const SCSI_DISK_SUFFIX: &'static str = "/device/block";
const SCSI_HOST_PATH: &'static str = "/sys/class/scis_host";

pub fn rescan_pci_bus() -> Result<()> {
    let byte: [u8; 1] = [1; 1];
    let mut f = File::create(PCI_BUS_RESCAN_FILE)?;
    f.write(&byte)?;
    Ok(())
}

// get_device_pci_address fetches the complete PCI address in sysfs, based on the PCI
// identifier provided. This should be in the format: "bridgeAddr/deviceAddr".
// Here, bridgeAddr is the address at which the brige is attached on the root bus,
// while deviceAddr is the address at which the device is attached on the bridge.
pub fn get_device_pci_address(pci_id: &str) -> Result<String> {
    let tokens: Vec<&str> = pci_id.split("/").collect();

    if tokens.len() != 2 {
        return Err(ErrorKind::ErrorCode(format!(
            "PCI Identifier for device should be of format [bridgeAddr/deviceAddr], got {}",
            pci_id
        ))
        .into());
    }

    let bridge_id = tokens[0];
    let device_id = tokens[1];

    // Deduce the complete bridge address based on the bridge address identifier passed
    // and the fact that bridges are attached on the main bus with function 0.
    let pci_bridge_addr = format!("0000:00:{}.0", bridge_id);

    // Find out the bus exposed by bridge
    let bridge_bus_path = format!("{}/{}/pci_bus/", SYS_BUS_PREFIX, pci_bridge_addr);

    let files_slice: Vec<_> = fs::read_dir(&bridge_bus_path)
        .unwrap()
        .map(|res| res.unwrap().path())
        .collect();
    let bus_num = files_slice.len();

    if bus_num != 1 {
        return Err(ErrorKind::ErrorCode(format!(
            "Expected an entry for bus in {}, got {} entries instead",
            bridge_bus_path, bus_num
        ))
        .into());
    }

    let bus = files_slice[0].file_name();

    // Device address is based on the bus of the bridge to which it is attached.
    // We do not pass devices as multifunction, hence the trailing 0 in the address.
    let pci_device_addr = format!("{:?}:{}.0", bus, device_id);

    let bridge_device_pci_addr = format!("{}/{}", pci_bridge_addr, pci_device_addr);

    info!(
        "Fetched PCI address for device completePCIAddr:{}\n",
        bridge_device_pci_addr
    );

    Ok(bridge_device_pci_addr)
}