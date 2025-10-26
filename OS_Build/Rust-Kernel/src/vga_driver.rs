const VID_MEM: *mut u8 = 0xb8000 as *mut u8;
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;

static mut CURSOR_POS: usize = 0;

pub fn vga_init() {
    clear_screen();
    print_string("VGA Driver Initialized", 0x0F);
}

pub fn clear_screen() {
    let blank: u16 = (b' ' as u16) | ((0x0F as u16) << 8); // Blank with white on black
    for i in 0..(VGA_WIDTH * VGA_HEIGHT) {
        unsafe {
            *((VID_MEM as *mut u16).add(i)) = blank;
        }
    }
    unsafe {
        CURSOR_POS = 0;
    }
}

pub fn print_string(s: &str, color: u8) {
    for byte in s.bytes() {
        put_char(byte, color);
    }
}

fn put_char(c: u8, color: u8) {
    if c == b'\n' {
        unsafe {
            CURSOR_POS += VGA_WIDTH - (CURSOR_POS % VGA_WIDTH);
        }
    } else {
        unsafe {
            let offset = CURSOR_POS * 2;
            *VID_MEM.add(offset) = c;
            *VID_MEM.add(offset + 1) = color;
            CURSOR_POS += 1;
        }
    }

    if unsafe { CURSOR_POS } >= VGA_WIDTH * VGA_HEIGHT {
        scroll_screen();
    }
}

fn scroll_screen() {
    for row in 1..VGA_HEIGHT {
        for col in 0..VGA_WIDTH {
            let from_offset = (row * VGA_WIDTH + col) * 2;
            let to_offset = ((row - 1) * VGA_WIDTH + col) * 2;
            unsafe {
                *VID_MEM.add(to_offset) = *VID_MEM.add(from_offset);
                *VID_MEM.add(to_offset + 1) = *VID_MEM.add(from_offset + 1);
            }
        }
    }

    // Clear the last row
    let blank: u16 = (b' ' as u16) | ((0x0F as u16) << 8);
    for col in 0..VGA_WIDTH {
        unsafe {
            *((VID_MEM as *mut u16).add((VGA_HEIGHT - 1) * VGA_WIDTH + col)) = blank;
        }
    }

    unsafe {
        CURSOR_POS = (VGA_HEIGHT - 1) * VGA_WIDTH;
    }
}
