#![allow(dead_code)]

//! Read-only Registry viewer.
//!
//! This is a tiny UI rendered inside the shell window content area.
//! Navigation:
//!   - Up/Down: select user
//!   - Enter:   view user values
//!   - Backspace: go back
//!
//! No editing (read-only), per user request.

use crate::{gui, registry};

const FG: u32 = gui::SHELL_FG_COLOR;
const BG: u32 = gui::SHELL_BG_COLOR;
const DIM: u32 = 0x94A3B8;
const HDR: u32 = 0xE5E7EB;
const ACC: u32 = 0x38BDF8;

const CH_W: i32 = 8;
const CH_H: i32 = 16;

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Users,
    UserDetail,
}

static mut VIEW: View = View::Users;
static mut SEL: i32 = 0;
static mut DETAIL_NTH: usize = 0;

pub fn reset() {
    unsafe {
        VIEW = View::Users;
        SEL = 0;
        DETAIL_NTH = 0;
    }
}

pub fn handle_ext_scancode(sc: u8) -> bool {
    // Return true if UI changed and needs re-render.
    unsafe {
        match VIEW {
            View::Users => {
                let count = registry::user_count() as i32;
                if count <= 0 { return false; }
                match sc {
                    0x48 => { // Up
                        SEL = (SEL - 1).clamp(0, count - 1);
                        true
                    }
                    0x50 => { // Down
                        SEL = (SEL + 1).clamp(0, count - 1);
                        true
                    }
                    _ => false,
                }
            }
            View::UserDetail => false,
        }
    }
}

pub fn handle_ascii(ch: u8) -> bool {
    unsafe {
        match VIEW {
            View::Users => {
                match ch {
                    b'\n' => {
                        let idx = SEL.max(0) as usize;
                        if registry::user_entry_by_index(idx).is_some() {
                            VIEW = View::UserDetail;
                            DETAIL_NTH = idx;
                            true
                        } else { false }
                    }
                    0x08 => { // backspace
                        // already at root
                        false
                    }
                    _ => false,
                }
            }
            View::UserDetail => {
                match ch {
                    0x08 => { // back
                        VIEW = View::Users;
                        true
                    }
                    _ => false,
                }
            }
        }
    }
}

fn draw_str(x: i32, y: i32, s: &[u8], fg: u32, bg: u32) {
    let mut cx = x;
    for &b in s {
        if b == b'\n' { break; }
        gui::draw_byte_nocursor(cx, y, b, fg, bg);
        cx += CH_W;
    }
}

fn draw_u64(x: i32, y: i32, mut v: u64, fg: u32, bg: u32) {
    let mut tmp = [0u8; 32];
    let mut n = 0usize;
    if v == 0 {
        tmp[n] = b'0';
        n += 1;
    } else {
        let mut rev = [0u8; 24];
        let mut rn = 0usize;
        while v > 0 && rn < rev.len() {
            rev[rn] = b'0' + (v % 10) as u8;
            v /= 10;
            rn += 1;
        }
        while rn > 0 {
            rn -= 1;
            tmp[n] = rev[rn];
            n += 1;
        }
    }
    draw_str(x, y, &tmp[..n], fg, bg);
}

pub fn render() {
    if !gui::shell_is_visible() { return; }
    gui::clear_shell_content_and_frame();
    gui::begin_paint();

    let x0 = gui::shell_content_left();
    let y0 = gui::shell_content_top();
    //let w  = gui::shell_content_w();

    let x = x0 + 12;
    let mut y = y0 + 10;

    draw_str(x, y, b"Registry Editor (Read Only)", HDR, BG);
    y += CH_H + 6;
    draw_str(x, y, b"HKLM\\SOFTWARE\\Othello\\Users", DIM, BG);
    y += CH_H + 10;

    unsafe {
        match VIEW {
            View::Users => {
                let count = registry::user_count();
                if count == 0 {
                    draw_str(x, y, b"(no users)", DIM, BG);
                } else {
                    for i in 0..count {
                        let yy = y + (i as i32) * (CH_H + 4);
                        let sel = (i as i32) == SEL;
                        let col = if sel { ACC } else { FG };

                        // "- <username>"
                        gui::draw_byte_nocursor(x, yy, b'-', col, BG);
                        gui::draw_byte_nocursor(x + CH_W, yy, b' ', col, BG);
                        if let Some(u) = registry::user_entry_by_index(i) {
                            let n = u.name_len as usize;
                            draw_str(x + CH_W * 2, yy, &u.name[..n], col, BG);
                        }
                    }
                    let hint_y = y0 + gui::shell_content_h() - 2 * CH_H - 6;
                    if hint_y > y {
                        draw_str(x, hint_y, b"Up/Down select, Enter view, Backspace back", DIM, BG);
                    }
                }
            }
            View::UserDetail => {
                let Some(u) = registry::user_entry_by_index(DETAIL_NTH) else {
                    draw_str(x, y, b"(missing user)", DIM, BG);
                    gui::end_paint();
                    return;
                };

                // Header: username
                draw_str(x, y, b"User:", DIM, BG);
                draw_str(x + CH_W * 6, y, &u.name[..u.name_len as usize], ACC, BG);
                y += CH_H + 8;

                draw_str(x, y, b"Salt:", DIM, BG);
                draw_u64(x + CH_W * 8, y, u.salt as u64, FG, BG);
                y += CH_H + 4;
                draw_str(x, y, b"Hash:", DIM, BG);
                draw_u64(x + CH_W * 8, y, u.hash, FG, BG);
                y += CH_H + 4;
                draw_str(x, y, b"CreatedTsc:", DIM, BG);
                draw_u64(x + CH_W * 12, y, u.created_tsc, FG, BG);

                let hint_y = y0 + gui::shell_content_h() - CH_H - 6;
                draw_str(x, hint_y, b"Backspace to return", DIM, BG);
            }
        }
    }

    // Footer hint
    let fx = gui::shell_footer_x();
    let fy = gui::shell_footer_y();
    draw_str(fx + 8, fy + 1, b"Dock: [R] Registry  [L] Lock/Login  [T] Terminal", DIM, BG);

    gui::end_paint();
}
