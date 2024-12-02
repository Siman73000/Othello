#![feature(asm)]
#![no_std]

pub fn port_byte_in(port: u16) -> u8 {
    let result: u8;
    unsafe {
        asm!(
            "in al, dx",
            out("al") result,
            in("dx") port,
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
    let result: u16;
    unsafe {
        asm!(
            "in ax, dx",
            out("ax") result,
            in("dx") port,
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