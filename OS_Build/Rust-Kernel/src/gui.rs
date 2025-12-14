#![allow(dead_code)]

use crate::{framebuffer_driver as fb, font};
use crate::mouse::MouseState;
use crate::serial_write_str;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction {
    None,
    ShellMoved,
}

#[derive(Clone, Copy, Debug)]
struct Rect { x: i32, y: i32, w: i32, h: i32 }
impl Rect {
    #[inline] fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
}

static mut SCREEN_W: i32 = 0;
static mut SCREEN_H: i32 = 0;

// Window layout
static mut SHELL_OUTER: Rect = Rect { x: 80, y: 80, w: 860, h: 520 };
static mut SHELL_TITLE: Rect = Rect { x: 80, y: 80, w: 860, h: 34 };
static mut SHELL_CONTENT: Rect = Rect { x: 80, y: 114, w: 860, h: 486 };
static mut SHELL_FOOT: Rect = Rect { x: 92, y: 568, w: 836, h: 18 };

static mut DRAG_ACTIVE: bool = false;
static mut DRAG_OFF_X: i32 = 0;
static mut DRAG_OFF_Y: i32 = 0;

// Cursor state (software-drawn arrow with background save)
const CUR_W: usize = 16;
const CUR_H: usize = 16;
static mut CUR_VISIBLE: bool = true;
static mut CUR_DRAWN: bool = false;
static mut CUR_X: i32 = 200;
static mut CUR_Y: i32 = 200;
static mut CUR_SAVE: [u32; CUR_W * CUR_H] = [0; CUR_W * CUR_H];

// Theme
pub const SHELL_BG_COLOR: u32 = 0x0B1220;
pub const SHELL_FG_COLOR: u32 = 0xE5E7EB;

const DESKTOP_BG_TOP: u32 = 0x0B1020;
const DESKTOP_BG_BOT: u32 = 0x102030;
const TOPBAR_BG: u32      = 0x0B1220;
const DOCK_BG: u32        = 0x111827;
const ACCENT: u32         = 0x38BDF8;

const WINDOW_BRD: u32     = 0x1F2937;
const WINDOW_BG: u32      = 0x0F172A;
const WINDOW_HDR: u32     = 0x111827;
const SHADOW: u32         = 0x000000;

#[inline]
fn lerp_u8(a: u8, b: u8, t_num: u32, t_den: u32) -> u8 {
    let a = a as u32;
    let b = b as u32;
    (((a * (t_den - t_num)) + (b * t_num)) / t_den) as u8
}
#[inline]
fn lerp_color(a: u32, b: u32, t_num: u32, t_den: u32) -> u32 {
    let ar = ((a >> 16) & 0xFF) as u8;
    let ag = ((a >> 8) & 0xFF) as u8;
    let ab = (a & 0xFF) as u8;
    let br = ((b >> 16) & 0xFF) as u8;
    let bg = ((b >> 8) & 0xFF) as u8;
    let bb = (b & 0xFF) as u8;
    let r = lerp_u8(ar, br, t_num, t_den) as u32;
    let g = lerp_u8(ag, bg, t_num, t_den) as u32;
    let bl = lerp_u8(ab, bb, t_num, t_den) as u32;
    (r << 16) | (g << 8) | bl
}

pub fn shell_left() -> i32 { unsafe { SHELL_OUTER.x } }
pub fn shell_top() -> i32 { unsafe { SHELL_OUTER.y } }
pub fn shell_content_left() -> i32 { unsafe { SHELL_CONTENT.x } }
pub fn shell_content_top() -> i32 { unsafe { SHELL_CONTENT.y } }
pub fn shell_content_w() -> i32 { unsafe { SHELL_CONTENT.w } }
pub fn shell_content_h() -> i32 { unsafe { SHELL_CONTENT.h } }
pub fn shell_footer_x() -> i32 { unsafe { SHELL_FOOT.x } }
pub fn shell_footer_y() -> i32 { unsafe { SHELL_FOOT.y } }
pub fn shell_footer_w() -> i32 { unsafe { SHELL_FOOT.w } }
pub fn shell_footer_h() -> i32 { unsafe { SHELL_FOOT.h } }

pub fn init_from_bootloader(info: *const fb::BootVideoInfoRaw) {
    unsafe {
        if !fb::init_from_bootinfo(info) {
            serial_write_str("GUI: framebuffer init failed.\n");
            loop {}
        }

        SCREEN_W = fb::width() as i32;
        SCREEN_H = fb::height() as i32;

        // Pick a nice default shell placement for big screens.
        if SCREEN_W >= 1600 { SHELL_OUTER.x = 160; SHELL_OUTER.y = 110; }
        recalc_layout();

        draw_desktop();
        draw_shell_window();
        clear_shell_content();
        cursor_redraw();
        serial_write_str("GUI: initialized.\n");
    }
}

fn recalc_layout() {
    unsafe {
        let hdr = 34;
        let pad = 12;

        SHELL_TITLE = Rect { x: SHELL_OUTER.x, y: SHELL_OUTER.y, w: SHELL_OUTER.w, h: hdr };
        SHELL_CONTENT = Rect {
            x: SHELL_OUTER.x + pad,
            y: SHELL_OUTER.y + hdr + pad,
            w: SHELL_OUTER.w - pad*2,
            h: (SHELL_OUTER.h - hdr - pad*3 - 18).max(0),
        };
        SHELL_FOOT = Rect {
            x: SHELL_OUTER.x + pad,
            y: SHELL_OUTER.y + SHELL_OUTER.h - (pad + 18),
            w: SHELL_OUTER.w - pad*2,
            h: 18,
        };
    }
}

pub fn clear_shell_content() {
    unsafe {
        cursor_restore();
        let r = SHELL_CONTENT;
        fb::fill_rect(r.x as usize, r.y as usize, r.w as usize, r.h as usize, WINDOW_BG);
        cursor_redraw();
    }
}

fn fill_round_rect(x: i32, y: i32, w: i32, h: i32, r: i32, color: u32) {
    if w <= 0 || h <= 0 { return; }
    let (x0, y0, w0, h0) = (x.max(0) as usize, y.max(0) as usize, w.max(0) as usize, h.max(0) as usize);

    if r <= 1 {
        fb::fill_rect(x0, y0, w0, h0, color);
        return;
    }

    let rr = r as usize;
    if w0 > rr*2 {
        fb::fill_rect(x0 + rr, y0, w0 - rr*2, h0, color);
    }
    if h0 > rr*2 {
        fb::fill_rect(x0, y0 + rr, w0, h0 - rr*2, color);
    }

    let r2 = (r*r) as i32;
    for dy in 0..r {
        for dx in 0..r {
            let cx = r - 1 - dx;
            let cy = r - 1 - dy;
            let d2 = cx*cx + cy*cy;
            if d2 <= r2 {
                fb::set_pixel((x + dx) as usize, (y + dy) as usize, color);
                fb::set_pixel((x + w - 1 - dx) as usize, (y + dy) as usize, color);
                fb::set_pixel((x + dx) as usize, (y + h - 1 - dy) as usize, color);
                fb::set_pixel((x + w - 1 - dx) as usize, (y + h - 1 - dy) as usize, color);
            }
        }
    }
}

fn draw_desktop() {
    unsafe {
        let w = SCREEN_W.max(0) as usize;
        let h = SCREEN_H.max(0) as usize;
        if w == 0 || h == 0 { return; }

        for y in 0..h {
            let c = lerp_color(DESKTOP_BG_TOP, DESKTOP_BG_BOT, y as u32, (h.saturating_sub(1) as u32).max(1));
            fb::fill_rect(0, y, w, 1, c);
        }

        // Top bar
        fb::fill_rect(0, 0, w, 32, TOPBAR_BG);
        fb::fill_rect(0, 31, w, 1, ACCENT);
        draw_text(12, 8, "O t h e l l o  O S", 0xE5E7EB, TOPBAR_BG);

        // Dock
        let dock_w = (w as i32 * 60 / 100).min(900).max(520);
        let dock_h = 54;
        let dock_x = (SCREEN_W - dock_w) / 2;
        let dock_y = SCREEN_H - dock_h - 16;
        fill_round_rect(dock_x, dock_y, dock_w, dock_h, 16, DOCK_BG);
        fb::fill_rect(dock_x as usize, (dock_y + dock_h - 1) as usize, dock_w as usize, 1, WINDOW_BRD);

        // simple dock icons
        let mut x = dock_x + 16;
        for i in 0..6 {
            fill_round_rect(x, dock_y + 10, 34, 34, 10, if i == 0 { ACCENT } else { 0x334155 });
            x += 46;
        }
    }
}

fn draw_shell_window() {
    unsafe {
        let r = SHELL_OUTER;
        let x = r.x; let y = r.y; let w = r.w; let h = r.h;

        // Shadow
        fill_round_rect(x + 6, y + 8, w, h, 14, SHADOW);
        // Border
        fill_round_rect(x, y, w, h, 14, WINDOW_BRD);
        // Body
        fill_round_rect(x + 1, y + 1, w - 2, h - 2, 14, WINDOW_BG);
        // Header
        fill_round_rect(x + 1, y + 1, w - 2, 34, 14, WINDOW_HDR);
        fb::fill_rect(x as usize + 1, (y + 33) as usize, (w - 2) as usize, 1, WINDOW_BRD);

        // Title
        draw_text(x + 16, y + 10, "Othello Shell", 0xF3F4F6, WINDOW_HDR);

        // window controls
        fill_round_rect(x + w - 56, y + 10, 10, 10, 5, 0xEF4444);
        fill_round_rect(x + w - 40, y + 10, 10, 10, 5, 0xF59E0B);
        fill_round_rect(x + w - 24, y + 10, 10, 10, 5, 0x10B981);
    }
}

/// Treat shadow as part of paint region so move doesn't leave trails.
fn shell_paint_rect(r: Rect) -> Rect {
    Rect { x: r.x, y: r.y, w: r.w + 6, h: r.h + 8 }
}

fn redraw_exposed(old: Rect, new: Rect) {
    let ix0 = old.x.max(new.x);
    let iy0 = old.y.max(new.y);
    let ix1 = (old.x + old.w).min(new.x + new.w);
    let iy1 = (old.y + old.h).min(new.y + new.h);

    if ix1 <= ix0 || iy1 <= iy0 {
        redraw_damage(old);
        return;
    }

    // Top strip
    if iy0 > old.y {
        redraw_damage(Rect { x: old.x, y: old.y, w: old.w, h: iy0 - old.y });
    }
    // Bottom strip
    let old_y1 = old.y + old.h;
    if iy1 < old_y1 {
        redraw_damage(Rect { x: old.x, y: iy1, w: old.w, h: old_y1 - iy1 });
    }
    // Left strip
    if ix0 > old.x {
        redraw_damage(Rect { x: old.x, y: iy0, w: ix0 - old.x, h: iy1 - iy0 });
    }
    // Right strip
    let old_x1 = old.x + old.w;
    if ix1 < old_x1 {
        redraw_damage(Rect { x: ix1, y: iy0, w: old_x1 - ix1, h: iy1 - iy0 });
    }
}

fn redraw_damage(d: Rect) {
    unsafe {
        cursor_restore();

        let x0 = d.x.max(0) as usize;
        let y0 = d.y.max(0) as usize;
        let x1 = (d.x + d.w).min(SCREEN_W).max(0) as usize;
        let y1 = (d.y + d.h).min(SCREEN_H).max(0) as usize;
        if x1 <= x0 || y1 <= y0 { return; }

        // Desktop gradient for the damaged region
        let den = (SCREEN_H.saturating_sub(1) as u32).max(1);
        for y in y0..y1 {
            let c = lerp_color(DESKTOP_BG_TOP, DESKTOP_BG_BOT, y as u32, den);
            fb::fill_rect(x0, y, x1 - x0, 1, c);
        }

        // Topbar if intersecting
        if y0 < 32 {
            let yy0 = y0;
            let yy1 = y1.min(32);
            fb::fill_rect(x0, yy0, x1 - x0, yy1 - yy0, TOPBAR_BG);
            if yy1 > 31 && yy0 <= 31 {
                fb::fill_rect(x0, 31, x1 - x0, 1, ACCENT);
            }
            if x0 < 140 && yy0 < 24 {
                draw_text(12, 8, "Othello OS", 0xE5E7EB, TOPBAR_BG);
            }
        }

        // Dock if intersecting
        let w = SCREEN_W.max(0) as usize;
        let dock_w = (w as i32 * 60 / 100).min(900).max(520);
        let dock_h = 54;
        let dock_x = (SCREEN_W - dock_w) / 2;
        let dock_y = SCREEN_H - dock_h - 16;
        let dock_rect = Rect { x: dock_x, y: dock_y, w: dock_w, h: dock_h };
        if rects_intersect(d, dock_rect) {
            fill_round_rect(dock_x, dock_y, dock_w, dock_h, 16, DOCK_BG);
            fb::fill_rect(dock_x as usize, (dock_y + dock_h - 1) as usize, dock_w as usize, 1, WINDOW_BRD);
            let mut x = dock_x + 16;
            for i in 0..6 {
                fill_round_rect(x, dock_y + 10, 34, 34, 10, if i == 0 { ACCENT } else { 0x334155 });
                x += 46;
            }
        }

        cursor_redraw();
    }
}

#[inline]
fn rects_intersect(a: Rect, b: Rect) -> bool {
    let ax1 = a.x + a.w;
    let ay1 = a.y + a.h;
    let bx1 = b.x + b.w;
    let by1 = b.y + b.h;
    a.x < bx1 && ax1 > b.x && a.y < by1 && ay1 > b.y
}

// ----------------------------------------------------------------------------
// Text drawing (8x16) with background fill
// ----------------------------------------------------------------------------

pub fn draw_char(x: i32, y: i32, ch: u8, fg: u32, bg: u32) {
    if ch as usize >= 128 { return; }
    if x < -16 || y < -16 { return; }
    unsafe {
        cursor_restore();
        for row in 0..font::FONT_H {
            let bits = font::glyph_row(ch, row);
            for col in 0..font::FONT_W {
                let px = x + col as i32;
                let py = y + row as i32;
                if px < 0 || py < 0 { continue; }
                let on = (bits & (1 << (7 - col))) != 0;
                fb::set_pixel(px as usize, py as usize, if on { fg } else { bg });
            }
        }
        cursor_redraw();
    }
}

pub fn draw_text(x: i32, y: i32, text: &str, fg: u32, bg: u32) {
    let mut cx = x;
    let mut cy = y;
    for &b in text.as_bytes() {
        if b == b'\n' {
            cx = x;
            cy += font::FONT_H as i32;
            continue;
        }
        if b == b'\r' { continue; }
        draw_glyph_nocursor(cx, cy, b, fg, bg);
        cx += font::FONT_W as i32;
    }
}

fn draw_glyph_nocursor(x: i32, y: i32, ch: u8, fg: u32, bg: u32) {
    if ch as usize >= 128 { return; }
    for row in 0..font::FONT_H {
        let bits = font::glyph_row(ch, row);
        for col in 0..font::FONT_W {
            let px = x + col as i32;
            let py = y + row as i32;
            if px < 0 || py < 0 { continue; }
            let on = (bits & (1 << (7 - col))) != 0;
            fb::set_pixel(px as usize, py as usize, if on { fg } else { bg });
        }
    }
}

// ----------------------------------------------------------------------------
// Cursor (save/restore) - IMPORTANT: always restore before drawing other things
// ----------------------------------------------------------------------------

fn cursor_restore() {
    unsafe {
        if !CUR_DRAWN { return; }
        let ox = CUR_X;
        let oy = CUR_Y;
        let sw = SCREEN_W;
        let sh = SCREEN_H;
        for cy in 0..CUR_H {
            for cx in 0..CUR_W {
                let x = ox + cx as i32;
                let y = oy + cy as i32;
                if x < 0 || y < 0 || x >= sw || y >= sh { continue; }
                let idx = cy * CUR_W + cx;
                fb::set_pixel(x as usize, y as usize, CUR_SAVE[idx]);
            }
        }
        CUR_DRAWN = false;
    }
}

fn cursor_redraw() {
    unsafe {
        if !CUR_VISIBLE { return; }
        let sw = SCREEN_W;
        let sh = SCREEN_H;
        // Clamp cursor into screen so restore/draw always matches.
        CUR_X = CUR_X.clamp(0, sw.saturating_sub(CUR_W as i32));
        CUR_Y = CUR_Y.clamp(0, sh.saturating_sub(CUR_H as i32));

        // Save background
        for cy in 0..CUR_H {
            for cx in 0..CUR_W {
                let x = CUR_X + cx as i32;
                let y = CUR_Y + cy as i32;
                let idx = cy * CUR_W + cx;
                CUR_SAVE[idx] = fb::get_pixel(x as usize, y as usize);
            }
        }

        // Arrow shape: white with dark outline
        for cy in 0..CUR_H {
            for cx in 0..CUR_W {
                let x = CUR_X + cx as i32;
                let y = CUR_Y + cy as i32;
                // shape mask
                let on = cx == 0
                    || (cy <= cx && cx <= 8)
                    || (cy == 8 && cx <= 10)
                    || (cy == 9 && cx <= 9)
                    || (cy == 10 && cx <= 8);
                if !on { continue; }

                // outline
                let outline = cx == 0 || cy == 0 || (cy == cx && cx <= 8);
                fb::set_pixel(x as usize, y as usize, if outline { 0x0B0F18 } else { 0xFFFFFF });
            }
        }

        CUR_DRAWN = true;
    }
}

pub fn cursor_only(x: i32, y: i32) {
    unsafe {
        if !CUR_VISIBLE { return; }
        cursor_restore();
        CUR_X = x;
        CUR_Y = y;
        cursor_redraw();
    }
}

pub fn cursor_draw() {
    unsafe {
        if !CUR_VISIBLE { return; }
        if !CUR_DRAWN { cursor_redraw(); }
    }
}

pub fn cursor_hide() {
    unsafe {
        if CUR_DRAWN { cursor_restore(); }
        CUR_VISIBLE = false;
    }
}

pub fn cursor_show() { unsafe { CUR_VISIBLE = true; } }

// ----------------------------------------------------------------------------
// Mouse/UI handling (drag shell by header)
// ----------------------------------------------------------------------------

pub fn ui_handle_mouse(ms: MouseState) -> UiAction {
    unsafe {
        // Update cursor first (restore old, set new, but don't redraw until after window ops)
        cursor_restore();
        CUR_X = ms.x;
        CUR_Y = ms.y;

        let old_outer = SHELL_OUTER;
        let title = SHELL_TITLE;

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

            nx = nx.clamp(12, (SCREEN_W - SHELL_OUTER.w - 12).max(12));
            ny = ny.clamp(40, (SCREEN_H - SHELL_OUTER.h - 88).max(40));

            if nx != SHELL_OUTER.x || ny != SHELL_OUTER.y {
                SHELL_OUTER.x = nx;
                SHELL_OUTER.y = ny;
                recalc_layout();

                // Hide cursor during blit to avoid any overlay getting copied.
                let cx = CUR_X; let cy = CUR_Y;
                CUR_VISIBLE = false;
                cursor_restore();

                let old_paint = shell_paint_rect(old_outer);
                let new_outer = SHELL_OUTER;
                let new_paint = shell_paint_rect(new_outer);

                fb::blit_move_rect(old_paint.x, old_paint.y, old_paint.w, old_paint.h, new_paint.x, new_paint.y);
                redraw_exposed(old_paint, new_paint);

                // Re-enable cursor and redraw at latest position.
                CUR_VISIBLE = true;
                CUR_X = cx; CUR_Y = cy;
                cursor_redraw();
                return UiAction::ShellMoved;
            }
        }

        cursor_redraw();
        UiAction::None
    }
}
