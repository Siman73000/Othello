//! PS/2 keyboard driver (separate from mouse using AUX bit).

#![allow(dead_code)]

use core::arch::asm;

const PS2_STATUS: u16 = 0x64;
const PS2_DATA: u16   = 0x60;

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!(
        "in al, dx",
        out("al") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );
    value
}

/// Poll the controller for a **keyboard** scan code.
///
/// Filters out mouse bytes (AUX bit set).
pub fn keyboard_poll_scancode() -> Option<u8> {
    unsafe {
        let status = inb(PS2_STATUS);

        // Output buffer not full?
        if status & 0x01 == 0 {
            return None;
        }

        // AUX bit set => mouse data, ignore here.
        if status & 0x20 != 0 {
            return None;
        }

        let sc = inb(PS2_DATA);
        Some(sc)
    }
}

/// Minimal set-1 scancode → ASCII (no modifiers).
pub fn scancode_to_ascii(sc: u8) -> Option<char> {
    if sc & 0x80 != 0 {
        // Break code
        return None;
    }

    let ch = match sc {
        // Row 1
        0x02 => '1',
        0x03 => '2',
        0x04 => '3',
        0x05 => '4',
        0x06 => '5',
        0x07 => '6',
        0x08 => '7',
        0x09 => '8',
        0x0A => '9',
        0x0B => '0',
        0x0C => '-',
        0x0D => '=',

        // Row 2: Q W E R T Y U I O P [ ]
        0x10 => 'q',
        0x11 => 'w',
        0x12 => 'e',
        0x13 => 'r',
        0x14 => 't',
        0x15 => 'y',
        0x16 => 'u',
        0x17 => 'i',
        0x18 => 'o',
        0x19 => 'p',
        0x1A => '[',   // [
        0x1B => ']',   // ]

        // Row 3: A S D F G H J K L ; '
        0x1E => 'a',
        0x1F => 's',
        0x20 => 'd',
        0x21 => 'f',
        0x22 => 'g',
        0x23 => 'h',
        0x24 => 'j',
        0x25 => 'k',
        0x26 => 'l',
        0x27 => ';',
        0x28 => '\'',

        // Row 4: Z X C V B N M , . /
        0x2C => 'z',
        0x2D => 'x',
        0x2E => 'c',
        0x2F => 'v',
        0x30 => 'b',
        0x31 => 'n',
        0x32 => 'm',
        0x33 => ',',
        0x34 => '.',
        0x35 => '/',

        // Space + Enter
        0x39 => ' ',
        0x1C => '\n',  // Enter

        _ => return None,
    };

    Some(ch)
}
