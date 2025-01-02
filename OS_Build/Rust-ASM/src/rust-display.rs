#![no_std]
#![no_main]

use core::panic::PanicInfo;
use x86_64::instructions::port::{Port, PortWriteOnly};

const VIDEO_ADDRESS: u32 = 0xb8000;
const MAX_ROWS: u32 = 25;
const MAX_COLS: u32 = 80;
const WHITE_ON_BLACK: u8 = 0x0f;
const REG_SCREEN_CTRL: u16 = 0x3d4;
const REG_SCREEN_DATA: u16 = 0x3d5;

enum DisplayType {
    VGA,
    Framebuffer,
    HDMI,
    DisplayPort,
    Unknown,
}

pub fn _start() -> ! {
    let display_type = detect_display();

    match display_type {
        DisplayType::VGA => vga_driver::init(),
        DisplayType::Framebuffer => framebuffer_driver::init(),
        DisplayType::HDMI => hdmi-driver::init(),
        DisplayType::DisplayPort => dp_driver::init(),
        DisplayType::Unknown => panic!("Unknown display type."),
    }
    loop {}
}

fn detect_display() -> DisplayType {
    // query UEFI GOP or BIOS for framebuffer
    if let Some(framebuffer_info) = query_framebuffer() {
        return DisplayType::Framebuffer;
    }
    if query_pci_for_hdmi() {
        return DisplayType::HDMI;
    }
    if query_pci_for_dp() {
        return DisplayType::DisplayPort;
    }
    DisplayType::VGA
}

fn query_framebuffer() -> Option<FrameBufferInfo> {
    None
}

fn query_pci_for_hdmi() -> bool {
    false
}

fn query_pci_for_dp() -> bool {
    false
}

// updates hardware cursor pos
// the pos is split into high & low bytes then sent to VGA ports 0x3d4 & 0x3d5
fn set_cursor(offset: u32) {
    let offset = offset / 2;
    let mut screen_ctrl = PortWriteOnly::new(REG_SCREEN_CTRL);
    let mut screen_data = PortWriteOnly::new(REG_SCREEN_DATA);

    unsafe {
        screen_ctrl.write(14);
        screen_data.write((offset >> 8) as u8);
        screen_ctrl.write(15);
        screen_data.write((offset & 0xff) as u8);
    }
}

// gets the current cursor pos from VGA controller
// returns offset in vid mem
fn get_cursor() -> u32 {
    let mut screen_ctrl = Port::new(REG_SCREEN_CTRL);
    let mut screen_data = Port::new(REG_SCREEN_DATA);

    let mut offset: u32;
    unsafe {
        screen_ctrl.write(14);
        offset = (screen_data.read() as u32) << 8;
        screen_ctrl.write(15);
        offset += screen_data.read() as u32;
    }
    offset * 2
}

fn get_offset(col: u32, row: u32) -> u32 {
    2 * (row * MAX_COLS + col)
}

fn get_row_from_offset(offset: u32) -> u32 {
    offset / (2 * MAX_COLS)
}

fn move_offset_to_new_line(offset: u32) -> u32 {
    get_offset(0, get_row_from_offset(offset) + 1)
}

fn set_char_at_video_memory(character: char, offset: u32) {
    unsafe {
        let vidmem = VIDEO_ADDRESS as *mut u8;
        *vidmem.add(offset as usize) = character as u8;
        *vidmem.add(offset as usize + 1) = WHITE_ON_BLACK;
    }
}

fn scroll_ln(offset: u32) -> u32 {
    let bytes_per_row = MAX_COLS * 2;
    unsafe {
        let src = VIDEO_ADDRESS as *const u8;
        let dst = VIDEO_ADDRESS as *mut u8;
        core::ptr::copy_nonoverlapping(
            src.add(bytes_per_row as usize),
            dst,
            (MAX_ROWS - 1) as usize * bytes_per_row as usize,
        );
    }
    for col in 0..MAX_COLS {
        set_char_at_video_memory(' ', get_offset(col, MAX_ROWS - 1));
    }
    offset - bytes_per_row
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

fn print_string(string: &str) {
    let mut offset = get_cursor();
    for character in string.chars() {
        if offset >= MAX_ROWS * MAX_COLS * 2 {
            offset = scroll_ln(offset);
        }
        if character == '\n' {
            offset = move_offset_to_new_line(offset);
        } else {
            set_char_at_video_memory(character, offset);
            offset += 2;
        }
    }
    set_cursor(offset);
}

fn print_nl() {
    let mut new_offset = move_offset_to_new_line(get_cursor());
    if new_offset >= MAX_ROWS * MAX_COLS * 2 {
        new_offset = scroll_ln(new_offset);
    }
    set_cursor(new_offset);
}

fn clear_screen() {
    let screen_size = MAX_COLS * MAX_ROWS;
    for i in 0..screen_size {
        set_char_at_video_memory(' ', i * 2);
    }
    set_cursor(get_offset(0, 0));
}
