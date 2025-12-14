#![allow(dead_code)]
use core::arch::asm;

const PS2_STATUS: u16 = 0x64;
const PS2_CMD: u16    = 0x64;
const PS2_DATA: u16   = 0x60;

const CMD_ENABLE_AUX: u8 = 0xA8;
const CMD_READ_CCB: u8   = 0x20;
const CMD_WRITE_CCB: u8  = 0x60;
const CMD_WRITE_AUX: u8  = 0xD4;

const MOUSE_RESET_DEFAULTS: u8 = 0xF6;
const MOUSE_ENABLE_STREAM: u8  = 0xF4;
const MOUSE_SET_SAMPLE: u8     = 0xF3;
const MOUSE_GET_ID: u8         = 0xF2;

#[inline] unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
    value
}
#[inline] unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

fn wait_in_clear() { for _ in 0..100_000 { unsafe { if inb(PS2_STATUS) & 0x02 == 0 { return; } } } }
fn wait_out_full() -> bool { for _ in 0..100_000 { unsafe { if inb(PS2_STATUS) & 0x01 != 0 { return true; } } } false }

fn cmd(c: u8) { wait_in_clear(); unsafe { outb(PS2_CMD, c); } }
fn data(d: u8) { wait_in_clear(); unsafe { outb(PS2_DATA, d); } }
fn read() -> u8 { if !wait_out_full() { 0 } else { unsafe { inb(PS2_DATA) } } }

fn mouse_write(v: u8) { cmd(CMD_WRITE_AUX); data(v); let _ = read(); }
fn mouse_read() -> u8 { read() }

#[derive(Clone, Copy, Debug, Default)]
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub wheel: i8,
}

static mut CUR_X: i32 = 200;
static mut CUR_Y: i32 = 200;
static mut PACKET: [u8; 4] = [0; 4];
static mut PACKET_LEN: usize = 3;
static mut IDX: usize = 0;

pub fn mouse_init() {
    unsafe {
        // enable aux device
        cmd(CMD_ENABLE_AUX);

        // enable mouse IRQ in controller command byte
        cmd(CMD_READ_CCB);
        let mut ccb = read();
        ccb |= 0x02; // enable mouse IRQ
        ccb |= 0x20; // enable aux clock
        cmd(CMD_WRITE_CCB);
        data(ccb);

        mouse_write(MOUSE_RESET_DEFAULTS);

        // Enable wheel (IntelliMouse): 200,100,80 sampling trick
        mouse_write(MOUSE_SET_SAMPLE); mouse_write(200);
        mouse_write(MOUSE_SET_SAMPLE); mouse_write(100);
        mouse_write(MOUSE_SET_SAMPLE); mouse_write(80);
        mouse_write(MOUSE_GET_ID);
        let id = mouse_read();
        PACKET_LEN = if id == 3 { 4 } else { 3 };

        mouse_write(MOUSE_ENABLE_STREAM);
        IDX = 0;
    }
}

pub fn mouse_poll(max_w: i32, max_h: i32) -> Option<MouseState> {
    let mut out = None;
    unsafe {
        loop {
            let st = inb(PS2_STATUS);
            if st & 0x01 == 0 { break; }
            if st & 0x20 == 0 { break; } // not mouse
            let b = inb(PS2_DATA);

            // packet sync bit
            if IDX == 0 && (b & 0x08) == 0 { continue; }

            PACKET[IDX] = b;
            IDX += 1;
            if IDX < PACKET_LEN { continue; }
            IDX = 0;

            let b0 = PACKET[0];
            let b1 = PACKET[1];
            let b2 = PACKET[2];

            let left = (b0 & 0x01) != 0;
            let right = (b0 & 0x02) != 0;
            let middle = (b0 & 0x04) != 0;

            let mut dx = b1 as i32;
            let mut dy = b2 as i32;
            if b0 & 0x10 != 0 { dx |= !0xFF; }
            if b0 & 0x20 != 0 { dy |= !0xFF; }
            dy = -dy;

            CUR_X = (CUR_X + dx).clamp(0, max_w.saturating_sub(1));
            CUR_Y = (CUR_Y + dy).clamp(0, max_h.saturating_sub(1));

            let wheel = if PACKET_LEN == 4 { PACKET[3] as i8 } else { 0 };

            out = Some(MouseState { x: CUR_X, y: CUR_Y, left, right, middle, wheel });
        }
    }
    out
}
