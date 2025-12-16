#![allow(dead_code)]
// src/editor.rs
//
// A tiny “Notepad”-style editor that opens a text file from the in-kernel FS,
// lets you edit it, and saves it back.
//
// Controls:
//   - Type to insert characters
//   - Enter = newline
//   - Backspace = delete left
//   - Arrow keys = move cursor (basic)
//   - Ctrl+S = save
//   - Ctrl+Q = exit back to Terminal (handled by shell.rs)

use crate::{framebuffer_driver as fb, fs, gui};

const FG: u32 = gui::SHELL_FG_COLOR;
const BG: u32 = gui::SHELL_BG_COLOR;
const DIM: u32 = 0x94A3B8;
const ACCENT: u32 = 0x38BDF8;
const ERR: u32 = 0xF87171;
const OK: u32  = 0x34D399;

#[inline(always)]
fn draw_text_bytes(x: i32, y: i32, bytes: &[u8], fg: u32, bg: u32) {
    // Editor content is ASCII/UTF-8; if a byte is non-UTF8 this will still render best-effort
    // because the GUI font is 1-byte-per-glyph anyway.
    let s = unsafe { core::str::from_utf8_unchecked(bytes) };
    gui::draw_text(x, y, s, fg, bg);
}


const PAD: i32 = 10;
const CH_W: i32 = 8;
const CH_H: i32 = 16;

const TOOLBAR_H: i32 = 28;
const STATUS_H: i32 = 22;

const PATH_MAX: usize = 128;
const MAX_BYTES: usize = 16 * 1024;

static mut OPEN: bool = false;
static mut DIRTY: bool = false;
static mut NEED_FRAME: bool = true;


static mut PATH: [u8; PATH_MAX] = [0; PATH_MAX];
static mut PATH_LEN: usize = 0;

static mut BUF: [u8; MAX_BYTES] = [0; MAX_BYTES];
static mut LEN: usize = 0;
static mut CUR: usize = 0;

static mut SCROLL_LINE: usize = 0;

// 0 = none, 1 = saved, 2 = save failed
static mut STATUS: u8 = 0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    None,
    Redraw,
    Save,
    Exit,
}

pub fn is_open() -> bool {
    unsafe { OPEN }
}

pub fn open_abs(abs_path: &str) {
    unsafe {
        // store path
        PATH_LEN = 0;
        let pb = abs_path.as_bytes();
        let n = pb.len().min(PATH_MAX);
        PATH[..n].copy_from_slice(&pb[..n]);
        PATH_LEN = n;

        // load file
        LEN = 0;
        CUR = 0;
        SCROLL_LINE = 0;
        DIRTY = false;
        OPEN = true;
        NEED_FRAME = true;

        let mut ok = false;
        if let Ok(data) = fs::GlobalFs.lock().read_all(abs_path) {
            let take = data.len().min(MAX_BYTES);
            BUF[..take].copy_from_slice(&data[..take]);
            LEN = take;
            ok = true;
        }
        if !ok {
            // If file doesn't exist, create empty.
            LEN = 0;
            let _ = fs::GlobalFs.lock().touch(abs_path);
        }
    }
}

pub fn set_status_saved(ok: bool) { unsafe { STATUS = if ok {1} else {2}; } }

pub fn save() -> bool {
    unsafe {
        if !OPEN { return false; }
        let path = core::str::from_utf8_unchecked(&PATH[..PATH_LEN]);
        let data = &BUF[..LEN];
        match fs::GlobalFs.lock().write_all(path, data) {
            Ok(_) => { DIRTY = false; true }
            Err(_) => false,
        }
    }
}

pub fn close() {
    unsafe { OPEN = false; NEED_FRAME = true; }
}

fn line_start_for(target_line: usize) -> usize {
    unsafe {
        if target_line == 0 { return 0; }
        let mut line = 0usize;
        let mut i = 0usize;
        while i < LEN {
            if BUF[i] == b'\n' {
                line += 1;
                if line == target_line {
                    return (i + 1).min(LEN);
                }
            }
            i += 1;
        }
        LEN
    }
}

fn cursor_line_col() -> (usize, usize) {
    unsafe {
        let upto = CUR.min(LEN);
        let mut line = 0usize;
        let mut col = 0usize;
        for i in 0..upto {
            if BUF[i] == b'\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        (line, col)
    }
}

fn index_from_line_col(line: usize, col: usize) -> usize {
    unsafe {
        let mut idx = line_start_for(line);
        let mut c = 0usize;
        while idx < LEN && BUF[idx] != b'\n' {
            if c == col { return idx; }
            c += 1;
            idx += 1;
        }
        idx // end-of-line (or EOF)
    }
}

fn clamp_scroll_to_cursor(visible_lines: usize) {
    unsafe {
        let (cl, _) = cursor_line_col();
        if cl < SCROLL_LINE {
            SCROLL_LINE = cl;
        } else if cl >= SCROLL_LINE + visible_lines {
            SCROLL_LINE = cl.saturating_sub(visible_lines.saturating_sub(1));
        }
    }
}

fn insert_byte(b: u8) {
    unsafe {
        if LEN >= MAX_BYTES { return; }
        let pos = CUR.min(LEN);

        // shift right
        let mut i = LEN;
        while i > pos {
            BUF[i] = BUF[i - 1];
            i -= 1;
        }
        BUF[pos] = b;
        LEN += 1;
        CUR = pos + 1;
        DIRTY = true;
    }
}

fn backspace() {
    unsafe {
        if CUR == 0 || LEN == 0 { return; }
        let pos = CUR - 1;

        let mut i = pos;
        while i + 1 < LEN {
            BUF[i] = BUF[i + 1];
            i += 1;
        }
        LEN -= 1;
        CUR -= 1;
        DIRTY = true;
    }
}

fn move_left() {
    unsafe {
        if CUR > 0 { CUR -= 1; }
    }
}
fn move_right() {
    unsafe {
        if CUR < LEN { CUR += 1; }
    }
}
fn move_up() {
    unsafe {
        let (line, col) = cursor_line_col();
        if line == 0 { return; }
        let nl = line - 1;
        CUR = index_from_line_col(nl, col);
    }
}
fn move_down() {
    unsafe {
        let (line, col) = cursor_line_col();
        // crude "has next line": see if any newline after current position
        let mut i = CUR.min(LEN);
        while i < LEN && BUF[i] != b'\n' { i += 1; }
        if i >= LEN { return; } // no newline => last line
        // next line exists
        CUR = index_from_line_col(line + 1, col);
    }
}

/// Handle extended scancodes (arrows). Returns action.
pub fn handle_ext_scancode(sc: u8, ctrl: bool) -> EditorAction {
    // Ctrl combos don't come here (they are ASCII), but allow Ctrl+Q on scancodes if wanted.
    let _ = ctrl;
    match sc {
        0x4B => { move_left();  EditorAction::Redraw } // left
        0x4D => { move_right(); EditorAction::Redraw } // right
        0x48 => { move_up();    EditorAction::Redraw } // up
        0x50 => { move_down();  EditorAction::Redraw } // down
        _ => EditorAction::None,
    }
}

/// Handle printable ASCII/newline/backspace. Returns action.
pub fn handle_char(ch: u8, ctrl: bool) -> EditorAction {
    // Save/Exit shortcuts:
    // - Ctrl+S / Ctrl+Q (shell passes ctrl=true with printable 's'/'q')
    // - Or control codes 0x13/0x11 if they come through as ASCII.
    if ctrl {
        match ch {
            b's' | b'S' => return EditorAction::Save,
            b'q' | b'Q' => return EditorAction::Exit,
            _ => return EditorAction::None,
        }
    }
    match ch {
        0x13 => { return EditorAction::Save; }
        0x11 => { return EditorAction::Exit; }

        b'\n' => { insert_byte(b'\n'); EditorAction::Redraw }
        0x08 => { backspace(); EditorAction::Redraw }
        b'\t' => { insert_byte(b' '); insert_byte(b' '); insert_byte(b' '); insert_byte(b' '); EditorAction::Redraw }
        _ => {
            if ch >= 0x20 && ch <= 0x7E {
                insert_byte(ch);
                EditorAction::Redraw
            } else {
                EditorAction::None
            }
        }
    }
}

pub fn render() {
    if !gui::shell_is_visible() { return; }

    unsafe {
        if NEED_FRAME {
            gui::clear_shell_content_and_frame();
            NEED_FRAME = false;
        }
    }

    gui::begin_paint();

    let x0 = gui::shell_content_left();
    let y0 = gui::shell_content_top();
    let w  = gui::shell_content_w();
    let h  = gui::shell_content_h();

    // background
    fb::fill_rect(x0 as usize, y0 as usize, w as usize, h as usize, BG);

    // toolbar
    fb::fill_rect(x0 as usize, y0 as usize, w as usize, TOOLBAR_H as usize, 0x111827);
    let mut title = [0u8; 192];
    let mut tn = 0usize;
    unsafe {
        title[tn..tn+5].copy_from_slice(b"Edit ");
        tn += 5;
        let p = &PATH[..PATH_LEN];
        let take = p.len().min(title.len().saturating_sub(tn));
        title[tn..tn+take].copy_from_slice(&p[..take]);
        tn += take;
        if DIRTY && tn + 2 < title.len() {
            title[tn] = b' '; title[tn+1] = b'*'; tn += 2;
        }
    }
    draw_text_bytes(x0 + PAD, y0 + 7, &title[..tn], FG, BG);

    // layout for text area
    let text_y = y0 + TOOLBAR_H + 6;
    let text_h = (h - TOOLBAR_H - STATUS_H - 12).max(0);
    let visible_lines = (text_h / CH_H).max(1) as usize;

    // clamp scroll to keep cursor visible
    clamp_scroll_to_cursor(visible_lines);

    // draw lines
    let mut line = unsafe { SCROLL_LINE };
    let mut y = text_y;
    for _ in 0..visible_lines {
        if y + CH_H > y0 + h - STATUS_H { break; }
        let start = line_start_for(line);
        unsafe {
            if start >= LEN && line != 0 {
                break;
            }
            // gather bytes until newline or max cols
            let max_cols = ((w - PAD*2) / CH_W).max(1) as usize;
            let mut out = [0u8; 256];
            let mut n = 0usize;
            let mut i = start;
            while i < LEN && BUF[i] != b'\n' && n < out.len() && n < max_cols {
                let b = BUF[i];
                out[n] = if b == b'\r' { b' ' } else { b };
                n += 1;
                i += 1;
            }
            if n == 0 {
                // draw faint placeholder for empty line
                gui::draw_text(x0 + PAD, y, "", FG, BG);
            } else {
                draw_text_bytes(x0 + PAD, y, &out[..n], FG, BG);
            }
        }

        y += CH_H;
        line += 1;
    }

    // cursor
    unsafe {
        if OPEN {
            let (cl, cc) = cursor_line_col();
            if cl >= SCROLL_LINE && cl < SCROLL_LINE + visible_lines {
                let cx = x0 + PAD + (cc as i32) * CH_W;
                let cy = text_y + ((cl - SCROLL_LINE) as i32) * CH_H;

                // background block for caret
                fb::fill_rect(cx.max(x0 + PAD) as usize, cy.max(text_y) as usize, 2usize, CH_H as usize, ACCENT);

                // draw char under cursor with highlight (optional)
                let b = if CUR < LEN { BUF[CUR] } else { b' ' };
                if b != b'\n' {
                    fb::fill_rect((cx+2) as usize, cy as usize, CH_W as usize, CH_H as usize, 0x0B1220);
                    gui::draw_char(cx+2, cy, b, FG, BG);
                }
            }
        }
    }

    // status bar
    let sb_y = y0 + h - STATUS_H;
    fb::fill_rect(x0 as usize, sb_y as usize, w as usize, STATUS_H as usize, 0x0B1220);

    // left status text
    gui::draw_text(x0 + PAD, sb_y + 4, "Ctrl+S Save   Ctrl+Q Exit", DIM, BG);

    // save status
    unsafe {
        if STATUS == 1 {
            gui::draw_text(x0 + PAD + 200, sb_y + 4, "Saved", OK, BG);
        } else if STATUS == 2 {
            gui::draw_text(x0 + PAD + 200, sb_y + 4, "Save failed", ERR, BG);
        }
    }

    // right status: Ln/Col + saved/err
    let (ln, col) = cursor_line_col();
    let mut st = [0u8; 64];
    let mut n = 0usize;
    st[n..n+3].copy_from_slice(b"Ln "); n += 3;
    n += write_u32_dec(&mut st[n..], (ln + 1) as u32);
    st[n..n+5].copy_from_slice(b"  Col"); n += 5;
    st[n..n+1].copy_from_slice(b" "); n += 1;
    n += write_u32_dec(&mut st[n..], (col + 1) as u32);

    let tw = (n as i32) * CH_W;
    let rx = (x0 + w - PAD - tw).max(x0 + PAD);
    draw_text_bytes(rx, sb_y + 4, &st[..n], FG, BG);

    gui::end_paint();
}

fn write_u32_dec(out: &mut [u8], mut v: u32) -> usize {
    let mut tmp = [0u8; 10];
    let mut n = 0usize;
    if v == 0 {
        if !out.is_empty() { out[0] = b'0'; return 1; }
        return 0;
    }
    while v > 0 && n < tmp.len() {
        tmp[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
    }
    // reverse
    let mut i = 0usize;
    while i < n && i < out.len() {
        out[i] = tmp[n - 1 - i];
        i += 1;
    }
    i
}