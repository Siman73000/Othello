#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

// =============================
// Constants
// =============================

const VGA_BUFFER: *mut u8 = 0xb8000 as *mut u8;
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;

const COM1_PORT: u16 = 0x3F8;

// Minimal banner ASCII (you can swap this for your big art later)
const ASCII_ART: &str = r#"
  ______   .___________. __    __   _______  __       __        ______   
 /  __  \  |           ||  |  |  | |   ____||  |     |  |      /  __  \  
|  |  |  | `---|  |----`|  |__|  | |  |__   |  |     |  |     |  |  |  | 
|  |  |  |     |  |     |   __   | |   __|  |  |     |  |     |  |  |  | 
|  `--'  |     |  |     |  |  |  | |  |____ |  `----.|  `----.|  `--'  | 
 \______/      |__|     |__|  |__| |_______||_______||_______| \______/  
                                                                         
   OTHELLO KERNEL MONITOR
"#;

// =============================
// Panic handler
// =============================

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    unsafe {
        serial_init();
        vga_clear(0x4F); // white on red
        vga_write_str(0, 0, "KERNEL PANIC", 0x4F);
        serial_write_str("KERNEL PANIC\r\n");
    }

    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

// =============================
// Entry point
// =============================

#[no_mangle]
#[link_section = ".text._start"]
pub extern "C" fn _start() -> ! {
    unsafe { kernel_main() }
}

unsafe fn kernel_main() -> ! {
    // Bring up serial first so we can debug everything else.
    serial_init();
    serial_write_str("Othello kernel: _start reached (long mode).\r\n");

    // VGA setup
    vga_clear(0x07); // gray on black
    vga_draw_header();
    vga_draw_ascii_art();

    serial_write_str("VGA banner drawn.\r\n");

    // Tiny “shell” area near the bottom
    let mut row: usize = 16;
    let mut col: usize = 0;

    const PROMPT: &str = "Othello shell> ";
    vga_write_str(row, 0, PROMPT, 0x0A); // green-ish
    col = PROMPT.len();

    serial_write_str("Entering keyboard poll loop...\r\n");

    loop {
        if let Some(scancode) = keyboard_read_scancode() {
            // Ignore break codes (key release)
            if (scancode & 0x80) != 0 {
                continue;
            }

            match scancode {
                0x1C => {
                    // Enter
                    serial_write_str("\r\n");
                    row += 1;
                    col = 0;
                    if row >= VGA_HEIGHT {
                        row = 16; // wrap to shell area
                        vga_clear_region(row, VGA_HEIGHT - 1, 0x07);
                    }
                    vga_write_str(row, 0, PROMPT, 0x0A);
                    col = PROMPT.len();
                }
                0x0E => {
                    // Backspace
                    let prompt_len = PROMPT.len();
                    if col > prompt_len {
                        col -= 1;
                        vga_write_char(row, col, b' ', 0x07);
                        // crude backspace on serial: BS, space, BS
                        serial_write_byte(0x08);
                        serial_write_byte(b' ');
                        serial_write_byte(0x08);
                    }
                }
                _ => {
                    if let Some(ch) = scancode_to_ascii(scancode) {
                        if col >= VGA_WIDTH {
                            row += 1;
                            col = 0;
                            if row >= VGA_HEIGHT {
                                row = 16;
                                vga_clear_region(row, VGA_HEIGHT - 1, 0x07);
                            }
                            vga_write_str(row, 0, PROMPT, 0x0A);
                            col = PROMPT.len();
                        }
                        vga_write_char(row, col, ch, 0x0F);
                        col += 1;
                        serial_write_byte(ch);
                    }
                }
            }
        }
        // simple busy poll; you can add a tiny delay or hlt later
    }
}

// =============================
// VGA helpers
// =============================

unsafe fn vga_clear(attr: u8) {
    let mut offset = 0usize;
    for _row in 0..VGA_HEIGHT {
        for _col in 0..VGA_WIDTH {
            core::ptr::write_volatile(VGA_BUFFER.add(offset), b' ');
            core::ptr::write_volatile(VGA_BUFFER.add(offset + 1), attr);
            offset += 2;
        }
    }
}

// Clear a subset of rows (inclusive)
unsafe fn vga_clear_region(start_row: usize, end_row: usize, attr: u8) {
    for row in start_row..=end_row {
        if row >= VGA_HEIGHT {
            break;
        }
        for col in 0..VGA_WIDTH {
            let idx = 2 * (row * VGA_WIDTH + col);
            core::ptr::write_volatile(VGA_BUFFER.add(idx), b' ');
            core::ptr::write_volatile(VGA_BUFFER.add(idx + 1), attr);
        }
    }
}

unsafe fn vga_write_char(row: usize, col: usize, byte: u8, attr: u8) {
    if row >= VGA_HEIGHT || col >= VGA_WIDTH {
        return;
    }
    let idx = 2 * (row * VGA_WIDTH + col);
    core::ptr::write_volatile(VGA_BUFFER.add(idx), byte);
    core::ptr::write_volatile(VGA_BUFFER.add(idx + 1), attr);
}

unsafe fn vga_write_str(mut row: usize, mut col: usize, s: &str, attr: u8) {
    for &b in s.as_bytes() {
        match b {
            b'\n' => {
                row += 1;
                col = 0;
                if row >= VGA_HEIGHT {
                    break;
                }
            }
            _ => {
                if col >= VGA_WIDTH {
                    row += 1;
                    col = 0;
                    if row >= VGA_HEIGHT {
                        break;
                    }
                }
                vga_write_char(row, col, b, attr);
                col += 1;
            }
        }
    }
}

unsafe fn vga_draw_header() {
    let header = " Othello RTOS – Bare-metal x86_64 long-mode kernel ";
    let attr = 0x1F; // bright white on blue

    // Fill first row with blue
    for col in 0..VGA_WIDTH {
        vga_write_char(0, col, b' ', attr);
    }

    // Center the header text
    let len = header.len();
    let start_col = if len >= VGA_WIDTH {
        0
    } else {
        (VGA_WIDTH - len) / 2
    };

    vga_write_str(0, start_col, header, attr);
}

unsafe fn vga_draw_ascii_art() {
    let mut row = 2; // leave row 0 for header, row 1 as spacer

    for line in ASCII_ART.lines() {
        if row >= VGA_HEIGHT {
            break;
        }

        let len = line.len();
        let start_col = if len >= VGA_WIDTH {
            0
        } else {
            (VGA_WIDTH - len) / 2
        };

        vga_write_str(row, start_col, line, 0x0F); // bright white
        row += 1;
    }
}

// =============================
// Serial (COM1) helpers
// =============================

unsafe fn outb(port: u16, val: u8) {
    asm!(
        "out dx, al",
        in("dx") port,
        in("al") val,
        options(nostack, preserves_flags),
    );
}

unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    asm!(
        "in al, dx",
        in("dx") port,
        out("al") val,
        options(nostack, preserves_flags),
    );
    val
}

unsafe fn serial_init() {
    // Disable interrupts
    outb(COM1_PORT + 1, 0x00);

    // Enable DLAB
    outb(COM1_PORT + 3, 0x80);

    // Baud divisor = 3 (38,400 baud if base is 115200*16)
    outb(COM1_PORT + 0, 0x03); // low byte
    outb(COM1_PORT + 1, 0x00); // high byte

    // 8 bits, no parity, one stop bit
    outb(COM1_PORT + 3, 0x03);

    // Enable FIFO, clear them, 14-byte threshold
    outb(COM1_PORT + 2, 0xC7);

    // IRQs enabled, RTS/DSR set
    outb(COM1_PORT + 4, 0x0B);
}

unsafe fn serial_can_transmit() -> bool {
    // Line Status Register (LSR) bit 5 = THR empty
    (inb(COM1_PORT + 5) & 0x20) != 0
}

unsafe fn serial_write_byte(b: u8) {
    while !serial_can_transmit() {
        asm!("pause", options(nomem, nostack, preserves_flags));
    }
    outb(COM1_PORT, b);
}

unsafe fn serial_write_str(s: &str) {
    for &b in s.as_bytes() {
        if b == b'\n' {
            serial_write_byte(b'\r');
        }
        serial_write_byte(b);
    }
}

// =============================
// Keyboard (polled PS/2) helpers
// =============================

// Non-blocking: returns Some(scancode) if data is waiting.
unsafe fn keyboard_read_scancode() -> Option<u8> {
    // Status port 0x64, bit 0 = output buffer full
    let status = inb(0x64);
    if (status & 0x01) == 0 {
        return None;
    }
    Some(inb(0x60))
}

// Very small scancode set 1 → ASCII mapping (US keyboard, lowercase)
fn scancode_to_ascii(sc: u8) -> Option<u8> {
    match sc {
        // Number row
        0x02 => Some(b'1'),
        0x03 => Some(b'2'),
        0x04 => Some(b'3'),
        0x05 => Some(b'4'),
        0x06 => Some(b'5'),
        0x07 => Some(b'6'),
        0x08 => Some(b'7'),
        0x09 => Some(b'8'),
        0x0A => Some(b'9'),
        0x0B => Some(b'0'),

        // Top row qwertyuiop
        0x10 => Some(b'q'),
        0x11 => Some(b'w'),
        0x12 => Some(b'e'),
        0x13 => Some(b'r'),
        0x14 => Some(b't'),
        0x15 => Some(b'y'),
        0x16 => Some(b'u'),
        0x17 => Some(b'i'),
        0x18 => Some(b'o'),
        0x19 => Some(b'p'),

        // Home row asdfghjkl
        0x1E => Some(b'a'),
        0x1F => Some(b's'),
        0x20 => Some(b'd'),
        0x21 => Some(b'f'),
        0x22 => Some(b'g'),
        0x23 => Some(b'h'),
        0x24 => Some(b'j'),
        0x25 => Some(b'k'),
        0x26 => Some(b'l'),

        // Bottom row zxcvbnm
        0x2C => Some(b'z'),
        0x2D => Some(b'x'),
        0x2E => Some(b'c'),
        0x2F => Some(b'v'),
        0x30 => Some(b'b'),
        0x31 => Some(b'n'),
        0x32 => Some(b'm'),

        // Space
        0x39 => Some(b' '),

        _ => None,
    }
}
