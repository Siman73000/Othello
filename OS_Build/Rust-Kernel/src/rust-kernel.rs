#![no_std]
#![no_main]

mod display;
mod network_drivers;
mod vga_driver;
mod dp_driver;
mod hdmi_driver;
mod window;


use core::ptr;
use vga_driver::vga_init;
use hdmi_driver::hdmi_init;
use dp_driver::dp_init;
use network_drivers::network_scan;
use crate::window::{create_window, start_gui_loop};
use crate::window::Window;
const MAX_WINDOWS: usize = 64;
static mut WINDOWS: [Window; MAX_WINDOWS] = [Window::default(); MAX_WINDOWS];

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

fn clear_screen() {
    let color = vga_entry_color(VgaColor::White, VgaColor::Black);
    let blank = vga_entry(b' ', color);

    for i in 0..(VGA_WIDTH * VGA_HEIGHT) {
        unsafe {
            ptr::write_volatile(VGA_BUFFER.add(i), blank);
        }
    }
}

fn print_string(s: &str) {
    static mut CURSOR_POSITION: usize = 0;
    let color = vga_entry_color(VgaColor::White, VgaColor::Black);

    for byte in s.bytes() {
        if unsafe { CURSOR_POSITION } >= VGA_WIDTH * VGA_HEIGHT {
            return;
        }

        unsafe {
            ptr::write_volatile(VGA_BUFFER.add(CURSOR_POSITION), vga_entry(byte, color));
            CURSOR_POSITION += 1;
        }
    }
}

fn print_nl() {
    static mut CURSOR_POSITION: usize = 0;

    unsafe {
        let line_number = CURSOR_POSITION / VGA_WIDTH;
        CURSOR_POSITION = (line_number + 1) * VGA_WIDTH;
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
fn kernel_main() -> ! {
    clear_screen();
    print_string("Kernel Booting...\n\n");
    print_string("Initializing VGA driver...\n");
    vga_init();
    print_string("Initializing HDMI driver...\n");
    hdmi_init();
    print_string("Initializing DisplayPort driver...\n");
    dp_init();
    print_string("Initializing Network driver...\n");
    network_scan();
    print_string("Creating GUI Window...\n");
    create_window(20, 20, 200, 150, 0x888888, "System Monitor");
    create_window(250, 80, 300, 200, 0x222222, "Network Console");
    start_gui_loop();
}