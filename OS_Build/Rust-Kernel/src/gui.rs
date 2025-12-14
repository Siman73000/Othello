#![allow(dead_code)]

use crate::framebuffer_driver as fb;
use crate::font::{glyph_row, FONT_H, FONT_W};
use crate::mouse::MouseState;
use crate::serial_write_str;

pub const SHELL_BG_COLOR: u32 = 0x0B1220;
pub const SHELL_FG_COLOR: u32 = 0xE5E7EB;

const DESKTOP_BG: u32 = 0x0F172A;
const TOPBAR_BG: u32  = 0x111827;
const DOCK_BG: u32    = 0x0B1220;
const WINDOW_BG: u32  = 0x0B1220;
const WINDOW_HDR: u32 = 0x1F2937;
const WINDOW_BRD: u32 = 0x243244;
const SHADOW: u32     = 0x000000;

#[derive(Clone, Copy, Debug)]
pub struct Rect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }
impl Rect {
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
    pub fn union(&self, other: Rect) -> Rect {
        let x1 = self.x.min(other.x);
        let y1 = self.y.min(other.y);
        let x2 = (self.x + self.w).max(other.x + other.w);
        let y2 = (self.y + self.h).max(other.y + other.h);
        Rect { x: x1, y: y1, w: x2 - x1, h: y2 - y1 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction { None, ShellMoved }

static mut SHELL_OUTER: Rect = Rect { x: 160, y: 120, w: 760, h: 520 };
static mut SHELL_TITLE: Rect = Rect { x: 0, y: 0, w: 0, h: 0 };
static mut SHELL_CONTENT: Rect = Rect { x: 0, y: 0, w: 0, h: 0 };

static mut DRAG_ACTIVE: bool = false;
static mut DRAG_OFF_X: i32 = 0;
static mut DRAG_OFF_Y: i32 = 0;

static mut CUR_X: i32 = 80;
static mut CUR_Y: i32 = 80;
static mut CUR_VISIBLE: bool = true;
static mut CUR_DRAWN: bool = false;
static mut CUR_OLD: (i32, i32) = (-1, -1);

pub unsafe fn init_from_bootloader(raw: *const fb::BootVideoInfoRaw) {
    fb::init_from_bootinfo(raw);
    serial_write_str("FB: initialized.\n");
    recalc_layout();
    draw_desktop_full();
    draw_shell_window();
    serial_write_str("GUI: ready.\n");
}

fn recalc_layout() {
    unsafe {
        let o = SHELL_OUTER;
        let pad = 10;
        let hdr = 34;
        SHELL_TITLE = Rect { x: o.x + pad, y: o.y + pad, w: o.w - pad*2, h: hdr };
        SHELL_CONTENT = Rect { x: o.x + pad, y: o.y + pad + hdr, w: o.w - pad*2, h: o.h - pad*2 - hdr };
    }
}

pub fn shell_left() -> usize { unsafe { SHELL_CONTENT.x.max(0) as usize } }
pub fn shell_top() -> usize { unsafe { SHELL_CONTENT.y.max(0) as usize } }
pub fn shell_bottom() -> usize { unsafe { (SHELL_CONTENT.y + SHELL_CONTENT.h).max(0) as usize } }
pub fn shell_content_rect() -> Rect { unsafe { SHELL_CONTENT } }
pub fn shell_title_rect() -> Rect { unsafe { SHELL_TITLE } }

pub fn clear_shell_area() {
    let r = shell_content_rect();
    fb::fill_rect(r.x.max(0) as usize, r.y.max(0) as usize, r.w.max(0) as usize, r.h.max(0) as usize, WINDOW_BG);
}

pub fn fill_rect(x: usize, y: usize, w: usize, h: usize, c: u32) { fb::fill_rect(x, y, w, h, c); }
pub fn invert_rect(x: usize, y: usize, w: usize, h: usize) { fb::invert_rect(x, y, w, h); }

pub fn draw_char(x: usize, y: usize, ch: char, fg: u32, bg: u32) {
    fb::fill_rect(x, y, FONT_W, FONT_H, bg);
    let c = ch as u8;
    for row in 0..FONT_H {
        let bits = glyph_row(c, row);
        for col in 0..8 {
            if (bits >> col) & 1 != 0 {
                let px = x + (7 - col);
                let py = y + row;
                fb::fill_rect(px, py, 1, 1, fg);
            }
        }
    }
}

pub fn draw_text(mut x: usize, y: usize, text: &str, fg: u32, bg: u32) {
    for &b in text.as_bytes() {
        if b == b'\n' { break; }
        draw_char(x, y, b as char, fg, bg);
        x += FONT_W;
    }
}

pub fn draw_text_fg(x: usize, y: usize, text: &str, fg: u32) {
    draw_text(x, y, text, fg, WINDOW_BG);
}

fn outline(x: usize, y: usize, w: usize, h: usize, c: u32) {
    if w == 0 || h == 0 { return; }
    fb::fill_rect(x, y, w, 1, c);
    fb::fill_rect(x, y + h - 1, w, 1, c);
    fb::fill_rect(x, y, 1, h, c);
    fb::fill_rect(x + w - 1, y, 1, h, c);
}

fn shadow(o: Rect) {
    let sx = (o.x + o.w + 2).max(0) as usize;
    let sy = (o.y + 2).max(0) as usize;
    let sh = (o.h + 6).max(0) as usize;
    fb::fill_rect(sx, sy, 6, sh, SHADOW);

    let bx = (o.x + 2).max(0) as usize;
    let by = (o.y + o.h + 2).max(0) as usize;
    let bw = (o.w + 6).max(0) as usize;
    fb::fill_rect(bx, by, bw, 6, SHADOW);
}

pub fn draw_desktop_full() {
    fb::clear(DESKTOP_BG);
    fb::fill_rect(0, 0, fb::logical_width(), 36, TOPBAR_BG);
    draw_text(12, 10, "Othello OS", 0xE5E7EB, TOPBAR_BG);
    draw_text(140, 10, "Shell", 0x93C5FD, TOPBAR_BG);

    let h = fb::logical_height();
    if h > 70 {
        fb::fill_rect(0, h - 56, fb::logical_width(), 56, DOCK_BG);
        draw_text(16, h - 40, "[ ]  [ ]  [ ]", 0x9CA3AF, DOCK_BG);
    }
}

pub fn draw_shell_window() {
    unsafe {
        let o = SHELL_OUTER;
        shadow(o);

        let ox = o.x.max(0) as usize;
        let oy = o.y.max(0) as usize;
        let ow = o.w.max(0) as usize;
        let oh = o.h.max(0) as usize;

        fb::fill_rect(ox, oy, ow, oh, WINDOW_BG);
        outline(ox, oy, ow, oh, WINDOW_BRD);

        let t = SHELL_TITLE;
        fb::fill_rect(t.x.max(0) as usize, t.y.max(0) as usize, t.w.max(0) as usize, t.h.max(0) as usize, WINDOW_HDR);
        draw_text((t.x + 12) as usize, (t.y + 10) as usize, "Terminal", 0xE5E7EB, WINDOW_HDR);
        fb::fill_rect(t.x.max(0) as usize, (t.y + t.h - 1).max(0) as usize, t.w.max(0) as usize, 1, WINDOW_BRD);

        clear_shell_area();
    }
}

// -----------------------------------------------------------------------------
// Cursor (XOR invert small crosshair)
// -----------------------------------------------------------------------------
fn invert_cursor_at(x: usize, y: usize) {
    fb::invert_rect(x.saturating_sub(4), y, 9, 1);
    fb::invert_rect(x, y.saturating_sub(4), 1, 9);
    fb::invert_rect(x, y, 1, 1);
}

pub fn cursor_hide() {
    unsafe {
        if CUR_VISIBLE && CUR_DRAWN {
            invert_cursor_at(CUR_OLD.0.max(0) as usize, CUR_OLD.1.max(0) as usize);
            CUR_DRAWN = false;
        }
        CUR_VISIBLE = false;
    }
}

pub fn cursor_show() { unsafe { CUR_VISIBLE = true; } }

pub fn cursor_draw() {
    unsafe {
        if !CUR_VISIBLE { return; }
        if CUR_DRAWN {
            invert_cursor_at(CUR_OLD.0.max(0) as usize, CUR_OLD.1.max(0) as usize);
        }
        invert_cursor_at(CUR_X.max(0) as usize, CUR_Y.max(0) as usize);
        CUR_OLD = (CUR_X, CUR_Y);
        CUR_DRAWN = true;
    }
}

// -----------------------------------------------------------------------------
// Mouse -> UI (drag window by title bar)
// -----------------------------------------------------------------------------
fn redraw_damage(_d: Rect) {
    // Simple + stable (optimize later): repaint desktop + shell window
    draw_desktop_full();
    draw_shell_window();
}

pub fn ui_handle_mouse(ms: MouseState) -> UiAction {
    unsafe {
        CUR_X = ms.x;
        CUR_Y = ms.y;

        let title = SHELL_TITLE;
        let old_outer = SHELL_OUTER;

        if ms.left && !DRAG_ACTIVE && title.contains(ms.x, ms.y) {
            DRAG_ACTIVE = true;
            DRAG_OFF_X = ms.x - SHELL_OUTER.x;
            DRAG_OFF_Y = ms.y - SHELL_OUTER.y;
        }
        if !ms.left && DRAG_ACTIVE {
            DRAG_ACTIVE = false;
        }

        if DRAG_ACTIVE {
            let mut nx = ms.x - DRAG_OFF_X;
            let mut ny = ms.y - DRAG_OFF_Y;

            let sw = fb::logical_width() as i32;
            let sh = fb::logical_height() as i32;
            nx = nx.max(8).min(sw - SHELL_OUTER.w - 8);
            ny = ny.max(44).min(sh - SHELL_OUTER.h - 64);

            if nx != SHELL_OUTER.x || ny != SHELL_OUTER.y {
                cursor_hide();
                SHELL_OUTER.x = nx;
                SHELL_OUTER.y = ny;
                recalc_layout();
                redraw_damage(old_outer.union(SHELL_OUTER));
                cursor_show();
                return UiAction::ShellMoved;
            }
        }

        UiAction::None
    }
}
