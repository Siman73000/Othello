#![no_std]
#![no_main]

use core::panic::PanicInfo;
use x86_64::instructions::port::{PortRead, PortWrite};

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub fn _start() -> ! {
    // Example: Write 0xAB to port 0x60 and read from it
    port_byte_out(0x60, 0xAB);
    let value = port_byte_in(0x60);
    loop {}
}

pub fn port_byte_in(port: u16) -> u8 {
    let mut port = PortRead::new(port);
    port.read()
}

pub fn port_byte_out(port: u16, data: u8) {
    let mut port = PortWrite::new(port);
    port.write(data);
}

pub fn port_word_in(port: u16) -> u16 {
    let mut port = PortRead::new(port);
    port.read()
}

pub fn port_word_out(port: u16, data: u16) {
    let mut port = PortWrite::new(port);
    port.write(data);
}
