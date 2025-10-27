
use x86_64::instructions::port::{PortRead, PortWrite};

/// Reads a word from PCI configuration space.
pub fn pci_config_read_word(bus: u8, slot: u8, func: u8, offset: u8) -> u16 {
    let lbus = bus as u32;
    let lslot = slot as u32;
    let lfunc = func as u32;

    // Construct the address.
    let address = (lbus << 16) | (lslot << 11) | (lfunc << 8) | ((offset & 0xfc) as u32) | 0x80000000;

    // Write to the configuration address port (0xCF8).
    outl(0xCF8, address);

    // Read from the configuration data port (0xCFC).
    let tmp = (inl(0xCFC) >> ((offset & 2) * 8)) & 0xFFFF;
    tmp as u16
}

/// Checks the vendor ID of a PCI device.
fn pci_check_vendor(bus: u8, slot: u8) -> Option<u16> {
    let vendor = pci_config_read_word(bus, slot, 0, 0);
    if vendor != 0xFFFF {
        Some(vendor)
    } else {
        None
    }
}

/// Checks a specific PCI device.
fn check_device(bus: u8, device: u8) {
    let function: u8 = 0;

    // Get the vendor ID.
    if let Some(vendor_id) = pci_check_vendor(bus, device) {
        // Process the vendor ID.
        // Extend this logic to handle devices based on their IDs.
        check_function(bus, device, function);
    }
}

/// Writes a 32-bit value to an I/O port.
fn outl(port: u16, value: u32) {
    let mut port_write = PortWrite::new(port);
    port_write.write(value);
}

/// Reads a 32-bit value from an I/O port.
fn inl(port: u16) -> u32 {
    let mut port_read = PortRead::new(port);
    port_read.read()
}

/// Placeholder for function-level checks.
fn check_function(bus: u8, device: u8, function: u8) {
    // Check the device function.
    let vendor = pci_config_read_word(bus, device, function, 0);
    if vendor != 0xFFFF {
        // Process the device function.
    }
}
