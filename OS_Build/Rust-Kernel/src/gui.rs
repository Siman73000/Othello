#![allow(dead_code)]

extern crate alloc;

use alloc::vec::Vec;

use crate::{framebuffer_driver as fb, font, time};
use crate::mouse::MouseState;
use crate::serial_write_str;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction {
    None,
    ShellMoved,
    ShellVisibilityChanged,
    ShellResized,
    DockLaunch(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiMode {
    Login,
    Desktop,
}

#[derive(Clone, Copy, Debug, Default)]
struct Rect { x: i32, y: i32, w: i32, h: i32 }
impl Rect {
    #[inline] fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
}

static mut SCREEN_W: i32 = 0;
static mut SCREEN_H: i32 = 0;

static mut UI_MODE: UiMode = UiMode::Desktop;

// Window layout
static mut SHELL_VISIBLE: bool = true;
static mut SHELL_MAXIMIZED: bool = false;
static mut SHELL_RESTORE: Rect = Rect { x: 80, y: 80, w: 860, h: 520 };

static mut SHELL_OUTER: Rect = Rect { x: 80, y: 80, w: 860, h: 520 };
static mut SHELL_TITLE: Rect = Rect { x: 80, y: 80, w: 860, h: 34 };
static mut SHELL_CONTENT: Rect = Rect { x: 80, y: 114, w: 860, h: 486 };
static mut SHELL_FOOT: Rect = Rect { x: 92, y: 568, w: 836, h: 18 };

// Current window title (header text)
static mut SHELL_TITLE_TEXT: &'static str = "Othello Shell";

// Traffic light hit rects
static mut BTN_CLOSE: Rect = Rect { x: 0, y: 0, w: 0, h: 0 };
static mut BTN_MIN: Rect   = Rect { x: 0, y: 0, w: 0, h: 0 };
static mut BTN_MAX: Rect   = Rect { x: 0, y: 0, w: 0, h: 0 };

static mut DRAG_ACTIVE: bool = false;
static mut DRAG_OFF_X: i32 = 0;
static mut DRAG_OFF_Y: i32 = 0;

// Taskbar (Windows 11-ish)
const TASKBAR_H: i32 = 44;
const TASKBAR_BG: u32 = 0x0B1220;
const TASKBAR_TOP: u32 = 0x1F2A3A;

// Dock layout (computed from screen)
const DOCK_ICON_COUNT: usize = 7;
static mut DOCK_RECT: Rect = Rect { x: 0, y: 0, w: 0, h: 0 };
static mut DOCK_ICONS: [Rect; DOCK_ICON_COUNT] = [Rect { x: 0, y: 0, w: 0, h: 0 }; DOCK_ICON_COUNT];

// Mouse click edge detection
static mut LAST_LEFT: bool = false;
static mut LAST_RIGHT: bool = false;

// -----------------------------------------------------------------------------
// Desktop right-click UI: context menu + wallpaper picker
// -----------------------------------------------------------------------------

struct SavedRegion {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    buf: Vec<u32>,
}

static mut CTX_OPEN: bool = false;
static mut CTX_RECT: Rect = Rect { x: 0, y: 0, w: 0, h: 0 };
static mut CTX_ITEM_BG: Rect = Rect { x: 0, y: 0, w: 0, h: 0 };
static mut CTX_SAVE: Option<SavedRegion> = None;

static mut PICKER_OPEN: bool = false;
static mut PICKER_RECT: Rect = Rect { x: 0, y: 0, w: 0, h: 0 };
static mut PICKER_CLOSE: Rect = Rect { x: 0, y: 0, w: 0, h: 0 };
static mut PICKER_SAVE: Option<SavedRegion> = None;

 // Cursor state (software-drawn arrow with background save)
const CUR_W: usize = 16;
const CUR_H: usize = 16;
static mut CUR_VISIBLE: bool = true;
static mut CUR_DRAWN: bool = false;
static mut CUR_X: i32 = 200;
static mut CUR_Y: i32 = 200;
static mut CUR_SAVE: [u32; CUR_W * CUR_H] = [0; CUR_W * CUR_H];

// cursor bitmap: 0=transparent, 1=black outline, 2=white fill (16x16) 256 bits
const CUR_BLACK: u32 = 0x000000;
const CUR_WHITE: u32 = 0xFFFFFF;

// put the cursor bitmap in `.data` for the same reason as the font table:
// if `.rodata` isn't loaded by the boot pipeline, the cursor can disappear
#[link_section = ".data"]
static CUR_BITMAP: [u8; CUR_W * CUR_H] = [
    1,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    1,2,1,0,0,0,0,0,0,0,0,0,0,0,0,0,
    1,2,2,1,0,0,0,0,0,0,0,0,0,0,0,0,
    1,2,2,2,1,0,0,0,0,0,0,0,0,0,0,0,
    1,2,2,2,2,1,0,0,0,0,0,0,0,0,0,0,
    1,2,2,2,2,2,1,0,0,0,0,0,0,0,0,0,
    1,2,2,2,2,2,2,1,0,0,0,0,0,0,0,0,
    1,2,2,2,2,2,2,2,1,0,0,0,0,0,0,0,
    1,2,2,2,2,2,2,2,2,1,0,0,0,0,0,0,
    1,2,2,2,2,2,2,2,2,2,1,0,0,0,0,0,
    1,2,2,2,2,2,1,1,1,1,1,1,0,0,0,0,
    1,2,2,1,2,2,1,0,0,0,0,0,0,0,0,0,
    1,1,1,0,1,2,2,1,0,0,0,0,0,0,0,0,
    0,0,0,0,1,2,2,1,0,0,0,0,0,0,0,0,
    0,0,0,0,0,1,2,2,1,0,0,0,0,0,0,0,
    0,0,0,0,0,0,1,1,1,0,0,0,0,0,0,0,
];

// ----------------------------------------------------------------------------
// Taskbar icon bitmaps (16x16) with 2-tone rendering
// 0 = transparent, outline bits render with ICON_OUTLINE, fill bits render with ICON_FILL.
// We put these in .text so they survive "text-only" kernel packaging.
// ----------------------------------------------------------------------------

#[repr(C)]
struct Icon16 {
    outline: [u16; 16],
    fill: [u16; 16],
}

const ICON_OUTLINE: u32 = 0x0F172A;
const ICON_FILL: u32    = 0xE5E7EB;
const ICON_ACTIVE: u32  = 0x2563EB; // active highlight bg
const ICON_HOVER: u32   = 0x1F2937; // hover highlight bg

// icon data
#[link_section = ".text"]
static ICON_TERM: Icon16 = Icon16 {
    outline: [
        0x0000, 0x7FFC, 0x4004, 0x4004, 0x4184, 0x4244, 0x4224, 0x4244,
        0x4184, 0x4044, 0x4004, 0x4004, 0x4004, 0x7FFC, 0x0000, 0x0000,
    ],
    fill: [
        0x0000, 0x0000, 0x3FF8, 0x3FF8, 0x3E78, 0x3C38, 0x3C18, 0x3C38,
        0x3E78, 0x3FB8, 0x3E18, 0x3FF8, 0x3FF8, 0x0000, 0x0000, 0x0000,
    ],
};

#[link_section = ".text"]
static ICON_NET: Icon16 = Icon16 {
    outline: [
        0x0000, 0x0000, 0x1FF8, 0x2004, 0x4002, 0x4002, 0x1818, 0x0810,
        0x0660, 0x0240, 0x0240, 0x0180, 0x0180, 0x0000, 0x0000, 0x0000,
    ],
    fill: [
        0x0000, 0x0000, 0x0000, 0x1FF8, 0x3C3C, 0x300C, 0x0000, 0x0660,
        0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    ],
};

#[link_section = ".text"]
static ICON_LOCK: Icon16 = Icon16 {
    outline: [
        0x0000, 0x03C0, 0x0420, 0x0810, 0x0810, 0x0810, 0x0FF0, 0x1FF8,
        0x1008, 0x1008, 0x1008, 0x1008, 0x1FF8, 0x0000, 0x0000, 0x0000,
    ],
    fill: [
        0x0000, 0x0000, 0x03C0, 0x07E0, 0x07E0, 0x07E0, 0x0000, 0x0000,
        0x0FF0, 0x0FF0, 0x0FF0, 0x0FF0, 0x0000, 0x0000, 0x0000, 0x0000,
    ],
};

#[link_section = ".text"]
static ICON_ABOUT: Icon16 = Icon16 {
    outline: [
        0x0000, 0x07E0, 0x1818, 0x2004, 0x2184, 0x4182, 0x4002, 0x4182,
        0x4182, 0x4002, 0x4182, 0x2184, 0x2004, 0x1818, 0x07E0, 0x0000,
    ],
    fill: [
        0x0000, 0x0000, 0x07E0, 0x1FF8, 0x1E78, 0x3E7C, 0x3FFC, 0x3E7C,
        0x3E7C, 0x3FFC, 0x3E7C, 0x1E78, 0x1FF8, 0x07E0, 0x0000, 0x0000,
    ],
};

#[link_section = ".text"]
static ICON_FILES: Icon16 = Icon16 {
    outline: [
        0x0000, 0x0000, 0x3FF8, 0x2038, 0x2048, 0x2048, 0x2004, 0x2004,
        0x2004, 0x2004, 0x2004, 0x2004, 0x3FFC, 0x0000, 0x0000, 0x0000,
    ],
    fill: [
        0x0000, 0x0000, 0x0000, 0x1FC0, 0x1F80, 0x1F80, 0x1FF8, 0x1FF8,
        0x1FF8, 0x1FF8, 0x1FF8, 0x1FF8, 0x0000, 0x0000, 0x0000, 0x0000,
    ],
};

#[link_section = ".text"]
#[link_section = ".text"]
static ICON_WEB: Icon16 = Icon16 {
    outline: [
        0x0000, 0x07E0, 0x1FF8, 0x3C3C, 0x781E, 0x700E, 0x700E, 0x700E,
        0x700E, 0x700E, 0x700E, 0x781E, 0x3C3C, 0x1FF8, 0x07E0, 0x0000,
    ],
    fill: [
        0x0000, 0x0000, 0x07E0, 0x0FF0, 0x1FF8, 0x1A58, 0x1A58, 0x1FF8,
        0x1FF8, 0x1A58, 0x1A58, 0x1FF8, 0x0FF0, 0x07E0, 0x0000, 0x0000,
    ],
};

#[link_section = ".text"]
static ICON_REG: Icon16 = Icon16 {
    outline: [
        0x0000, 0x3CF0, 0x2080, 0x2080, 0x3CF0, 0x0F00, 0x0800, 0x0800,
        0x0F00, 0x3C00, 0x2000, 0x2000, 0x3C00, 0x0000, 0x0000, 0x0000,
    ],
    fill: [
        0x0000, 0x0000, 0x1C70, 0x1C70, 0x0000, 0x0000, 0x0700, 0x0700,
        0x0000, 0x0000, 0x1C00, 0x1C00, 0x0000, 0x0000, 0x0000, 0x0000,
    ],
};

#[inline]
fn draw_icon16_scaled(x: i32, y: i32, scale: i32, icon: &Icon16, outline: u32, fill: u32) {
    if scale <= 0 { return; }
    for row in 0..16 {
        let o = icon.outline[row];
        let f = icon.fill[row];
        for col in 0..16 {
            let bit = 1u16 << (15 - col);
            let px = x + col as i32 * scale;
            let py = y + row as i32 * scale;

            if (o & bit) != 0 {
                fb::fill_rect(px.max(0) as usize, py.max(0) as usize, scale as usize, scale as usize, outline);
            } else if (f & bit) != 0 {
                fb::fill_rect(px.max(0) as usize, py.max(0) as usize, scale as usize, scale as usize, fill);
            }
        }
    }
}

#[inline]
fn icon_for_index(i: usize) -> &'static Icon16 {
    match i {
        0 => &ICON_TERM,
        1 => &ICON_NET,
        2 => &ICON_LOCK,
        3 => &ICON_ABOUT,
        4 => &ICON_FILES,
        5 => &ICON_REG,
        6 => &ICON_WEB,
        _ => &ICON_REG,
    }
}
// Theme
pub const SHELL_BG_COLOR: u32 = 0x0F172A;// window body background
pub const SHELL_FG_COLOR: u32 = 0xE5E7EB;

const DESKTOP_BG_TOP: u32 = 0x0B1020;
const DESKTOP_BG_BOT: u32 = 0x102030;
const TOPBAR_BG: u32      = 0x0B1220;
const DOCK_BG: u32        = 0x111827;
const ACCENT: u32         = 0x38BDF8;

const WINDOW_BRD: u32     = 0x334155;
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
pub fn point_in_shell_content(x: i32, y: i32) -> bool {
    let l = shell_content_left();
    let t = shell_content_top();
    let w = shell_content_w();
    let h = shell_content_h();
    x >= l && y >= t && x < l + w && y < t + h
}
pub fn shell_footer_x() -> i32 { unsafe { SHELL_FOOT.x } }
pub fn shell_footer_y() -> i32 { unsafe { SHELL_FOOT.y } }
pub fn shell_footer_w() -> i32 { unsafe { SHELL_FOOT.w } }
pub fn shell_footer_h() -> i32 { unsafe { SHELL_FOOT.h } }

pub fn shell_is_visible() -> bool { unsafe { SHELL_VISIBLE } }
pub fn shell_is_dragging() -> bool { unsafe { DRAG_ACTIVE } }
pub fn shell_is_maximized() -> bool { unsafe { SHELL_MAXIMIZED } }

pub fn screen_w() -> i32 { unsafe { SCREEN_W } }
pub fn screen_h() -> i32 { unsafe { SCREEN_H } }

pub fn ui_mode() -> UiMode { unsafe { UI_MODE } }

/// Switch between desktop and full-screen login UI.
///
/// - Login: hides dock + shell window frame and disables window chrome hit-tests.
/// - Desktop: normal UI.
pub fn set_ui_mode(mode: UiMode) {
    unsafe { UI_MODE = mode; }
}

/// Force shell visibility (used when transitioning from login -> desktop).
pub fn set_shell_visible(vis: bool) {
    unsafe {
        SHELL_VISIBLE = vis;
        if !vis { DRAG_ACTIVE = false; }
    }
}

/// Force maximize/restore the shell window.
pub fn set_shell_maximized(max: bool) {
    unsafe {
        if max == SHELL_MAXIMIZED { return; }
        if max {
            SHELL_RESTORE = SHELL_OUTER;
            recompute_dock_layout();
            // Fill the usable area below the topbar and above the dock.
            let top = 32 + 8;
            let w = (SCREEN_W - 16).max(200);
            let h = (DOCK_RECT.y - top - 12).max(200);
            SHELL_OUTER = Rect { x: 8, y: top, w, h };
            SHELL_MAXIMIZED = true;
        } else {
            SHELL_OUTER = SHELL_RESTORE;
            SHELL_MAXIMIZED = false;
        }
        recalc_layout();
    }
}

/// Set the shell window title text.
///
/// Note: this does not immediately repaint the header; callers should
/// trigger a normal redraw (e.g. clear_shell_content_and_frame()).
pub fn set_shell_title(title: &'static str) {
    unsafe { SHELL_TITLE_TEXT = title; }
}

pub fn init_from_bootloader(info: *const fb::BootVideoInfoRaw) {
    unsafe {
        if !fb::init_from_bootinfo(info) {
            serial_write_str("GUI: framebuffer init failed.\n");
            loop {}
        }

        SCREEN_W = fb::width() as i32;
        SCREEN_H = fb::height() as i32;

        // Pick a nice default shell placement for big screens.
        if SCREEN_W >= 1600 {
            SHELL_OUTER.x = 160;
            SHELL_OUTER.y = 110;
        }
        SHELL_RESTORE = SHELL_OUTER;

        recalc_layout();
        recompute_dock_layout();

        redraw_all();
        // Ensure the shell content area starts clean (avoid leftover framebuffer garbage)
        if SHELL_VISIBLE { clear_shell_content_nocursor(); }
        serial_write_str("GUI: initialized.\n");
    }
}

/// Wrap a bunch of drawing operations so we don't leave cursor artifacts.
pub fn begin_paint() { cursor_restore(); }
pub fn end_paint()   { cursor_redraw(); }

pub fn redraw_all() {
    unsafe {
        begin_paint();
        match UI_MODE {
            UiMode::Login => {
                draw_login_background();
            }
            UiMode::Desktop => {
                draw_desktop();
                if SHELL_VISIBLE {
                    recalc_layout();
                    draw_shell_window_frame();
                }
            }
        }
        end_paint();
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
            w: SHELL_OUTER.w - pad * 2,
            h: (SHELL_OUTER.h - hdr - pad * 3 - 18).max(0),
        };
        SHELL_FOOT = Rect {
            x: SHELL_OUTER.x + pad,
            y: SHELL_OUTER.y + SHELL_OUTER.h - (pad + 18),
            w: SHELL_OUTER.w - pad * 2,
            h: 18,
        };

        // Traffic lights (Mac-style, left side)
        BTN_CLOSE = Rect { x: SHELL_OUTER.x + 16, y: SHELL_OUTER.y + 12, w: 10, h: 10 };
        BTN_MIN   = Rect { x: SHELL_OUTER.x + 32, y: SHELL_OUTER.y + 12, w: 10, h: 10 };
        BTN_MAX   = Rect { x: SHELL_OUTER.x + 48, y: SHELL_OUTER.y + 12, w: 10, h: 10 };
    }
}

fn recompute_dock_layout() {
    unsafe {
        let w = SCREEN_W.max(0) as i32;
        let h = SCREEN_H.max(0) as i32;
        if w <= 0 || h <= 0 { return; }

        // Taskbar rect (we reuse DOCK_RECT as the "reserved" bottom area)
        let tb_y = (h - TASKBAR_H).max(0);
        DOCK_RECT = Rect { x: 0, y: tb_y, w, h: TASKBAR_H };

        // Windows 11-ish centered icons inside the taskbar
        let icon_box: i32 = 36;     // clickable box (we draw a 16x16 icon scaled 2x inside)
        let gap: i32 = 10;
        let group_w = (DOCK_ICON_COUNT as i32) * icon_box + ((DOCK_ICON_COUNT as i32) - 1) * gap;
        let start_x = (w - group_w) / 2;
        let y = tb_y + (TASKBAR_H - icon_box) / 2;

        let mut x = start_x;
        for i in 0..DOCK_ICON_COUNT {
            DOCK_ICONS[i] = Rect { x, y, w: icon_box, h: icon_box };
            x += icon_box + gap;
        }
    }
}

pub fn clear_shell_content() {
    unsafe {
        begin_paint();
        clear_shell_content_nocursor();
        end_paint();
    }
}

/// Clear the shell content *and* ensure the window frame exists.
/// (Useful if something repainted the desktop without redrawing the shell.)
pub fn clear_shell_content_and_frame() {
    unsafe {
        if !SHELL_VISIBLE { return; }
        begin_paint();
        draw_shell_window_frame();
        clear_shell_content_nocursor();
        end_paint();
    }
}

/// Same as `clear_shell_content_and_frame()`, but does NOT touch cursor state.
///
/// Use this when you want to batch multiple draws inside one begin_paint()/end_paint()
/// without flicker or cursor trails.
pub fn clear_shell_content_and_frame_nocursor() {
    unsafe {
        if !SHELL_VISIBLE { return; }
        draw_shell_window_frame();
        clear_shell_content_nocursor();
    }
}


fn clear_shell_content_nocursor() {
    unsafe {
        let r = SHELL_CONTENT;
        if r.w > 0 && r.h > 0 {
            fb::fill_rect(r.x.max(0) as usize, r.y.max(0) as usize, r.w.max(0) as usize, r.h.max(0) as usize, SHELL_BG_COLOR);
        }
        // footer background too (so shell can just draw text/caret)
        let f = SHELL_FOOT;
        if f.w > 0 && f.h > 0 {
            fb::fill_rect(f.x.max(0) as usize, f.y.max(0) as usize, f.w.max(0) as usize, f.h.max(0) as usize, SHELL_BG_COLOR);
        }
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
    if w0 > rr * 2 {
        fb::fill_rect(x0 + rr, y0, w0 - rr * 2, h0, color);
    }
    if h0 > rr * 2 {
        fb::fill_rect(x0, y0 + rr, w0, h0 - rr * 2, color);
    }

    let r2 = (r * r) as i32;
    for dy in 0..r {
        for dx in 0..r {
            let cx = r - 1 - dx;
            let cy = r - 1 - dy;
            if cx * cx + cy * cy <= r2 {
                let px0 = x + dx;
                let py0 = y + dy;
                let px1 = x + w - 1 - dx;
                let py1 = y + h - 1 - dy;
                if px0 >= 0 && py0 >= 0 { fb::set_pixel(px0 as usize, py0 as usize, color); }
                if px1 >= 0 && py0 >= 0 { fb::set_pixel(px1 as usize, py0 as usize, color); }
                if px0 >= 0 && py1 >= 0 { fb::set_pixel(px0 as usize, py1 as usize, color); }
                if px1 >= 0 && py1 >= 0 { fb::set_pixel(px1 as usize, py1 as usize, color); }
            }
        }
    }
}

/// Fill a rounded-rectangle without touching cursor state.
///
/// Call begin_paint()/end_paint() around batches for speed.
pub fn fill_round_rect_nocursor(x: i32, y: i32, w: i32, h: i32, r: i32, color: u32) {
    fill_round_rect(x, y, w, h, r, color);
}

fn draw_login_background() {
    unsafe {
        let w = SCREEN_W.max(0) as usize;
        let h = SCREEN_H.max(0) as usize;
        if w == 0 || h == 0 { return; }

        // Windows-ish dark blue gradient with a soft vertical highlight.
        let top = 0x070A14;
        let bot = 0x0B1B3A;
        let hi  = 0x0F2A55;

        let den = (h.saturating_sub(1) as u32).max(1);
        let mid = (h as i32 / 2).max(1);

        for y in 0..h {
            let base = lerp_color(top, bot, y as u32, den);
            let dist = ((y as i32) - mid).abs() as u32;
            let t = (dist * 255) / (mid as u32);
            let t = t.min(255);
            let glow = lerp_color(hi, base, t, 255);
            fb::fill_rect(0, y, w, 1, glow);
        }
    }
}

fn paint_taskbar_and_dock() {
    unsafe {
        let w = SCREEN_W.max(0) as usize;
        let h = SCREEN_H.max(0) as i32;
        if w == 0 || h <= 0 { return; }

        // Taskbar background (bottom)
        let tb_y = (h - TASKBAR_H).max(0);
        fb::fill_rect(0, tb_y as usize, w, TASKBAR_H as usize, TASKBAR_BG);
        fb::fill_rect(0, tb_y as usize, w, 1, TASKBAR_TOP);

        // Taskbar icons (centered)
        recompute_dock_layout();

        let active_app = if SHELL_VISIBLE { crate::shell::active_taskbar_index() } else { 255u8 };

        for i in 0..DOCK_ICON_COUNT {
            let r = DOCK_ICONS[i];

            let hovered = r.contains(CUR_X, CUR_Y);
            let shell_active = (i == 0 && SHELL_VISIBLE);

            let bg = if shell_active {
                ICON_ACTIVE
            } else if hovered {
                ICON_HOVER
            } else {
                TASKBAR_BG
            };

            if bg != TASKBAR_BG {
                fill_round_rect(r.x, r.y, r.w, r.h, 10, bg);
            }

            // Draw 16x16 icon scaled 2x (32x32) centered in the box (36x36)
            let scale = 2;
            let icon_px = 16 * scale;
            let ix = r.x + (r.w - icon_px) / 2;
            let iy = r.y + (r.h - icon_px) / 2;
            draw_icon16_scaled(ix, iy, scale, icon_for_index(i), ICON_OUTLINE, ICON_FILL);

            // Running indicator dot for current active view inside the shell
            if i as u8 == active_app {
                let dot_w = 8;
                let dot_x = r.x + (r.w - dot_w) / 2;
                fb::fill_rect(dot_x as usize, (r.y + r.h + 2) as usize, dot_w as usize, 2, 0xE5E7EB);
            }
        }

        // Clock (bottom-right)
        let dt = time::rtc_now();
        let mut buf = [0u8; 32];
        let n = time::format_datetime(&mut buf, dt);
        let s = unsafe { core::str::from_utf8_unchecked(&buf[..n]) };

        let text_w = (n as i32) * (font::FONT_W as i32);
        let x = (SCREEN_W - 16 - text_w).max(12);
        let y = tb_y + (TASKBAR_H - font::FONT_H as i32) / 2;

        // Clear a small region behind the text (prevents leftovers on variable widths)
        fb::fill_rect((x - 8).max(0) as usize, y.max(0) as usize, (text_w + 16) as usize, font::FONT_H, TASKBAR_BG);
        draw_text_nocursor(x, y, s, 0xE5E7EB, TASKBAR_BG);
    }
}

/// Redraw just the bottom taskbar + dock + clock (does NOT touch the shell window contents).
pub fn redraw_taskbar() {
    unsafe {
        begin_paint();
        paint_taskbar_and_dock();
        end_paint();
    }
}

fn draw_desktop() {
    unsafe {
        let w = SCREEN_W.max(0) as usize;
        let h = SCREEN_H.max(0) as usize;
        if w == 0 || h == 0 { return; }

        // Wallpaper background
        crate::wallpaper::draw_fullscreen();

// Top bar
        fb::fill_rect(0, 0, w, 32, TOPBAR_BG);
        fb::fill_rect(0, 31, w, 1, ACCENT);
        draw_text_nocursor(12, 8, "O t h e l l o  O S", 0xE5E7EB, TOPBAR_BG);

        // Bottom taskbar (icons + clock)
        paint_taskbar_and_dock();
    }
}

// -----------------------------------------------------------------------------
// Desktop right-click UI helpers
// -----------------------------------------------------------------------------

#[inline]
fn save_region(r: Rect) -> Option<SavedRegion> {
    unsafe {
        let Some(r) = clip_to_screen(r) else { return None; };
        let w = r.w.max(0) as usize;
        let h = r.h.max(0) as usize;
        if w == 0 || h == 0 { return None; }

        let mut buf = Vec::with_capacity(w.saturating_mul(h));
        for yy in 0..h {
            for xx in 0..w {
                buf.push(fb::get_pixel((r.x as usize) + xx, (r.y as usize) + yy));
            }
        }
        Some(SavedRegion { x: r.x, y: r.y, w: r.w, h: r.h, buf })
    }
}

#[inline]
fn restore_region(s: &SavedRegion) {
    let w = s.w.max(0) as usize;
    let h = s.h.max(0) as usize;
    if w == 0 || h == 0 { return; }
    for yy in 0..h {
        for xx in 0..w {
            let idx = yy * w + xx;
            if idx >= s.buf.len() { return; }
            fb::set_pixel((s.x as usize) + xx, (s.y as usize) + yy, s.buf[idx]);
        }
    }
}

fn close_context_menu() {
    unsafe {
        if !CTX_OPEN { return; }
        begin_paint();
        if let Some(s) = CTX_SAVE.take() {
            restore_region(&s);
        }
        CTX_OPEN = false;
        end_paint();
    }
}

fn draw_context_menu() {
    unsafe {
        let r = CTX_RECT;
        // Border + body
        fill_round_rect(r.x, r.y, r.w, r.h, 10, 0x334155);
        fill_round_rect(r.x + 1, r.y + 1, r.w - 2, r.h - 2, 10, 0x0F172A);

        // Single item
        let item = CTX_ITEM_BG;
        fill_round_rect(item.x, item.y, item.w, item.h, 8, 0x111827);
        draw_text_nocursor(item.x + 10, item.y + 5, "Desktop background", 0xE5E7EB, 0x111827);
    }
}

fn open_context_menu_at(px: i32, py: i32) {
    unsafe {
        // Only one overlay at a time.
        if PICKER_OPEN { close_wallpaper_picker(); }
        if CTX_OPEN { close_context_menu(); }

        let sw = SCREEN_W;
        let sh = SCREEN_H;
        if sw <= 0 || sh <= 0 { return; }

        let menu_w: i32 = 240;
        let menu_h: i32 = 44;
        let mut x = px;
        let mut y = py;

        // Keep on-screen
        if x + menu_w > sw { x = (sw - menu_w - 2).max(2); }
        if y + menu_h > sh { y = (sh - menu_h - 2).max(2); }
        if x < 2 { x = 2; }
        if y < 2 { y = 2; }

        CTX_RECT = Rect { x, y, w: menu_w, h: menu_h };
        CTX_ITEM_BG = Rect { x: x + 8, y: y + 8, w: menu_w - 16, h: 28 };

        begin_paint();
        CTX_SAVE = save_region(CTX_RECT);
        CTX_OPEN = true;
        draw_context_menu();
        end_paint();
    }
}

fn close_wallpaper_picker() {
    unsafe {
        if !PICKER_OPEN { return; }
        begin_paint();
        if let Some(s) = PICKER_SAVE.take() {
            restore_region(&s);
        }
        PICKER_OPEN = false;
        end_paint();
    }
}

fn draw_wallpaper_preview(dst_x: i32, dst_y: i32, pw: i32, ph: i32, idx: usize) {
    if pw <= 0 || ph <= 0 { return; }
    let pw_u = pw as usize;
    let ph_u = ph as usize;
    let wp = &crate::wallpaper::WALLPAPERS[idx];
    for yy in 0..ph_u {
        for xx in 0..pw_u {
            let c = wp.sample(xx, yy, pw_u, ph_u);
            fb::set_pixel((dst_x as usize) + xx, (dst_y as usize) + yy, c);
        }
    }
}

fn draw_wallpaper_picker() {
    unsafe {
        let r = PICKER_RECT;
        // Shadow + border + body
        fill_round_rect(r.x + 6, r.y + 8, r.w, r.h, 14, 0x000000);
        fill_round_rect(r.x, r.y, r.w, r.h, 14, 0x334155);
        fill_round_rect(r.x + 1, r.y + 1, r.w - 2, r.h - 2, 14, 0x0F172A);

        // Header
        fill_round_rect(r.x + 1, r.y + 1, r.w - 2, 36, 14, 0x111827);
        fb::fill_rect((r.x + 1) as usize, (r.y + 35) as usize, (r.w - 2) as usize, 1, 0x334155);
        draw_text_nocursor(r.x + 14, r.y + 11, "Desktop background", 0xF3F4F6, 0x111827);

        // Close button
        let c = PICKER_CLOSE;
        fill_round_rect(c.x, c.y, c.w, c.h, 8, 0xEF4444);
        draw_text_nocursor(c.x + 6, c.y + 2, "X", 0xFFFFFF, 0xEF4444);

        // List
        let n = crate::wallpaper::count();
        let cur = crate::wallpaper::current_index();
        let list_x = r.x + 12;
        let list_w = r.w - 24;
        let mut y = r.y + 48;
        let row_h: i32 = 62;

        for i in 0..n {
            let row = Rect { x: list_x, y, w: list_w, h: row_h - 8 };

            // Row background (highlight current)
            let bg = if i == cur { 0x1F2937 } else { 0x0F172A };
            fill_round_rect(row.x, row.y, row.w, row.h, 12, bg);
            fb::fill_rect(row.x.max(0) as usize, row.y.max(0) as usize, row.w.max(0) as usize, 1, 0x334155);

            // Preview box
            let pv_w = 96;
            let pv_h = 54;
            let pv_x = row.x + 10;
            let pv_y = row.y + (row.h - pv_h) / 2;
            fill_round_rect(pv_x, pv_y, pv_w, pv_h, 10, 0x334155);
            fill_round_rect(pv_x + 1, pv_y + 1, pv_w - 2, pv_h - 2, 10, 0x0B1220);
            draw_wallpaper_preview(pv_x + 2, pv_y + 2, pv_w - 4, pv_h - 4, i);

            // Name
            draw_text_nocursor(pv_x + pv_w + 12, row.y + 18, crate::wallpaper::WALLPAPERS[i].name(), 0xE5E7EB, bg);
            if i == cur {
                draw_text_nocursor(pv_x + pv_w + 12, row.y + 34, "(current)", 0x94A3B8, bg);
            }

            y += row_h;
        }
    }
}

fn open_wallpaper_picker() {
    unsafe {
        if PICKER_OPEN { return; }
        if CTX_OPEN { close_context_menu(); }

        let sw = SCREEN_W;
        let sh = SCREEN_H;
        if sw <= 0 || sh <= 0 { return; }

        let n = crate::wallpaper::count().max(1);
        let row_h: i32 = 62;
        let mut w: i32 = 540;
        w = w.min(sw - 40).max(280);
        let mut h: i32 = 48 + (n as i32) * row_h + 18;
        h = h.min(sh - 40).max(180);

        let x = (sw - w) / 2;
        let y = (sh - h) / 2;
        PICKER_RECT = Rect { x, y, w, h };
        PICKER_CLOSE = Rect { x: x + w - 30, y: y + 8, w: 20, h: 20 };

        begin_paint();
        PICKER_SAVE = save_region(PICKER_RECT);
        PICKER_OPEN = true;
        draw_wallpaper_picker();
        end_paint();
    }
}

fn picker_hit_wallpaper(px: i32, py: i32) -> Option<usize> {
    unsafe {
        if !PICKER_OPEN { return None; }
        let r = PICKER_RECT;
        let n = crate::wallpaper::count();
        if n == 0 { return None; }

        let list_x0 = r.x + 12;
        let list_x1 = r.x + r.w - 12;
        let list_y0 = r.y + 48;
        let row_h: i32 = 62;

        if px < list_x0 || px >= list_x1 || py < list_y0 { return None; }
        let idx = ((py - list_y0) / row_h) as isize;
        if idx < 0 { return None; }
        let i = idx as usize;
        if i >= n { None } else { Some(i) }
    }
}

fn repaint_visible_desktop_after_wallpaper_change() {
    unsafe {
        begin_paint();

        let sw = SCREEN_W;
        let sh = SCREEN_H;
        if sw <= 0 || sh <= 0 {
            end_paint();
            return;
        }

        if !SHELL_VISIBLE {
            draw_desktop();
            end_paint();
            return;
        }

        let win = shell_paint_rect(SHELL_OUTER);
        // Top
        if win.y > 0 {
            draw_desktop_region(Rect { x: 0, y: 0, w: sw, h: win.y });
        }
        // Bottom
        let by = win.y + win.h;
        if by < sh {
            draw_desktop_region(Rect { x: 0, y: by, w: sw, h: sh - by });
        }
        // Left
        if win.x > 0 {
            draw_desktop_region(Rect { x: 0, y: win.y, w: win.x, h: win.h });
        }
        // Right
        let rx = win.x + win.w;
        if rx < sw {
            draw_desktop_region(Rect { x: rx, y: win.y, w: sw - rx, h: win.h });
        }

        end_paint();
    }
}


// ----------------------------------------------------------------------------
// Fast window move helpers (damage redraw)
// ----------------------------------------------------------------------------

#[inline]
fn clip_to_screen(mut r: Rect) -> Option<Rect> {
    unsafe {
        let sw = SCREEN_W;
        let sh = SCREEN_H;
        if sw <= 0 || sh <= 0 { return None; }

        // clip left/top
        if r.x < 0 {
            r.w -= -r.x;
            r.x = 0;
        }
        if r.y < 0 {
            r.h -= -r.y;
            r.y = 0;
        }
        // clip right/bottom
        let max_x = sw;
        let max_y = sh;
        let over_x = (r.x + r.w) - max_x;
        if over_x > 0 { r.w -= over_x; }
        let over_y = (r.y + r.h) - max_y;
        if over_y > 0 { r.h -= over_y; }

        if r.w <= 0 || r.h <= 0 { None } else { Some(r) }
    }
}

#[inline]
fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    let w = x1 - x0;
    let h = y1 - y0;
    if w <= 0 || h <= 0 { None } else { Some(Rect { x: x0, y: y0, w, h }) }
}

/// Paint rect for the shell window including drop shadow.
#[inline]
fn shell_paint_rect(outer: Rect) -> Rect {
    // Shadow extends +6,+8; include a small margin on all sides.
    Rect { x: outer.x - 2, y: outer.y - 2, w: outer.w + 10, h: outer.h + 12 }
}

/// Clip a (src,dst) move pair to the screen while keeping them identical in size.
///
/// Why: framebuffer blit does its own clipping. If the GUI computes damage/exposed
/// regions using *unclipped* rects (especially near the right/bottom edges where
/// shadows extend off-screen), we can end up repainting desktop pixels *over* the
/// moved window, causing "smeared" / broken text when dragging.
#[inline]
fn clip_move_pair(src: Rect, dst: Rect) -> Option<(Rect, Rect)> {
    unsafe {
        let sw = SCREEN_W;
        let sh = SCREEN_H;
        if sw <= 0 || sh <= 0 { return None; }

        let mut sx = src.x;
        let mut sy = src.y;
        let mut dx = dst.x;
        let mut dy = dst.y;
        let mut w  = src.w;
        let mut h  = src.h;
        if w <= 0 || h <= 0 { return None; }

        // Clip left/top: keep src/dst aligned.
        if sx < 0 { let shft = -sx; sx = 0; dx += shft; w -= shft; }
        if sy < 0 { let shft = -sy; sy = 0; dy += shft; h -= shft; }
        if dx < 0 { let shft = -dx; dx = 0; sx += shft; w -= shft; }
        if dy < 0 { let shft = -dy; dy = 0; sy += shft; h -= shft; }

        // Clip right/bottom edges.
        w = w.min(sw - sx).min(sw - dx);
        h = h.min(sh - sy).min(sh - dy);

        if w <= 0 || h <= 0 { return None; }
        Some((Rect { x: sx, y: sy, w, h }, Rect { x: dx, y: dy, w, h }))
    }
}

/// Redraw desktop elements only within a region (no shell).
fn draw_desktop_region(r: Rect) {
    unsafe {
        let Some(r) = clip_to_screen(r) else { return; };
        let sw = SCREEN_W as usize;
        let sh = SCREEN_H as usize;
        if sw == 0 || sh == 0 { return; }

        // Wallpaper background only for the affected region
        let y0 = r.y.max(0) as usize;
        let y1 = (r.y + r.h).min(SCREEN_H) as usize;
        let x0 = r.x.max(0) as usize;
        let w  = r.w.max(0) as usize;

        crate::wallpaper::draw_region(x0, y0, w, y1.saturating_sub(y0));

        // Topbar overlap
        if r.y < 32 {
            let top_h = (32 - r.y).min(r.h).max(0) as usize;
            fb::fill_rect(x0, y0, w, top_h, TOPBAR_BG);
            // accent line at y=31
            if 31 >= y0 && 31 < y1 {
                fb::fill_rect(x0, 31, w, 1, ACCENT);
            }

            // Re-draw the topbar title if our region touches it.
            // (Otherwise, dragging windows across the topbar will erase the text.)
            if r.x < 240 && (r.x + r.w) > 0 {
                draw_text_nocursor(12, 8, "O t h e l l o  O S", 0xE5E7EB, TOPBAR_BG);
            }
        }

        // Dock overlap: redraw full dock (small) if intersecting
        recompute_dock_layout();
        if let Some(_) = intersect(r, DOCK_RECT) {
            paint_taskbar_and_dock();
        }
    }
}

/// Redraw only the regions of `old` that are no longer covered by `new`.
fn redraw_exposed(old: Rect, new: Rect) {
    if let Some(i) = intersect(old, new) {
        // top
        if i.y > old.y {
            draw_desktop_region(Rect { x: old.x, y: old.y, w: old.w, h: i.y - old.y });
        }
        // bottom
        let old_bot = old.y + old.h;
        let i_bot = i.y + i.h;
        if i_bot < old_bot {
            draw_desktop_region(Rect { x: old.x, y: i_bot, w: old.w, h: old_bot - i_bot });
        }
        // left
        if i.x > old.x {
            draw_desktop_region(Rect { x: old.x, y: i.y, w: i.x - old.x, h: i.h });
        }
        // right
        let old_right = old.x + old.w;
        let i_right = i.x + i.w;
        if i_right < old_right {
            draw_desktop_region(Rect { x: i_right, y: i.y, w: old_right - i_right, h: i.h });
        }
    } else {
        // No overlap: old area is fully exposed
        draw_desktop_region(old);
    }
}

fn draw_shell_window_frame() {
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
        fb::fill_rect((x + 1) as usize, (y + 33) as usize, (w - 2) as usize, 1, WINDOW_BRD);

        // Title
        draw_text_nocursor(x + 76, y + 10, SHELL_TITLE_TEXT, 0xF3F4F6, WINDOW_HDR);

        // Window controls (traffic lights)
        let close = BTN_CLOSE;
        let min   = BTN_MIN;
        let max   = BTN_MAX;
        fill_round_rect(close.x, close.y, close.w, close.h, 5, 0xEF4444);
        fill_round_rect(min.x,   min.y,   min.w,   min.h,   5, 0xF59E0B);
        fill_round_rect(max.x,   max.y,   max.w,   max.h,   5, 0x10B981);
    }
}

/// Mouse + cursor handler:
/// - title bar drag moves shell
/// - traffic lights: close/min/max
/// - dock icon 0 toggles shell visibility
pub fn ui_handle_mouse(ms: MouseState) -> UiAction {
    unsafe {
        // Edge detection for click/drag
        let left_edge = ms.left && !LAST_LEFT;
        let left_release = !ms.left && LAST_LEFT;
        LAST_LEFT = ms.left;

        let right_edge = ms.right && !LAST_RIGHT;
        let _right_release = !ms.right && LAST_RIGHT;
        LAST_RIGHT = ms.right;

        // In full-screen login mode, we only want the software cursor to move.
        // No window chrome, no dock hit-tests.
        if UI_MODE == UiMode::Login {
            if left_release { DRAG_ACTIVE = false; }
            cursor_move_to(ms.x, ms.y);
            return UiAction::None;
        }

        if left_release {
            DRAG_ACTIVE = false;
        }

	    let act = 'act: loop {
            // ----------------------------------------------------------------
            // Overlays: wallpaper picker + desktop context menu
            // ----------------------------------------------------------------
            if PICKER_OPEN {
                if left_edge {
                    if PICKER_CLOSE.contains(ms.x, ms.y) {
                        close_wallpaper_picker();
                        break UiAction::None;
                    }
                    if let Some(i) = picker_hit_wallpaper(ms.x, ms.y) {
                        close_wallpaper_picker();
                        crate::wallpaper::set(i);
                        repaint_visible_desktop_after_wallpaper_change();
                        break UiAction::None;
                    }
                    if !PICKER_RECT.contains(ms.x, ms.y) {
                        close_wallpaper_picker();
                        break UiAction::None;
                    }
                }

                // Right-click anywhere outside closes the picker.
                if right_edge && !PICKER_RECT.contains(ms.x, ms.y) {
                    close_wallpaper_picker();
                    break UiAction::None;
                }

                break UiAction::None;
            }

            if CTX_OPEN {
                if left_edge {
                    if CTX_ITEM_BG.contains(ms.x, ms.y) {
                        close_context_menu();
                        open_wallpaper_picker();
                        break UiAction::None;
                    }
                    if !CTX_RECT.contains(ms.x, ms.y) {
                        close_context_menu();
                        break UiAction::None;
                    }
                }

                // Reposition menu on right-click elsewhere.
                if right_edge && !CTX_RECT.contains(ms.x, ms.y) {
                    close_context_menu();
                    recompute_dock_layout();
                    let on_desktop = ms.y >= 32
                        && !DOCK_RECT.contains(ms.x, ms.y)
                        && !(SHELL_VISIBLE && shell_paint_rect(SHELL_OUTER).contains(ms.x, ms.y));
                    if on_desktop {
                        open_context_menu_at(ms.x, ms.y);
                    }
                    break UiAction::None;
                }

                break UiAction::None;
            }

            // Open context menu on desktop right-click.
            if right_edge {
                recompute_dock_layout();
                let on_desktop = ms.y >= 32
                    && !DOCK_RECT.contains(ms.x, ms.y)
                    && !(SHELL_VISIBLE && shell_paint_rect(SHELL_OUTER).contains(ms.x, ms.y));
                if on_desktop {
                    open_context_menu_at(ms.x, ms.y);
                    break UiAction::None;
                }
            }
            // ----------------------------------------------------------------
            // Window traffic lights (close/min/max) - paint on click
            // ----------------------------------------------------------------
            if SHELL_VISIBLE && left_edge {
                if BTN_CLOSE.contains(ms.x, ms.y) {
                    SHELL_VISIBLE = false;
                    DRAG_ACTIVE = false;

                    // repaint desktop (cursor hidden to prevent trails)
                    cursor_restore();
                    draw_desktop();
	                            break 'act UiAction::ShellVisibilityChanged;
                }

                if BTN_MIN.contains(ms.x, ms.y) {
                    SHELL_VISIBLE = false;
                    DRAG_ACTIVE = false;

                    cursor_restore();
                    draw_desktop();
	                            break 'act UiAction::ShellVisibilityChanged;
                }

                if BTN_MAX.contains(ms.x, ms.y) {
                    if !SHELL_MAXIMIZED {
                        SHELL_RESTORE = SHELL_OUTER;
                        // Fill the usable area (below topbar, above dock)
                        recompute_dock_layout();
                        SHELL_OUTER = Rect {
                            x: 8,
                            y: 32 + 8,
                            w: SCREEN_W - 16,
                            h: (DOCK_RECT.y - (32 + 8) - 12).max(200),
                        };
                        SHELL_MAXIMIZED = true;
                    } else {
                        SHELL_OUTER = SHELL_RESTORE;
                        SHELL_MAXIMIZED = false;
                    }

                    recalc_layout();

                    cursor_restore();
                    draw_desktop();
                    draw_shell_window_frame();
                    break UiAction::ShellResized;
                }
            }

            // ----------------------------------------------------------------
            // Dock: toggle shell visibility on leftmost icon
            // ----------------------------------------------------------------
            if left_edge {
                for i in 0..DOCK_ICON_COUNT {
                    if DOCK_ICONS[i].contains(ms.x, ms.y) {
                if i == 0 {
                            // Terminal / Shell icon:
                            // If the shell is hidden, show it and draw the frame.
                            // Always emit DockLaunch(0) so shell.rs can decide:
                            //   - switch to Terminal, or
                            //   - minimize if Terminal is already active.
                            if !SHELL_VISIBLE {
                                SHELL_VISIBLE = true;
                                DRAG_ACTIVE = false;

                                cursor_restore();
                                draw_desktop();
                                recalc_layout();
                                draw_shell_window_frame();
                                clear_shell_content_nocursor();
                            }

                            break 'act UiAction::DockLaunch(0);
                        } else {
                            // Launch/switch app via dock: ensure shell is visible
                            if !SHELL_VISIBLE {
                                SHELL_VISIBLE = true;
                            }
                            DRAG_ACTIVE = false;

                            cursor_restore();
                            draw_desktop();
                            recalc_layout();
                            draw_shell_window_frame();
                            clear_shell_content_nocursor();

                            break 'act UiAction::DockLaunch(i as u8);
                        }

                    }
                }
            }

            // ----------------------------------------------------------------
            // Start dragging from title bar (not on buttons)
            // ----------------------------------------------------------------
            if SHELL_VISIBLE && left_edge && !SHELL_MAXIMIZED {
                let on_title = SHELL_TITLE.contains(ms.x, ms.y);
                let on_btn = BTN_CLOSE.contains(ms.x, ms.y) || BTN_MIN.contains(ms.x, ms.y) || BTN_MAX.contains(ms.x, ms.y);
                if on_title && !on_btn {
                    DRAG_ACTIVE = true;
	                    DRAG_OFF_X = ms.x - SHELL_OUTER.x;
	                    DRAG_OFF_Y = ms.y - SHELL_OUTER.y;
                }
            }

            // ----------------------------------------------------------------
            // Dragging: fast blit move + damage redraw
            // ----------------------------------------------------------------
            if SHELL_VISIBLE && DRAG_ACTIVE && ms.left && !SHELL_MAXIMIZED {
                let old = SHELL_OUTER;
	                let mut nx = ms.x - DRAG_OFF_X;
	                let mut ny = ms.y - DRAG_OFF_Y;

                // clamp window on screen (keep above dock)
                recompute_dock_layout();
                let max_x = (SCREEN_W - old.w).max(0);
                let max_y = (DOCK_RECT.y - old.h - 12).max(32);
                nx = nx.clamp(0, max_x);
                ny = ny.clamp(32, max_y);

                if nx != old.x || ny != old.y {
	                    let old_paint = shell_paint_rect(old);

                    SHELL_OUTER.x = nx;
                    SHELL_OUTER.y = ny;
                    SHELL_RESTORE = SHELL_OUTER;
                    recalc_layout();

	                    let new_paint = shell_paint_rect(SHELL_OUTER);

	                    // Hide cursor so it doesn't get copied by the blit
	                    cursor_restore();

	                    // IMPORTANT: use the same clipped rects for both the blit
	                    // and the exposed-region redraw. Otherwise (near edges)
	                    // the framebuffer blit may clip internally, while our
	                    // exposed calculations do not, causing desktop repaint to
	                    // overwrite part of the moved window ("broken" text).
	                    if let Some((src, dst)) = clip_move_pair(old_paint, new_paint) {
	                        fb::blit_move_rect(src.x, src.y, src.w, src.h, dst.x, dst.y);
	                        redraw_exposed(src, dst);
	                    } else {
	                        // Fallback: if for some reason we can't blit, just do
	                        // a conservative desktop repaint of the old area.
	                        draw_desktop_region(old_paint);
	                    }

                    break UiAction::ShellMoved;
                }
            }

            break UiAction::None;
        };

        // Move/redraw cursor last (so it stays "Mac smooth" even during repaint)
        cursor_move_to(ms.x, ms.y);

        act
    }
}

fn toggle_maximize() {
    unsafe {
        if !SHELL_MAXIMIZED {
            SHELL_RESTORE = SHELL_OUTER;
            // Fill most of the space between topbar and dock
            let top = 32 + 16;
            let bottom = 54 + 16 + 16; // dock + padding + margin
            let w = (SCREEN_W - 32).max(200);
            let h = (SCREEN_H - top - bottom).max(200);
            SHELL_OUTER = Rect { x: 16, y: top, w, h };
            SHELL_MAXIMIZED = true;
        } else {
            SHELL_OUTER = SHELL_RESTORE;
            SHELL_MAXIMIZED = false;
        }
        recalc_layout();
    }
}

// ----------------------------------------------------------------------------
// Text drawing (8x16) with background fill
// ----------------------------------------------------------------------------

pub fn draw_char(x: i32, y: i32, ch: u8, fg: u32, bg: u32) {
    if ch as usize >= 128 { return; }
    begin_paint();
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
    end_paint();
}

pub fn draw_text(x: i32, y: i32, text: &str, fg: u32, bg: u32) {
    begin_paint();
    draw_text_nocursor(x, y, text, fg, bg);
    end_paint();
}

/// Draw a single ASCII byte glyph without cursor save/restore.
/// Call begin_paint()/end_paint() around bulk text for speed.
pub fn draw_byte_nocursor(x: i32, y: i32, ch: u8, fg: u32, bg: u32) {
    draw_glyph_nocursor(x, y, ch, fg, bg);
}

fn draw_text_nocursor(x: i32, y: i32, text: &str, fg: u32, bg: u32) {
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
    // NOTE:
    // Some firmware/QEMU configurations are picky about per-pixel writes.
    // Drawing glyphs using fb::fill_rect (1x1) is slightly slower but much more reliable.

    // Fast path: fully on-screen -> clear the glyph cell once, then plot only "on" pixels.
    if x >= 0 && y >= 0 {
        let ux = x as usize;
        let uy = y as usize;
        fb::fill_rect(ux, uy, font::FONT_W, font::FONT_H, bg);
        for row in 0..font::FONT_H {
            let bits = font::glyph_row(ch, row);
            for col in 0..font::FONT_W {
                let on = (bits & (1 << (7 - col))) != 0;
                if !on { continue; }
                fb::fill_rect(ux + col, uy + row, 1, 1, fg);
            }
        }
        return;
    }

    // Slow path: partially off-screen -> clip per pixel.
    for row in 0..font::FONT_H {
        let bits = font::glyph_row(ch, row);
        for col in 0..font::FONT_W {
            let px = x + col as i32;
            let py = y + row as i32;
            if px < 0 || py < 0 { continue; }
            let on = (bits & (1 << (7 - col))) != 0;
            fb::fill_rect(px as usize, py as usize, 1, 1, if on { fg } else { bg });
        }
    }
}

// ----------------------------------------------------------------------------
// Cursor (save/restore)
// ----------------------------------------------------------------------------

fn cursor_restore() {
    unsafe {
        if !CUR_DRAWN { return; }
        let ox = CUR_X;
        let oy = CUR_Y;
        for cy in 0..CUR_H as i32 {
            for cx in 0..CUR_W as i32 {
                let px = ox + cx;
                let py = oy + cy;
                if px < 0 || py < 0 || px >= SCREEN_W || py >= SCREEN_H { continue; }
                let idx = (cy as usize) * CUR_W + (cx as usize);
                fb::set_pixel(px as usize, py as usize, CUR_SAVE[idx]);
            }
        }
        CUR_DRAWN = false;
    }
}

fn cursor_redraw() {
    unsafe {
        if !CUR_VISIBLE { return; }

        // Keep the full cursor on-screen (more like Windows/Linux)
        let max_x = (SCREEN_W - CUR_W as i32).max(0);
        let max_y = (SCREEN_H - CUR_H as i32).max(0);
        CUR_X = CUR_X.clamp(0, max_x);
        CUR_Y = CUR_Y.clamp(0, max_y);

        // Save background under cursor
        for cy in 0..CUR_H as i32 {
            for cx in 0..CUR_W as i32 {
                let px = CUR_X + cx;
                let py = CUR_Y + cy;
                let idx = (cy as usize) * CUR_W + (cx as usize);
                if px < 0 || py < 0 || px >= SCREEN_W || py >= SCREEN_H {
                    CUR_SAVE[idx] = 0;
                } else {
                    CUR_SAVE[idx] = fb::get_pixel(px as usize, py as usize);
                }
            }
        }

        // Cursor bitmap draw (black outline + white fill)
        for cy in 0..CUR_H as i32 {
            for cx in 0..CUR_W as i32 {
                let px = CUR_X + cx;
                let py = CUR_Y + cy;
                if px < 0 || py < 0 || px >= SCREEN_W || py >= SCREEN_H { continue; }

                let idx = (cy as usize) * CUR_W + (cx as usize);
                let v = CUR_BITMAP[idx];
                if v == 0 { continue; }

                let col = if v == 2 { CUR_WHITE } else { CUR_BLACK };
                fb::set_pixel(px as usize, py as usize, col);
            }
        }

        CUR_DRAWN = true;
    }
}


fn cursor_move_to(x: i32, y: i32) {
    unsafe {
        // Clamp so the full cursor stays on-screen.
        let max_x = (SCREEN_W - CUR_W as i32).max(0);
        let max_y = (SCREEN_H - CUR_H as i32).max(0);
        let nx = x.clamp(0, max_x);
        let ny = y.clamp(0, max_y);

        if nx == CUR_X && ny == CUR_Y {
            // If the cursor was hidden for a repaint, redraw it at the same spot.
            if !CUR_DRAWN { cursor_redraw(); }
            return;
        }

        cursor_restore();
        CUR_X = nx;
        CUR_Y = ny;
        cursor_redraw();
    }
}
