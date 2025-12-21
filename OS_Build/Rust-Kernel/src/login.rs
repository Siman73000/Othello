#![allow(dead_code)]

//! Boot-time login screen with user creation.
//!
//! - Allocation free
//! - Passwords are stored as salted hashes in the in-memory registry
//! - Rendered full-screen (Windows-11-ish) when gui::UiMode::Login is active

use crate::{gui, registry, time};
use crate::framebuffer_driver as fb;

const FG: u32 = 0xE5E7EB;
const BG: u32 = 0x0B1220;
const DIM: u32 = 0x94A3B8;
const HDR: u32 = 0xE5E7EB;
const OK: u32  = 0x34D399;
const ERR: u32 = 0xF87171;
const ACC: u32 = 0x38BDF8;

const CH_W: i32 = 8;
const CH_H: i32 = 16;

// ----------------------------------------------------------------------------
// UI strings stored in `.data`
//
// Reason: if the kernel build/loader forgets to load `.rodata`, normal
// string literals (and other immutable `static` data) may appear as all-zero,
// which makes UI text vanish. Putting these in `.data` makes the login UI
// resilient without touching your boot pipeline.
// ----------------------------------------------------------------------------

#[link_section = ".data"]
static STR_TITLE: [u8; 10] = *b"Othello OS";
#[link_section = ".data"]
static STR_SUB_LOGIN: [u8; 7] = *b"Sign in";
#[link_section = ".data"]
static STR_SUB_CREATE: [u8; 14] = *b"Create account";

#[link_section = ".data"]
static STR_TAB_LOGIN: [u8; 9] = *b"[L] Login";
#[link_section = ".data"]
static STR_TAB_CREATE: [u8; 10] = *b"[C] Create";

#[link_section = ".data"]
static STR_USERNAME: [u8; 8] = *b"Username";
#[link_section = ".data"]
static STR_PASSWORD: [u8; 8] = *b"Password";
#[link_section = ".data"]
static STR_CONFIRM: [u8; 16] = *b"Confirm password";

#[link_section = ".data"]
static STR_HELP: [u8; 43] = *b"Tab: next  Enter: submit  Backspace: delete";

#[link_section = ".data"]
static MSG_HINT_LOGIN_OR_CREATE: [u8; 57] = *b"Enter username/password. Press C to create a new account.";
#[link_section = ".data"]
static MSG_HINT_FIRST_USER: [u8; 41] = *b"No users found. Create the first account.";
#[link_section = ".data"]
static MSG_MODE_LOGIN: [u8; 46] = *b"Login: type username/password and press Enter.";
#[link_section = ".data"]
static MSG_MODE_CREATE: [u8; 57] = *b"Create: choose username + password + confirm, then Enter.";

#[link_section = ".data"]
static MSG_ERR_MISSING: [u8; 28] = *b"Missing username or password";
#[link_section = ".data"]
static MSG_OK_LOGIN: [u8; 16] = *b"Login successful";
#[link_section = ".data"]
static MSG_ERR_INVALID: [u8; 28] = *b"Invalid username or password";
#[link_section = ".data"]
static MSG_ERR_MATCH: [u8; 22] = *b"Passwords do not match";
#[link_section = ".data"]
static MSG_OK_CREATED: [u8; 27] = *b"Account created & logged in";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode { Login, Create }

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field { Username, Password, Confirm }

static mut MODE: Mode = Mode::Login;
static mut FIELD: Field = Field::Username;

static mut USER: [u8; registry::MAX_USERNAME] = [0; registry::MAX_USERNAME];
static mut USER_LEN: usize = 0;
static mut PASS: [u8; registry::MAX_PASSWORD] = [0; registry::MAX_PASSWORD];
static mut PASS_LEN: usize = 0;
static mut CONF: [u8; registry::MAX_PASSWORD] = [0; registry::MAX_PASSWORD];
static mut CONF_LEN: usize = 0;

static mut STATUS: [u8; 96] = [0; 96];
static mut STATUS_LEN: usize = 0;
static mut STATUS_COLOR: u32 = DIM;

static mut LOGGED_IN: bool = false;
static mut ACTIVE_USER: [u8; registry::MAX_USERNAME] = [0; registry::MAX_USERNAME];
static mut ACTIVE_USER_LEN: usize = 0;

pub fn is_logged_in() -> bool { unsafe { LOGGED_IN } }

pub fn current_user_bytes() -> &'static [u8] {
    unsafe { &ACTIVE_USER[..ACTIVE_USER_LEN] }
}

pub fn lock() {
    unsafe {
        LOGGED_IN = false;
        ACTIVE_USER_LEN = 0;
    }
    reset();
}

pub fn reset() {
    unsafe {
        USER_LEN = 0;
        PASS_LEN = 0;
        CONF_LEN = 0;
        FIELD = Field::Username;
        MODE = if registry::has_users() { Mode::Login } else { Mode::Create };
        set_status(
            if registry::has_users() { &MSG_HINT_LOGIN_OR_CREATE[..] } else { &MSG_HINT_FIRST_USER[..] },
            DIM,
        );
    }
}

fn set_status(msg: &[u8], color: u32) {
    unsafe {
        let n = msg.len().min(STATUS.len());
        STATUS[..n].copy_from_slice(&msg[..n]);
        STATUS_LEN = n;
        STATUS_COLOR = color;
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

fn draw_field_value(x: i32, y: i32, buf: &[u8], len: usize, masked: bool, fg: u32, bg: u32) {
    let mut cx = x;
    // Defensive clamp: prevents kernel panic if len is corrupted or exceeds buffer capacity.
    let safe_len = core::cmp::min(len, buf.len());
    for i in 0..safe_len {
        let b = if masked { b'*' } else { buf[i] };
        gui::draw_byte_nocursor(cx, y, b, fg, bg);
        cx += CH_W;
    }
    // caret underscore
    gui::draw_byte_nocursor(cx, y, b'_', fg, bg);
}


fn active_buf_mut() -> (&'static mut [u8], &'static mut usize, bool) {
    unsafe {
        match FIELD {
            Field::Username => (&mut USER[..], &mut USER_LEN, false),
            Field::Password => (&mut PASS[..], &mut PASS_LEN, true),
            Field::Confirm  => (&mut CONF[..], &mut CONF_LEN, true),
        }
    }
}

fn cycle_field() {
    unsafe {
        FIELD = match (MODE, FIELD) {
            (_, Field::Username) => Field::Password,
            (Mode::Login, Field::Password) => Field::Username,
            (Mode::Create, Field::Password) => Field::Confirm,
            (Mode::Create, Field::Confirm) => Field::Username,
            _ => Field::Username,
        };
    }
}

fn set_mode(m: Mode) {
    unsafe {
        MODE = m;
        FIELD = Field::Username;
        PASS_LEN = 0;
        CONF_LEN = 0;
    }
    set_status(match m {
        Mode::Login => &MSG_MODE_LOGIN[..],
        Mode::Create => &MSG_MODE_CREATE[..],
    }, DIM);
}

pub fn handle_ext_scancode(sc: u8) -> bool {
    // No mouse-driven fields yet; we keep login purely keyboard.
    // Return true if re-render needed.
    unsafe {
        match sc {
            0x4B => { // Left
                // treat as cycle backward
                FIELD = match (MODE, FIELD) {
                    (_, Field::Password) => Field::Username,
                    (Mode::Create, Field::Confirm) => Field::Password,
                    (Mode::Login, Field::Username) => Field::Password,
                    (Mode::Create, Field::Username) => Field::Confirm,
                    _ => Field::Username,
                };
                true
            }
            0x4D => { // Right
                cycle_field();
                true
            }
            _ => false,
        }
    }
}

pub enum LoginOutcome {
    None,
    Success,
}

pub fn handle_ascii(ch: u8) -> (bool, LoginOutcome) {
    // Returns (needs_redraw, outcome)

    // quick hotkeys (work regardless of focus)
    match ch {
        b'c' | b'C' => { set_mode(Mode::Create); return (true, LoginOutcome::None); }
        b'l' | b'L' => {
            if registry::has_users() {
                set_mode(Mode::Login);
                return (true, LoginOutcome::None);
            }
        }
        _ => {}
    }

    match ch {
        b'\t' => {
            cycle_field();
            return (true, LoginOutcome::None);
        }
        0x08 => {
            // backspace
            let (_buf, len, _masked) = active_buf_mut();
            unsafe {
                if *len > 0 { *len -= 1; }
            }
            return (true, LoginOutcome::None);
        }
        b'\n' => {
            // submit
            if try_submit() {
                return (true, LoginOutcome::Success);
            }
            return (true, LoginOutcome::None);
        }
        _ => {}
    }

    // input chars
    if ch >= 0x20 && ch <= 0x7E {
        let (buf, len, _masked) = active_buf_mut();
        unsafe {
            if *len < buf.len() {
                buf[*len] = ch;
                *len += 1;
                return (true, LoginOutcome::None);
            }
        }
    }

    (false, LoginOutcome::None)
}

fn try_submit() -> bool {
    unsafe {
        let ulen = core::cmp::min(USER_LEN, USER.len());
        let username = core::str::from_utf8_unchecked(&USER[..ulen]);
        let plen = core::cmp::min(PASS_LEN, PASS.len());
        let password = core::str::from_utf8_unchecked(&PASS[..plen]);
        let clen = core::cmp::min(CONF_LEN, CONF.len());
        let confirm  = core::str::from_utf8_unchecked(&CONF[..clen]);

        match MODE {
            Mode::Login => {
                if USER_LEN == 0 || PASS_LEN == 0 {
                    set_status(&MSG_ERR_MISSING[..], ERR);
                    return false;
                }
                if registry::validate_login(username, password) {
                    LOGGED_IN = true;
                    ACTIVE_USER[..ulen].copy_from_slice(&USER[..ulen]);
                    ACTIVE_USER_LEN = ulen;
                    set_status(&MSG_OK_LOGIN[..], OK);
                    return true;
                }
                set_status(&MSG_ERR_INVALID[..], ERR);
                false
            }
            Mode::Create => {
                if USER_LEN == 0 || PASS_LEN == 0 {
                    set_status(&MSG_ERR_MISSING[..], ERR);
                    return false;
                }
                if plen != clen || &PASS[..plen] != &CONF[..clen] {
                    set_status(&MSG_ERR_MATCH[..], ERR);
                    return false;
                }
                match registry::create_user(username, password) {
                    Ok(()) => {
                        LOGGED_IN = true;
                        ACTIVE_USER[..ulen].copy_from_slice(&USER[..ulen]);
                        ACTIVE_USER_LEN = ulen;
                        set_status(&MSG_OK_CREATED[..], OK);
                        true
                    }
                    Err(e) => {
                        // e is &'static str
                        set_status(e.as_bytes(), ERR);
                        false
                    }
                }
            }
        }
    }
}

pub fn render() {
    // If we're in login UI mode, use the full-screen renderer.
    if gui::ui_mode() == gui::UiMode::Login {
        render_fullscreen();
        return;
    }
    // Fallback: if someone calls login::render() while in desktop mode,
    // render a small centered panel (still full-screen, not in-shell) so
    // we don't depend on the shell window.
    render_fullscreen();
}

pub fn render_fullscreen() {
    // Background is drawn by gui::redraw_all() (mode-dependent)
    gui::redraw_all();
    gui::begin_paint();

    let sw = gui::screen_w().max(0);
    let sh = gui::screen_h().max(0);
    if sw <= 0 || sh <= 0 { gui::end_paint(); return; }

    // Panel width: clamp to the screen with margins (works down to ~360px wide).
    let max_w = (sw - 48).max(320);
    let panel_w = (max_w.min(560)).max(360);
    // Reserve space for a slightly larger logo banner above the fields (drawn above the panel)
    let logo_h: i32 = 96;
    let mut panel_h = if unsafe { MODE } == Mode::Create { 320 } else { 280 };
    // Ensure it fits on small screens.
    panel_h = panel_h.min((sh - 32).max(220));
    let px = (sw - panel_w) / 2;
    let py = (sh - panel_h) / 2;

    // Logo (above the panel)
    const OTHELLO_SRC_W: usize = 739;
    const OTHELLO_SRC_H: usize = 739;
    let max_logo_w = (panel_w - 96).max(64);
    let max_logo_h = logo_h;

    // Compute destination size preserving source aspect ratio.
    let mut dst_w = max_logo_w;
    let mut dst_h = (dst_w as i32 * OTHELLO_SRC_H as i32) / OTHELLO_SRC_W as i32;
    if dst_h > max_logo_h {
        dst_h = max_logo_h;
        dst_w = (dst_h as i32 * OTHELLO_SRC_W as i32) / OTHELLO_SRC_H as i32;
    }
    if dst_w < 1 { dst_w = 1; }
    if dst_h < 1 { dst_h = 1; }

    let logo_w = dst_w as i32;
    let logo_x = px + (panel_w - logo_w) / 2;
    // place the logo above the panel with a small gap; clamp to screen
    let mut logo_y = py - logo_h - 12;
    if logo_y < 12 { logo_y = 12; }
    let rgba: &'static [u8] = include_bytes!("../wallpapers/Othello.rgba");
    if logo_w > 0 {
        let dst_w_u = logo_w as usize;
        let dst_h_u = logo_h as usize;
        for yy in 0..dst_h_u {
            for xx in 0..dst_w_u {
                let sx = (xx.saturating_mul(OTHELLO_SRC_W) / dst_w_u).min(OTHELLO_SRC_W - 1);
                let sy = (yy.saturating_mul(OTHELLO_SRC_H) / dst_h_u).min(OTHELLO_SRC_H - 1);
                let i = (sy.saturating_mul(OTHELLO_SRC_W).saturating_add(sx)).saturating_mul(4);
                let sr = rgba.get(i + 0).copied().unwrap_or(0) as u32;
                let sg = rgba.get(i + 1).copied().unwrap_or(0) as u32;
                let sb = rgba.get(i + 2).copied().unwrap_or(0) as u32;
                let sa = rgba.get(i + 3).copied().unwrap_or(0) as u32; // 0..255

                // If fully opaque, fast path
                let dst_x = (logo_x + xx as i32) as usize;
                let dst_y = (logo_y + yy as i32) as usize;
                if sa >= 255 {
                    let color = (sr << 16) | (sg << 8) | sb;
                    fb::set_pixel(dst_x, dst_y, color);
                } else if sa == 0 {
                    // fully transparent: skip
                } else {
                    // alpha blend: out = sa*src + (255-sa)*dst
                    let dstc = fb::get_pixel(dst_x, dst_y);
                    let dr = ((dstc >> 16) & 0xFF) as u32;
                    let dg = ((dstc >> 8) & 0xFF) as u32;
                    let db = (dstc & 0xFF) as u32;
                    let inv = 255u32 - sa;
                    let nr = (sa * sr + inv * dr) / 255u32;
                    let ng = (sa * sg + inv * dg) / 255u32;
                    let nb = (sa * sb + inv * db) / 255u32;
                    let color = (nr << 16) | (ng << 8) | nb;
                    fb::set_pixel(dst_x, dst_y, color);
                }
            }
        }
    }

    // Shadow + panel
    gui::fill_round_rect_nocursor(px + 8, py + 10, panel_w, panel_h, 22, 0x000000);
    gui::fill_round_rect_nocursor(px, py, panel_w, panel_h, 22, BG);
    gui::fill_round_rect_nocursor(px, py, panel_w, 56, 22, 0x0B1220);

    // Header text
    draw_str(px + 24, py + 18, &STR_TITLE[..], HDR, BG);
    unsafe {
        let sub: &[u8] = match MODE {
            Mode::Login => &STR_SUB_LOGIN[..],
            Mode::Create => &STR_SUB_CREATE[..],
        };
        draw_str(px + 24, py + 18 + CH_H, sub, DIM, BG);

    // Current date/time (from CMOS RTC) in header (top-right of card)
    {
        let dt = time::rtc_now();
        let mut tbuf = [0u8; 32];
        let n = time::format_datetime(&mut tbuf, dt);
        let tw = (n as i32) * CH_W;
        let x = (px + panel_w - 24 - tw).max(px + 24);
        draw_str(x, py + 18, &tbuf[..n], DIM, BG);
    }

    }

    // Tabs (Windows-ish)
    unsafe {
        let (m_login, m_create) = match MODE { Mode::Login => (ACC, DIM), Mode::Create => (DIM, ACC) };
        let ty = py + 64;
        draw_str(px + 24, ty, &STR_TAB_LOGIN[..], m_login, BG);
        draw_str(px + 24 + CH_W * 12, ty, &STR_TAB_CREATE[..], m_create, BG);
    }

    // Fields
    let mut y = py + 64 + CH_H + 14;
    let fx = px + 24;
    let fv = px + 24 + CH_W * 12;
    let field_w = panel_w - 48;

    unsafe {
        // Username
        let ucol = if FIELD == Field::Username { ACC } else { DIM };
        draw_str(fx, y, &STR_USERNAME[..], ucol, BG);
        y += CH_H + 4;
        draw_field_value(fx, y, &USER, USER_LEN, false, FG, BG);
        // underline
        crate::framebuffer_driver::fill_rect(fx as usize, (y + CH_H + 2) as usize, field_w as usize, 1, if FIELD == Field::Username { ACC } else { 0x334155 });
        y += CH_H + 14;

        // Password
        let pcol = if FIELD == Field::Password { ACC } else { DIM };
        draw_str(fx, y, &STR_PASSWORD[..], pcol, BG);
        y += CH_H + 4;
        draw_field_value(fx, y, &PASS, PASS_LEN, true, FG, BG);
        crate::framebuffer_driver::fill_rect(fx as usize, (y + CH_H + 2) as usize, field_w as usize, 1, if FIELD == Field::Password { ACC } else { 0x334155 });
        y += CH_H + 14;

        if MODE == Mode::Create {
            let ccol = if FIELD == Field::Confirm { ACC } else { DIM };
            draw_str(fx, y, &STR_CONFIRM[..], ccol, BG);
            y += CH_H + 4;
            draw_field_value(fx, y, &CONF, CONF_LEN, true, FG, BG);
            crate::framebuffer_driver::fill_rect(fx as usize, (y + CH_H + 2) as usize, field_w as usize, 1, if FIELD == Field::Confirm { ACC } else { 0x334155 });
            y += CH_H + 14;
        }
    }

    // Status line
    unsafe {
        let sy = py + panel_h - 44;
        draw_str(px + 24, sy, &STATUS[..STATUS_LEN], STATUS_COLOR, BG);
    }

    // Help footer
    let hy = py + panel_h - 24;
    draw_str(px + 24, hy, &STR_HELP[..], DIM, BG);
    gui::end_paint();
}