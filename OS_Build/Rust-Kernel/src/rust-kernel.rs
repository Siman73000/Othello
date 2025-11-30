#![no_std]
#![no_main]

mod display;
mod network_drivers;
mod vga_driver;
mod dp_driver;
mod hdmi_driver;
mod window;
mod security;
mod framebuffer_driver;

use core::ptr;
use vga_driver::vga_init;
use hdmi_driver::{configure_from_firmware as configure_hdmi_from_firmware, hdmi_init, HdmiDescriptor, HdmiRegisters};
use dp_driver::{configure_from_firmware as configure_dp_from_firmware, dp_init, DpDescriptor, DpRegisters};
use network_drivers::network_scan;
use crate::window::{create_window, start_gui_loop, Window};
use core::mem::MaybeUninit;
use crate::framebuffer_driver::{init_from_boot_info, BootFramebuffer};
use crate::security::initialize_security;
use core::sync::atomic::{AtomicBool, Ordering};
use x86_64::instructions::hlt;

use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

static HEAP_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn init_heap(heap_start: usize, heap_size: usize) {
    if HEAP_INITIALIZED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        // Already initialized; avoid a second mutable borrow of the heap region.
        return;
    }

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
static mut SHELL_BUFFER: [u32; 360 * 220] = [0; 360 * 220];

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

fn firmware_framebuffer_descriptor() -> BootFramebuffer {
    BootFramebuffer {
        base_addr: 0xA000_0000,
        width: 1024,
        height: 768,
        pitch: 1024 * 4,
        bytes_per_pixel: 4,
    }
}

fn firmware_hdmi_descriptor() -> HdmiDescriptor {
    HdmiDescriptor {
        framebuffer_base: 0xA000_0000,
        registers: HdmiRegisters {
            clock_base: 0x4000_0200,
            phy_base: 0x4000_0300,
            audio_base: 0x4000_0100,
            debug_base: 0x4000_0400,
            hotplug_reg: 0x4000_0404,
            pixel_format_reg: 0x4000_0410,
        },
        edid_block: None,
    }
}

fn firmware_dp_descriptor() -> DpDescriptor {
    DpDescriptor {
        framebuffer_base: 0xA000_0000,
        registers: DpRegisters {
            clock_base: 0x4000_0500,
            phy_base: 0x4000_0600,
            audio_base: 0x4000_0100,
            hotplug_reg: 0x4000_0640,
            pixel_format_reg: 0x4000_0644,
            debug_reg: 0x4000_0648,
        },
        edid_block: None,
    }
}

// ======================= Panic Handler =======================
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        hlt();
    }
}

// ======================= Kernel Entry =======================
#[no_mangle]
pub extern "C" fn kernel_main() -> ! {
    // Initialize heap first
    init_heap(HEAP_START, HEAP_SIZE);

    // Reset security/registry state so we never boot with stale secrets.
    initialize_security();

    // Clear VGA screen
    clear_screen();
    print_string("Kernel Booting...\n\n");

    // Apply firmware-provided display descriptors
    let _ = init_from_boot_info(firmware_framebuffer_descriptor());
    configure_hdmi_from_firmware(firmware_hdmi_descriptor());
    configure_dp_from_firmware(firmware_dp_descriptor());

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
        let monitor = create_window(24, 24, 260, 180, 0x10131f, "System Monitor", &mut SYSTEM_MONITOR_BUFFER);
        if let Ok(id) = monitor {
            window::paint_system_monitor(id);
        }

        let net = create_window(320, 48, 300, 200, 0x0f172a, "Network Console", &mut NETWORK_CONSOLE_BUFFER);
        if let Ok(id) = net {
            window::paint_network_console(id);
        }

        let shell = create_window(120, 240, 360, 220, 0x0b1220, "Othello Shell", &mut SHELL_BUFFER);
        if let Ok(id) = shell {
            window::paint_shell(id);
        }
    }

    // Start GUI loop
    start_gui_loop();
}
