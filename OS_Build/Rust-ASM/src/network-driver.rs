#![no_std]
#![no_main]

use core::panic::PanicInfo;
use x86_64::instructions::port::{Port, PortWriteOnly};
use core::ptr;

pub fn network_Scan() -> ! {
    Port::new(0x52).write(0x00);
    
}