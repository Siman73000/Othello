#![no_std]
#![no_main]

use core::ptr;

const VGA_BUFFER: *mut u16 = 0xb8000 as *mut u16;
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;

#[repr(u8)]
enum VgaColor {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGrey = 7,
    DarkGrey = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    LightMagenta = 13,
    LightBrown = 14,
    White = 15,
}

#[inline(always)]
fn vga_entry_color(fg: VgaColor, bg: VgaColor) -> u8 {
    (fg as u8) | ((bg as u8) << 4)
}

#[inline(always)]
fn vga_entry(character: u8, color: u8) -> u16 {
    (character as u16) | ((color as u16) << 8)
}

/// Clears the screen by filling the VGA buffer with spaces.
fn clear_screen() {
    let color = vga_entry_color(VgaColor::White, VgaColor::Black);
    let blank = vga_entry(b' ', color);

    for i in 0..(VGA_WIDTH * VGA_HEIGHT) {
        unsafe {
            ptr::write_volatile(VGA_BUFFER.add(i), blank);
        }
    }
}

/// Writes a string to the VGA buffer.
fn print_string(s: &str) {
    static mut CURSOR_POSITION: usize = 0;

    let color = vga_entry_color(VgaColor::White, VgaColor::Black);

    for byte in s.bytes() {
        if unsafe { CURSOR_POSITION } >= VGA_WIDTH * VGA_HEIGHT {
            // No more space on the screen; do nothing.
            return;
        }

        unsafe {
            ptr::write_volatile(
                VGA_BUFFER.add(CURSOR_POSITION),
                vga_entry(byte, color),
            );
            CURSOR_POSITION += 1;
        }
    }
}

/// Writes a newline by moving the cursor to the start of the next line.
fn print_nl() {
    static mut CURSOR_POSITION: usize = 0;

    unsafe {
        let line_number = CURSOR_POSITION / VGA_WIDTH;
        CURSOR_POSITION = (line_number + 1) * VGA_WIDTH;
    }
}

// Converts an integer to a string in the specified radix.
fn int_to_string(value: u32, buffer: &mut [u8], radix: u32) -> &str {
    assert!(radix >= 2 && radix <= 36);

    let mut v = value;
    let mut i = buffer.len();

    // similar to utility.rs but is used for specific VGA text output
    while v != 0 {
        i -= 1;
        buffer[i] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"[(v % radix) as usize];
        v /= radix;
    }

    let start = i;
    &core::str::from_utf8(&buffer[start..]).unwrap_or("")
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
fn kernel_main() -> ! {
    clear_screen();

    let mut buffer: [u8; 100] = [0; 100];

    loop {
        for i in 1..=35 {
            let number_str = int_to_string(i, &mut buffer, 10);
            print_string(number_str);
            print_nl();
        }
    }
}
