#![no_std]
#![no_main]

use core::arch::asm;

#[no_mangle]
pub extern "C" fn print16(mut message: *const u8) {
    unsafe {
        while *message != 0 {
            let char = *message;
            asm!(
                "mov ah, 0x0e", // TTY output
                "int 0x10",     // BIOS interrupt
                in("al") char,
                options(nomem, nostack)
            );
            message = message.add(1); // Move to the next character
        }
    }
}

#[no_mangle]
pub extern "C" fn print16_nl() {
    unsafe {
        asm!(
            "mov ah, 0x0e", // TTY output
            "mov al, 0x0a", // Newline character
            "int 0x10",
            "mov al, 0x0d", // Carriage return
            "int 0x10",
            options(nomem, nostack)
        );
    }
}

#[no_mangle]
pub extern "C" fn print16_cls() {
    unsafe {
        asm!(
            "mov ah, 0x00", // Set video mode
            "mov al, 0x03", // 80x25 text mode, 16 colors
            "int 0x10",
            options(nomem, nostack)
        );
    }
}

#[no_mangle]
pub extern "C" fn print16_hex(mut value: u16) {
    const HEX_DIGITS: &[u8] = b"0123456789ABCDEF";
    let mut output = [b'0', b'x', b'0', b'0', b'0', b'0', 0];
    let mut index = 5;

    while value > 0 || index > 1 {
        let digit = (value & 0xF) as usize; // Extract the last nibble
        output[index] = HEX_DIGITS[digit];
        value >>= 4; // Shift right by 4 bits
        if index > 2 {
            index -= 1;
        }
    }

    print16(output.as_ptr());
}
