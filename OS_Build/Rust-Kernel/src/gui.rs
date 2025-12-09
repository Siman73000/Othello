#![allow(dead_code)]

use core::cmp::min;

#[repr(C, packed)]
pub struct BootVideoInfo {
    pub width: u16,
    pub height: u16,
    pub bpp: u16,
    pub framebuffer_addr: u64,
}

// Default: VGA Mode 13h (320x200x8bpp) at 0xA0000
static mut VIDEO_INFO: BootVideoInfo = BootVideoInfo {
    width: 320,
    height: 200,
    bpp: 8,
    framebuffer_addr: 0x000A0000,
};

pub fn init_from_bootloader(ptr: *const BootVideoInfo) {
    // Ready for a future VBE handoff, unused for now.
    unsafe { VIDEO_INFO = core::ptr::read(ptr) };
}

pub fn fb_width() -> usize {
    unsafe { VIDEO_INFO.width as usize }
}

pub fn fb_height() -> usize {
    unsafe { VIDEO_INFO.height as usize }
}

fn fb_ptr() -> *mut u8 {
    unsafe { VIDEO_INFO.framebuffer_addr as *mut u8 }
}

// -----------------------------------------------------------------------------
// Colors / font (black / gray / white)
// -----------------------------------------------------------------------------

pub const FONT_W: usize = 8;
pub const FONT_H: usize = 8;

// Classic VGA-ish palette:
// 0x00 = black, 0x08 = dark gray, 0x07 = light gray, 0x0F = bright white
pub const DESKTOP_BG_COLOR: u8 = 0x00; // black
pub const TITLE_BAR_COLOR: u8   = 0x08; // dark gray
pub const SHELL_BG_COLOR: u8    = 0x00; // black
pub const SHELL_FG_COLOR: u8    = 0x0F; // white

// -----------------------------------------------------------------------------
// Framebuffer helpers (8bpp Mode 13h)
// -----------------------------------------------------------------------------

pub fn fb_put_pixel(x: usize, y: usize, color: u8) {
    let w = fb_width();
    let h = fb_height();
    if x >= w || y >= h {
        return;
    }
    let idx = y * w + x;
    unsafe {
        *fb_ptr().add(idx) = color;
    }
}

pub fn fb_get_pixel(x: usize, y: usize) -> u8 {
    let w = fb_width();
    let h = fb_height();
    if x >= w || y >= h {
        return 0;
    }
    let idx = y * w + x;
    unsafe { *fb_ptr().add(idx) }
}

pub fn clear_screen(color: u8) {
    let w = fb_width();
    let h = fb_height();
    for y in 0..h {
        for x in 0..w {
            fb_put_pixel(x, y, color);
        }
    }
}

pub fn fill_rect(x: usize, y: usize, w: usize, h: usize, color: u8) {
    let fw = fb_width();
    let fh = fb_height();
    if x >= fw || y >= fh {
        return;
    }

    let x2 = min(x + w, fw);
    let y2 = min(y + h, fh);
    for yy in y..y2 {
        for xx in x..x2 {
            fb_put_pixel(xx, yy, color);
        }
    }
}

pub fn draw_rect_outline(x: usize, y: usize, w: usize, h: usize, color: u8) {
    let fw = fb_width();
    let fh = fb_height();
    if w == 0 || h == 0 || x >= fw || y >= fh {
        return;
    }

    let x2 = min(x + w - 1, fw - 1);
    let y2 = min(y + h - 1, fh - 1);

    for xx in x..=x2 {
        fb_put_pixel(xx, y, color);
        fb_put_pixel(xx, y2, color);
    }
    for yy in y..=y2 {
        fb_put_pixel(x, yy, color);
        fb_put_pixel(x2, yy, color);
    }
}

// -----------------------------------------------------------------------------
// 8×8 bitmap font (same as before)
// -----------------------------------------------------------------------------

const GLYPH_BLANK: [u8; FONT_H] = [0x00; FONT_H];
const GLYPH_DOT:   [u8; FONT_H] = [0x00,0x00,0x00,0x00,0x00,0x18,0x18,0x00];
const GLYPH_GT:    [u8; FONT_H] = [0x00,0x40,0x20,0x10,0x20,0x40,0x00,0x00];

const GLYPH_0: [u8; FONT_H] = [0x3C,0x66,0x6E,0x7E,0x76,0x66,0x3C,0x00];
const GLYPH_1: [u8; FONT_H] = [0x18,0x38,0x18,0x18,0x18,0x18,0x3C,0x00];
const GLYPH_2: [u8; FONT_H] = [0x3C,0x66,0x06,0x0C,0x30,0x60,0x7E,0x00];
const GLYPH_3: [u8; FONT_H] = [0x3C,0x66,0x06,0x1C,0x06,0x66,0x3C,0x00];
const GLYPH_4: [u8; FONT_H] = [0x0C,0x1C,0x3C,0x6C,0x7E,0x0C,0x0C,0x00];
const GLYPH_5: [u8; FONT_H] = [0x7E,0x60,0x7C,0x06,0x06,0x66,0x3C,0x00];
const GLYPH_6: [u8; FONT_H] = [0x1C,0x30,0x60,0x7C,0x66,0x66,0x3C,0x00];
const GLYPH_7: [u8; FONT_H] = [0x7E,0x06,0x0C,0x18,0x30,0x30,0x30,0x00];
const GLYPH_8: [u8; FONT_H] = [0x3C,0x66,0x66,0x3C,0x66,0x66,0x3C,0x00];
const GLYPH_9: [u8; FONT_H] = [0x3C,0x66,0x66,0x3E,0x06,0x0C,0x38,0x00];

const GLYPH_A: [u8; FONT_H] = [0x18,0x24,0x42,0x7E,0x42,0x42,0x42,0x00];
const GLYPH_B: [u8; FONT_H] = [0x7C,0x62,0x62,0x7C,0x62,0x62,0x7C,0x00];
const GLYPH_C: [u8; FONT_H] = [0x3C,0x62,0x60,0x60,0x60,0x62,0x3C,0x00];
const GLYPH_D: [u8; FONT_H] = [0x78,0x64,0x62,0x62,0x62,0x64,0x78,0x00];
const GLYPH_E: [u8; FONT_H] = [0x7E,0x60,0x60,0x7C,0x60,0x60,0x7E,0x00];
const GLYPH_F: [u8; FONT_H] = [0x7E,0x60,0x60,0x7C,0x60,0x60,0x60,0x00];
const GLYPH_G: [u8; FONT_H] = [0x3C,0x62,0x60,0x6E,0x62,0x62,0x3C,0x00];
const GLYPH_H: [u8; FONT_H] = [0x42,0x42,0x42,0x7E,0x42,0x42,0x42,0x00];
const GLYPH_I: [u8; FONT_H] = [0x3C,0x18,0x18,0x18,0x18,0x18,0x3C,0x00];
const GLYPH_J: [u8; FONT_H] = [0x1E,0x0C,0x0C,0x0C,0x0C,0x6C,0x38,0x00];
const GLYPH_K: [u8; FONT_H] = [0x62,0x64,0x68,0x70,0x68,0x64,0x62,0x00];
const GLYPH_L: [u8; FONT_H] = [0x60,0x60,0x60,0x60,0x60,0x60,0x7E,0x00];
const GLYPH_M: [u8; FONT_H] = [0x42,0x66,0x5A,0x5A,0x42,0x42,0x42,0x00];
const GLYPH_N: [u8; FONT_H] = [0x42,0x62,0x72,0x5A,0x4E,0x46,0x42,0x00];
const GLYPH_O: [u8; FONT_H] = [0x3C,0x62,0x62,0x62,0x62,0x62,0x3C,0x00];
const GLYPH_P: [u8; FONT_H] = [0x7C,0x62,0x62,0x7C,0x60,0x60,0x60,0x00];
const GLYPH_Q: [u8; FONT_H] = [0x3C,0x62,0x62,0x62,0x6A,0x64,0x3A,0x00];
const GLYPH_R: [u8; FONT_H] = [0x7C,0x62,0x62,0x7C,0x68,0x64,0x62,0x00];
const GLYPH_S: [u8; FONT_H] = [0x3C,0x62,0x30,0x1C,0x06,0x62,0x3C,0x00];
const GLYPH_T: [u8; FONT_H] = [0x7E,0x18,0x18,0x18,0x18,0x18,0x18,0x00];
const GLYPH_U: [u8; FONT_H] = [0x42,0x42,0x42,0x42,0x42,0x42,0x3C,0x00];
const GLYPH_V: [u8; FONT_H] = [0x42,0x42,0x42,0x24,0x24,0x18,0x18,0x00];
const GLYPH_W: [u8; FONT_H] = [0x42,0x42,0x42,0x5A,0x5A,0x66,0x42,0x00];
const GLYPH_X: [u8; FONT_H] = [0x42,0x24,0x18,0x18,0x18,0x24,0x42,0x00];
const GLYPH_Y: [u8; FONT_H] = [0x42,0x24,0x18,0x18,0x18,0x18,0x18,0x00];
const GLYPH_Z: [u8; FONT_H] = [0x7E,0x04,0x08,0x10,0x20,0x40,0x7E,0x00];

fn glyph_for(ch: char) -> &'static [u8; FONT_H] {
    match ch {
        '0' => &GLYPH_0,
        '1' => &GLYPH_1,
        '2' => &GLYPH_2,
        '3' => &GLYPH_3,
        '4' => &GLYPH_4,
        '5' => &GLYPH_5,
        '6' => &GLYPH_6,
        '7' => &GLYPH_7,
        '8' => &GLYPH_8,
        '9' => &GLYPH_9,

        'a' | 'A' => &GLYPH_A,
        'b' | 'B' => &GLYPH_B,
        'c' | 'C' => &GLYPH_C,
        'd' | 'D' => &GLYPH_D,
        'e' | 'E' => &GLYPH_E,
        'f' | 'F' => &GLYPH_F,
        'g' | 'G' => &GLYPH_G,
        'h' | 'H' => &GLYPH_H,
        'i' | 'I' => &GLYPH_I,
        'j' | 'J' => &GLYPH_J,
        'k' | 'K' => &GLYPH_K,
        'l' | 'L' => &GLYPH_L,
        'm' | 'M' => &GLYPH_M,
        'n' | 'N' => &GLYPH_N,
        'o' | 'O' => &GLYPH_O,
        'p' | 'P' => &GLYPH_P,
        'q' | 'Q' => &GLYPH_Q,
        'r' | 'R' => &GLYPH_R,
        's' | 'S' => &GLYPH_S,
        't' | 'T' => &GLYPH_T,
        'u' | 'U' => &GLYPH_U,
        'v' | 'V' => &GLYPH_V,
        'w' | 'W' => &GLYPH_W,
        'x' | 'X' => &GLYPH_X,
        'y' | 'Y' => &GLYPH_Y,
        'z' | 'Z' => &GLYPH_Z,

        '>' => &GLYPH_GT,
        '.' => &GLYPH_DOT,
        ' ' => &GLYPH_BLANK,

        _ => &GLYPH_BLANK,
    }
}

pub fn draw_char(x: usize, y: usize, ch: char, color: u8) {
    let glyph = glyph_for(ch);
    let h = fb_height();
    let w = fb_width();

    for (row, pattern) in glyph.iter().enumerate() {
        let yy = y + row;
        if yy >= h {
            break;
        }
        for col in 0..FONT_W {
            let xx = x + col;
            if xx >= w {
                break;
            }
            let bit = 1 << (7 - col);
            if (pattern & bit) != 0 {
                fb_put_pixel(xx, yy, color);
            }
        }
    }
}

pub fn draw_text(mut x: usize, mut y: usize, s: &str, color: u8) {
    for ch in s.chars() {
        if ch == '\n' {
            y += FONT_H + 2;
            x = 0;
            continue;
        }
        draw_char(x, y, ch, color);
        x += FONT_W;
    }
}

// -----------------------------------------------------------------------------
// Shell geometry (scales with fb_width/fb_height)
// -----------------------------------------------------------------------------

const SHELL_MARGIN_X: usize = 16;
const SHELL_MARGIN_Y_TOP: usize = 40;
const SHELL_MARGIN_Y_BOTTOM: usize = 16;

pub fn shell_left() -> usize {
    SHELL_MARGIN_X
}

pub fn shell_top() -> usize {
    SHELL_MARGIN_Y_TOP
}

pub fn shell_right() -> usize {
    let w = fb_width();
    if w > SHELL_MARGIN_X * 2 {
        w - SHELL_MARGIN_X
    } else {
        w
    }
}

pub fn shell_bottom() -> usize {
    let h = fb_height();
    if h > SHELL_MARGIN_Y_TOP + SHELL_MARGIN_Y_BOTTOM {
        h - SHELL_MARGIN_Y_BOTTOM
    } else {
        h
    }
}

pub fn clear_shell_area() {
    let left = shell_left();
    let top = shell_top();
    let right = shell_right();
    let bottom = shell_bottom();

    if right <= left || bottom <= top {
        return;
    }

    let shell_w = right - left;
    let shell_h = bottom - top;
    fill_rect(left, top, shell_w, shell_h, SHELL_BG_COLOR);
}

// -----------------------------------------------------------------------------
// Desktop / window
// -----------------------------------------------------------------------------

pub fn init_desktop() {
    let w = fb_width();
    let h = fb_height();

    // Desktop background (black)
    clear_screen(DESKTOP_BG_COLOR);

    // Title bar (dark gray)
    fill_rect(0, 0, w, 20, TITLE_BAR_COLOR);
    draw_text(8, 6, "Othello OS", 0x0F);

    // Outer window (dark gray with light gray border)
    let win_x = 8usize;
    let win_y = 24usize;
    let win_w = w.saturating_sub(16);
    let win_h = h.saturating_sub(32);

    fill_rect(win_x, win_y, win_w, win_h, 0x08);
    draw_rect_outline(win_x, win_y, win_w, win_h, 0x07);

    // Shell panel
    let left = shell_left();
    let top = shell_top();
    let right = shell_right();
    let bottom = shell_bottom();
    if right <= left || bottom <= top {
        return;
    }

    let shell_w = right - left;
    let shell_h = bottom - top;
    fill_rect(left, top, shell_w, shell_h, SHELL_BG_COLOR);
    draw_rect_outline(left, top, shell_w, shell_h, 0x07);

    draw_text(
        left + 4,
        top + 4,
        "Shell: type 'help' (scroll.up / scroll.down / net.scan)",
        SHELL_FG_COLOR,
    );
}
