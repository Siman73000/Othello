//#![feature(asm)]
#![no_std]
#![no_main]


use core::panic::PanicInfo;

/*
fn port_byte_out(port: u32, data: u8);
fn port_byte_in(port: u32) -> u8;
fn port_word_in(port: u32) -> u16;
fn port_word_out(port: u32, data: u16);
*/

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub fn _start() -> ! {
    loop {}
}


use core::arch::asm;

pub fn port_byte_in(port: u16) -> u8 {
    let mut result: u8;
    unsafe {
        asm!(
            "in al, dx",
            in("dx") port,
            lateout("al") result,
        );
    }
    result
}

pub fn port_byte_out(port: u16, data: u8) {
    unsafe {
        asm!(
            "out dx, al",
            in("al") data,
            in("dx") port,
        );
    }
}

pub fn port_word_in(port: u16) -> u16 {
    let mut result: u16;
    unsafe {
        asm!(
            "in ax, dx",
            in("dx") port,
            lateout("ax") result,
        );
    }
    result
}

pub fn port_word_out(port: u16, data: u16) {
    unsafe {
        asm!(
            "out dx, ax",
            in("ax") data,
            in("dx") port,
        );
    }
}