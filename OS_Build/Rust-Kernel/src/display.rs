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

const FRAMEBUFFER_BASE: usize = 0xA000_0000;

static mut FRAMEBUFFER: Option<&mut [u8]> = None;

// ======================= Framebuffer & Display Types ========================

pub enum DisplayType {
    VGA,
    Framebuffer,
    HDMI,
    DisplayPort,
    Unknown,
}

pub struct FrameBufferInfo {
    pub base_addr: usize,
    pub width: usize,
    pub height: usize,
    pub bytes_per_pixel: usize,
}

// ======================= Startup / Detect ========================

pub fn _start() -> ! {
    let display_type = detect_display();

    match display_type {
        DisplayType::VGA => vga_driver::init(),
        DisplayType::Framebuffer => framebuffer_driver::init(),
        DisplayType::HDMI => hdmi_driver::init(),
        DisplayType::DisplayPort => dp_driver::init(),
        DisplayType::Unknown => panic!("Unknown display type."),
    }

    loop {}
}

fn detect_display() -> DisplayType {
    // query UEFI GOP or BIOS for framebuffer
    if let Some(_) = query_framebuffer() {
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

// ======================= Query / Hardware ========================

fn query_framebuffer() -> Option<FrameBufferInfo> {
    None
}

fn query_pci_for_hdmi() -> bool {
    let mut hdmi_port = Port::new(0x3c0);
    unsafe {
        hdmi_port.write(0x00);
        let status = hdmi_port.read();
        (status & 0x04) != 0
    }
}

fn query_pci_for_vga() -> bool {
    let mut vga_port = Port::new(0x3c0);
    unsafe {
        vga_port.write(0x00);
        let status = vga_port.read();
        (status & 0x01) != 0
    }
}

fn query_pci_for_dp() -> bool {
    let mut dp_port = Port::new(0x3c0);
    unsafe {
        dp_port.write(0x00);
        let status = dp_port.read();
        (status & 0x02) != 0
    }
}

// ======================= Cursor ========================

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

// ======================= VGA Video Memory ========================

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

// ======================= Print Functions ========================

pub fn print_string(string: &str) {
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

pub fn print_nl() {
    let mut new_offset = move_offset_to_new_line(get_cursor());
    if new_offset >= MAX_ROWS * MAX_COLS * 2 {
        new_offset = scroll_ln(new_offset);
    }
    set_cursor(new_offset);
}

pub fn clear_screen() {
    let screen_size = MAX_COLS * MAX_ROWS;
    for i in 0..screen_size {
        set_char_at_video_memory(' ', i * 2);
    }
    set_cursor(get_offset(0, 0));
}

// ======================= Framebuffer Functions ========================

pub fn allocate_framebuffer(size: usize) {
    unsafe {
        FRAMEBUFFER = Some(core::slice::from_raw_parts_mut(FRAMEBUFFER_BASE as *mut u8, size));
    }
}

pub fn set_pixel(x: usize, y: usize, color: u32, width: usize) {
    unsafe {
        if let Some(buffer) = FRAMEBUFFER.as_mut() {
            let offset = (y * width + x) * 4;
            if offset + 3 < buffer.len() {
                buffer[offset..offset + 4].copy_from_slice(&color.to_le_bytes());
            }
        }
    }
}

pub fn clear_screen_fb(color: u32, width: usize, height: usize) {
    unsafe {
        if let Some(buffer) = FRAMEBUFFER.as_mut() {
            for y in 0..height {
                for x in 0..width {
                    set_pixel(x, y, color, width);
                }
            }
        }
    }
}

pub fn commit_framebuffer() {
    // For now, framebuffer writes are directly memory-mapped; no extra commit needed.
}