#![no_std]
#![no_main]

use core::panic::PanicInfo;
use x86_64::instructions::port::{Port, PortWriteOnly};
use core::ptr;

pub fn network_Scan() -> ! {
    Port::new(0x52).write(0x00);
    loop {
        let status: u8 = Port::new(0x53).read();
        if status & 0x01 != 0 {
            let byte1: u8 = Port::new(0x54).read();
            let byte2: u8 = Port::new(0x55).read();
            let byte3: u8 = Port::new(0x56).read();
            let byte4: u8 = Port::new(0x57).read();
        }
        if status & 0x02 != 0 {
            break;
        }
    }
}