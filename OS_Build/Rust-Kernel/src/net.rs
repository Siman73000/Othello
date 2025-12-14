#![allow(dead_code)]
pub struct NetScanResult {
    pub devices: &'static [&'static str],
}

pub fn net_scan() -> NetScanResult {
    NetScanResult {
        devices: &[
            "rtl8139: simulated device (no real NIC yet)",
            "loopback: 127.0.0.1",
        ],
    }
}
