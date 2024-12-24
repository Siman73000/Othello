#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::naked_asm;

#[no_mangle]
#[naked]
fn port_byte_out(port: u32, data: u8) {
    unsafe {
        naked_asm!("out dx, al", in("dx") port, in("al") data);
    }
}

#[no_mangle]
#[naked]
fn port_byte_in(port: u32) -> u8 {
    let data: u8;
    unsafe {
        naked_asm!("in al, dx", out("al") data, in("dx") port);
    }
    data
}

#[no_mangle]
#[naked]
fn port_word_in(port: u32) -> u16 {
    let data: u16;
    unsafe {
        naked_asm!("in ax, dx", out("ax") data, in("dx") port);
    }
    data
}

#[no_mangle]
#[naked]
fn port_word_out(port: u32, data: u16) {
    unsafe {
        naked_asm!("out dx, ax", in("dx") port, in("ax") data);
    }
}

const VIDEO_ADDRESS: u32 = 0xb8000;
const MAX_ROWS: u32 = 25;
const MAX_COLS: u32 = 80;
const WHITE_ON_BLACK: u8 = 0x0f;
const REG_SCREEN_CTRL: u32 = 0x3d4;
const REG_SCREEN_DATA: u32 = 0x3d5;

fn set_cursor(offset: u32) {
    let offset = offset / 2;
    unsafe {
        port_byte_out(REG_SCREEN_CTRL, 14);
        port_byte_out(REG_SCREEN_DATA, (offset >> 8) as u8);
        port_byte_out(REG_SCREEN_CTRL, 15);
        port_byte_out(REG_SCREEN_DATA, (offset & 0xff) as u8);
    }
}

fn get_cursor() -> u32 {
    let mut offset: u32;
    unsafe {
        port_byte_out(REG_SCREEN_CTRL, 14);
        offset = (port_byte_in(REG_SCREEN_DATA) as u32) << 8;
        port_byte_out(REG_SCREEN_CTRL, 15);
        offset += port_byte_in(REG_SCREEN_DATA) as u32;
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
        core::ptr::copy_nonoverlapping(src.add(bytes_per_row as usize), dst, (MAX_ROWS - 1) as usize * bytes_per_row as usize);
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
