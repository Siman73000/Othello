use core::ptr::{copy, write_volatile};
use spin::Mutex;
use x86_64::instructions::port::PortWriteOnly;

const VID_MEM: *mut u8 = 0xb8000 as *mut u8;
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;
const VGA_BYTES_PER_CELL: usize = 2;
const REG_SCREEN_CTRL: u16 = 0x3d4;
const REG_SCREEN_DATA: u16 = 0x3d5;

static VGA_STATE: Mutex<VgaState> = Mutex::new(VgaState::new());

#[derive(Copy, Clone)]
struct VgaState {
    cursor_pos: usize,
    default_color: u8,
    hardware_cursor: bool,
}

impl VgaState {
    const fn new() -> Self {
        VgaState {
            cursor_pos: 0,
            default_color: 0x0F, // white on black
            hardware_cursor: true,
        }
    }

    fn reset(&mut self) {
        self.clear_with_color(self.default_color);
        self.cursor_pos = 0;
        self.sync_cursor();
    }

    fn clear_with_color(&mut self, color: u8) {
        let safe_color = sanitize_color(color);
        let blank: u16 = (b' ' as u16) | ((safe_color as u16) << 8);
        for i in 0..(VGA_WIDTH * VGA_HEIGHT) {
            unsafe {
                write_volatile((VID_MEM as *mut u16).add(i), blank);
            }
        }
        self.cursor_pos = 0;
    }

    fn write_byte(&mut self, byte: u8, color: u8) {
        if byte == b'\n' {
            let line = self.cursor_pos / VGA_WIDTH;
            self.cursor_pos = (line + 1) * VGA_WIDTH;
            if self.cursor_pos >= VGA_WIDTH * VGA_HEIGHT {
                self.scroll();
            }
            return;
        }

        if self.cursor_pos >= VGA_WIDTH * VGA_HEIGHT {
            self.scroll();
        }

        let safe_color = sanitize_color(color);
        let offset = self.cursor_pos * VGA_BYTES_PER_CELL;
        unsafe {
            write_volatile(VID_MEM.add(offset), byte);
            write_volatile(VID_MEM.add(offset + 1), safe_color);
        }
        self.cursor_pos += 1;
    }

    fn write_str(&mut self, s: &str, color: u8) {
        for byte in s.bytes() {
            self.write_byte(byte, color);
        }
        self.sync_cursor();
    }

    fn scroll(&mut self) {
        // Move rows 1..=height-1 up by one row
        let row_bytes = VGA_WIDTH * VGA_BYTES_PER_CELL;
        let copy_bytes = (VGA_HEIGHT - 1) * row_bytes;
        unsafe { copy(VID_MEM.add(row_bytes), VID_MEM, copy_bytes) }

        // Clear the last row
        let blank: u16 = (b' ' as u16) | ((sanitize_color(self.default_color) as u16) << 8);
        for col in 0..VGA_WIDTH {
            unsafe {
                write_volatile(
                    (VID_MEM as *mut u16).add((VGA_HEIGHT - 1) * VGA_WIDTH + col),
                    blank,
                );
            }
        }

        // Position cursor at start of last line
        self.cursor_pos = (VGA_HEIGHT - 1) * VGA_WIDTH;
    }

    fn sync_cursor(&self) {
        if !self.hardware_cursor {
            return;
        }

        let offset = (self.cursor_pos as u32) as u32;
        let mut screen_ctrl: PortWriteOnly<u8> = PortWriteOnly::new(REG_SCREEN_CTRL);
        let mut screen_data: PortWriteOnly<u8> = PortWriteOnly::new(REG_SCREEN_DATA);

        unsafe {
            screen_ctrl.write(14);
            screen_data.write((offset >> 8) as u8);
            screen_ctrl.write(15);
            screen_data.write((offset & 0xff) as u8);
        }
    }
}

fn sanitize_color(color: u8) -> u8 {
    // VGA text mode uses two 4-bit channels; strip any undefined upper bits
    color & 0x7F
}

// ======================= Public API ========================

pub fn vga_init() {
    let mut state = VGA_STATE.lock();
    let color = state.default_color;
    state.reset();
    state.write_str("VGA Driver Initialized", color);
}

pub fn clear_screen() {
    let mut state = VGA_STATE.lock();
    let color = state.default_color;
    state.clear_with_color(color);
    state.sync_cursor();
}

pub fn clear_with_color(color: u8) {
    let mut state = VGA_STATE.lock();
    state.clear_with_color(color);
    state.sync_cursor();
}

pub fn print_string(s: &str, color: u8) {
    VGA_STATE.lock().write_str(s, color);
}

pub fn print_line(s: &str, color: u8) {
    let mut guard = VGA_STATE.lock();
    guard.write_str(s, color);
    guard.write_byte(b'\n', color);
    guard.sync_cursor();
}
