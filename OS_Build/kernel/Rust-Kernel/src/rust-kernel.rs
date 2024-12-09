#![no_std]
#![no_main]

use core::ffi::c_char;

fn clear_screen();
fn print_string(str: char*);
fn print_nl();
fn int_to_string(v: u32, buff: char*, radix_base: u32);

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

fn strlen(s: *const c_char) -> usize {
    unsafe {
        let mut len = 0;
        while *s.add(len) != 0 {
            len += 1;
        }
        len
    }
}

extern "C" {
    fn clear_screen();
    fn print_string(s: *const c_char);
    fn print_nl();
    fn int_to_string(value: i32, buffer: *mut c_char, radix: i32) -> *mut c_char;
}

const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn kernel_main() -> ! {
    unsafe {
        clear_screen();
    }

    let mut buffer: [u8; 100] = [0; 100];

    loop {
        for i in 1..=35 {
            unsafe {
                let c_string = int_to_string(i, buffer.as_mut_ptr().cast(), 10);
                print_string(c_string);
                print_nl();
            }
        }
    }
}
