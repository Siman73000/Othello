//! PS/2 mouse driver with software cursor (no trails).

#![allow(dead_code)]

use core::arch::asm;
use crate::gui::{fb_get_pixel, fb_put_pixel, fb_width, fb_height};

const PS2_STATUS: u16 = 0x64;
const PS2_CMD: u16    = 0x64;
const PS2_DATA: u16   = 0x60;

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!(
        "in al, dx",
        out("al") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );
    value
}

#[inline]
unsafe fn outb(port: u16, value: u8) {
    asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags)
    );
}

unsafe fn ps2_wait_write() {
    loop {
        if inb(PS2_STATUS) & 0x02 == 0 {
            break;
        }
    }
}

unsafe fn ps2_wait_read() {
    loop {
        if inb(PS2_STATUS) & 0x01 != 0 {
            break;
        }
    }
}

unsafe fn mouse_write(byte: u8) {
    ps2_wait_write();
    outb(PS2_CMD, 0xD4);   // next write goes to mouse
    ps2_wait_write();
    outb(PS2_DATA, byte);
}

unsafe fn mouse_read() -> u8 {
    ps2_wait_read();
    inb(PS2_DATA)
}

unsafe fn ps2_enable_mouse() {
    // Enable auxiliary device
    ps2_wait_write();
    outb(PS2_CMD, 0xA8);

    // Read command byte
    ps2_wait_write();
    outb(PS2_CMD, 0x20);
    ps2_wait_read();
    let mut status = inb(PS2_DATA);
    status |= 0x02; // keyboard IRQ
    status |= 0x04; // mouse IRQ

    // Write command byte
    ps2_wait_write();
    outb(PS2_CMD, 0x60);
    ps2_wait_write();
    outb(PS2_DATA, status);

    // Reset to defaults
    mouse_write(0xF6);
    let _ = mouse_read(); // ACK

    // Enable streaming
    mouse_write(0xF4);
    let _ = mouse_read(); // ACK
}

// -----------------------------------------------------------------------------
// Cursor state
// -----------------------------------------------------------------------------

const CURSOR_W: usize = 8;
const CURSOR_H: usize = 8;

static mut MOUSE_X: usize = 32;
static mut MOUSE_Y: usize = 32;
static mut BUTTONS: u8    = 0;

static mut PACKET: [u8; 3] = [0; 3];
static mut PACKET_IDX: u8  = 0;

static mut CURSOR_VISIBLE: bool = false;
static mut CURSOR_BG: [[u8; CURSOR_W]; CURSOR_H] = [[0; CURSOR_W]; CURSOR_H];

const CURSOR_SHAPE: [[u8; CURSOR_W]; CURSOR_H] = [
    [1,0,0,0,0,0,0,0],
    [1,1,0,0,0,0,0,0],
    [1,1,1,0,0,0,0,0],
    [1,1,1,1,0,0,0,0],
    [1,1,1,1,1,0,0,0],
    [1,1,0,1,0,0,0,0],
    [0,0,0,0,0,0,0,0],
    [0,0,0,0,0,0,0,0],
];

unsafe fn cursor_erase() {
    if !CURSOR_VISIBLE {
        return;
    }

    let fw = fb_width();
    let fh = fb_height();

    if MOUSE_X + CURSOR_W > fw || MOUSE_Y + CURSOR_H > fh {
        CURSOR_VISIBLE = false;
        return;
    }

    for y in 0..CURSOR_H {
        for x in 0..CURSOR_W {
            let px = MOUSE_X + x;
            let py = MOUSE_Y + y;
            let c = CURSOR_BG[y][x];
            fb_put_pixel(px, py, c);
        }
    }

    CURSOR_VISIBLE = false;
}

unsafe fn cursor_draw() {
    let fw = fb_width();
    let fh = fb_height();

    if fw == 0 || fh == 0 {
        return;
    }

    if MOUSE_X + CURSOR_W > fw || MOUSE_Y + CURSOR_H > fh {
        return;
    }

    for y in 0..CURSOR_H {
        for x in 0..CURSOR_W {
            let px = MOUSE_X + x;
            let py = MOUSE_Y + y;

            CURSOR_BG[y][x] = fb_get_pixel(px, py);

            if CURSOR_SHAPE[y][x] != 0 {
                fb_put_pixel(px, py, 0x3F); // bright cursor
            }
        }
    }

    CURSOR_VISIBLE = true;
}

pub fn mouse_init() {
    unsafe {
        ps2_enable_mouse();

        let fw = fb_width();
        let fh = fb_height();
        if fw > CURSOR_W && fh > CURSOR_H {
            MOUSE_X = (fw - CURSOR_W) / 2;
            MOUSE_Y = (fh - CURSOR_H) / 2;
        }

        cursor_draw();
    }
}

/// Poll for mouse packets. Call frequently from your main loop.
pub fn mouse_poll() {
    unsafe {
        let status = inb(PS2_STATUS);

        // Output buffer empty?
        if status & 0x01 == 0 {
            return;
        }

        // AUX not set => keyboard byte, ignore here
        if status & 0x20 == 0 {
            return;
        }

        let byte = inb(PS2_DATA);

        PACKET[PACKET_IDX as usize] = byte;
        PACKET_IDX += 1;
        if PACKET_IDX < 3 {
            return;
        }
        PACKET_IDX = 0;

        let p0 = PACKET[0];
        let p1 = PACKET[1] as i8;
        let p2 = PACKET[2] as i8;

        // Sync: bit 3 should be set
        if p0 & 0x08 == 0 {
            return;
        }

        let dx = p1 as isize;
        let dy = -(p2 as isize);

        let fw = fb_width() as isize;
        let fh = fb_height() as isize;
        if fw <= 0 || fh <= 0 {
            return;
        }

        let max_x = (fw - CURSOR_W as isize).max(0);
        let max_y = (fh - CURSOR_H as isize).max(0);

        let mut new_x = MOUSE_X as isize + dx;
        let mut new_y = MOUSE_Y as isize + dy;

        if new_x < 0 {
            new_x = 0;
        }
        if new_y < 0 {
            new_y = 0;
        }
        if new_x > max_x {
            new_x = max_x;
        }
        if new_y > max_y {
            new_y = max_y;
        }

        if new_x as usize != MOUSE_X || new_y as usize != MOUSE_Y {
            cursor_erase();
            MOUSE_X = new_x as usize;
            MOUSE_Y = new_y as usize;
            cursor_draw();
        }

        BUTTONS = p0 & 0x07; // L,R,M
    }
}

pub fn mouse_position() -> (usize, usize) {
    unsafe { (MOUSE_X, MOUSE_Y) }
}

pub fn mouse_buttons() -> u8 {
    unsafe { BUTTONS }
}
