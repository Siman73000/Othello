#![no_std]
#![no_main]

mod display;
mod network_drivers;
mod vga_driver;
mod dp_driver;
mod hdmi_driver;
mod window;
mod framebuffer_driver;

use core::ptr;
use vga_driver::vga_init;
use hdmi_driver::hdmi_init;
use dp_driver::dp_init;
use network_drivers::network_scan;
use crate::window::{create_window, start_gui_loop, Window};
use core::mem::MaybeUninit;

use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub fn init_heap(heap_start: usize, heap_size: usize) {
    unsafe { ALLOCATOR.lock().init(heap_start as *mut u8, heap_size) }
}

// ======================= Constants and Globals =======================
pub const MAX_WINDOWS: usize = 16;
static mut WINDOWS: [MaybeUninit<Window>; MAX_WINDOWS] = unsafe { MaybeUninit::uninit().assume_init() };

const VGA_BUFFER: *mut u16 = 0xb8000 as *mut u16;
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;

const HEAP_START: usize = 0x_4444_0000;
const HEAP_SIZE: usize = 1024 * 1024; // 1 MB heap

// Static framebuffers for GUI windows
static mut SYSTEM_MONITOR_BUFFER: [u32; 200 * 150] = [0; 200 * 150];
static mut NETWORK_CONSOLE_BUFFER: [u32; 300 * 200] = [0; 300 * 200];

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

// ======================= VGA Text Functions =======================
fn clear_screen() {
    let color = vga_entry_color(VgaColor::White, VgaColor::Black);
    let blank = vga_entry(b' ', color);

    for i in 0..(VGA_WIDTH * VGA_HEIGHT) {
        unsafe { ptr::write_volatile(VGA_BUFFER.add(i), blank) }
    }
}

fn print_string(s: &str) {
    static mut CURSOR_POSITION: usize = 0;
    let color = vga_entry_color(VgaColor::White, VgaColor::Black);
    for byte in s.bytes() {
        unsafe {
            if byte == b'\n' {
                let line = CURSOR_POSITION / VGA_WIDTH;
                CURSOR_POSITION = (line + 1) * VGA_WIDTH;
            } else {
                ptr::write_volatile(VGA_BUFFER.add(CURSOR_POSITION), vga_entry(byte, color));
                CURSOR_POSITION += 1;
            }
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

// ======================= Panic Handler =======================
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

// ======================= Kernel Entry =======================
#[no_mangle]
pub extern "C" fn kernel_main() -> ! {
    // Initialize heap first
    init_heap(HEAP_START, HEAP_SIZE);

    // Clear VGA screen
    clear_screen();
    print_string("Kernel Booting...\n\n");

    // Initialize drivers
    print_string("Initializing VGA driver...\n");
    vga_init();
    print_string("Initializing HDMI driver...\n");
    hdmi_init();
    print_string("Initializing DisplayPort driver...\n");
    dp_init();
    print_string("Initializing Network driver...\n");
    network_scan();

    // Initialize GUI windows
    print_string("Creating GUI Window...\n");

    unsafe {
        create_window(20, 20, 200, 150, 0x888888, "System Monitor", &mut SYSTEM_MONITOR_BUFFER);
        create_window(250, 80, 300, 200, 0x222222, "Network Console", &mut NETWORK_CONSOLE_BUFFER);
    }

    // Start GUI loop
    start_gui_loop();
}
