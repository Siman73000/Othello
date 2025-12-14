#![allow(dead_code)]

use crate::keyboard::{keyboard_poll_scancode, scancode_to_ascii};
use crate::mouse;
use crate::gui::{self, UiAction};
use crate::font::{FONT_W, FONT_H};
use crate::framebuffer_driver;

const MAX_LINE: usize = 120;
const MAX_LINES: usize = 256;

static mut LINES: [[u8; MAX_LINE]; MAX_LINES] = [[0; MAX_LINE]; MAX_LINES];
static mut LINE_LEN: [usize; MAX_LINES] = [0; MAX_LINES];
static mut LINE_COUNT: usize = 0;
static mut VIEW_OFFSET: usize = 0;

static mut INBUF: [u8; MAX_LINE] = [0; MAX_LINE];
static mut INLEN: usize = 0;

static mut CARET_X: usize = 0;
static mut CARET_Y: usize = 0;
static mut CARET_ON: bool = false;
static mut CARET_T: u32 = 0;

const CARET_W: usize = 2;
const CARET_BLINK_TICKS: u32 = 35;

fn push_line(bytes: &[u8]) {
    unsafe {
        let idx = LINE_COUNT % MAX_LINES;
        let mut n = bytes.len().min(MAX_LINE - 1);
        while n > 0 && (bytes[n - 1] == b'\n' || bytes[n - 1] == b'\r') { n -= 1; }
        LINES[idx][..n].copy_from_slice(&bytes[..n]);
        LINE_LEN[idx] = n;
        LINE_COUNT += 1;
        VIEW_OFFSET = 0;
    }
}

fn print_line(s: &str) { push_line(s.as_bytes()); }

fn draw_bytes_line(mut x: usize, y: usize, bytes: &[u8], fg: u32, bg: u32) {
    for &b in bytes {
        if b < 0x20 { continue; }
        gui::draw_char(x, y, b as char, fg, bg);
        x += FONT_W;
    }
}

fn caret_off() {
    unsafe {
        if CARET_ON {
            gui::invert_rect(CARET_X, CARET_Y, CARET_W, FONT_H);
            CARET_ON = false;
        }
    }
}

fn caret_set(x: usize, y: usize) {
    unsafe {
        if CARET_ON {
            gui::invert_rect(CARET_X, CARET_Y, CARET_W, FONT_H);
            CARET_ON = false;
        }
        CARET_X = x;
        CARET_Y = y;
        CARET_T = 0;
    }
}

fn caret_tick() {
    unsafe {
        CARET_T = CARET_T.wrapping_add(1);
        if CARET_T >= CARET_BLINK_TICKS {
            CARET_T = 0;
            gui::invert_rect(CARET_X, CARET_Y, CARET_W, FONT_H);
            CARET_ON = !CARET_ON;
        }
    }
}

fn eq_cmd(line: &[u8], cmd: &[u8]) -> bool {
    if line.len() != cmd.len() { return false; }
    for i in 0..line.len() {
        let mut a = line[i];
        let mut b = cmd[i];
        if b'A' <= a && a <= b'Z' { a += 32; }
        if b'A' <= b && b <= b'Z' { b += 32; }
        if a != b { return false; }
    }
    true
}

fn starts_with(line: &[u8], pfx: &[u8]) -> bool {
    line.len() >= pfx.len() && &line[..pfx.len()] == pfx
}

fn handle_command(line: &[u8]) {
    let mut end = line.len();
    while end > 0 && (line[end - 1] == b' ' || line[end - 1] == b'\t') { end -= 1; }
    let line = &line[..end];
    if line.is_empty() { return; }

    if eq_cmd(line, b"help") {
        print_line("Commands:");
        print_line("  help      - show commands");
        print_line("  clear     - clear scrollback");
        print_line("  echo ...  - print args");
        print_line("  about     - info");
        print_line("  reboot    - stub");
        print_line("  panic     - trigger panic");
    } else if eq_cmd(line, b"about") {
        print_line("Othello OS shell (UI demo).");
    } else if eq_cmd(line, b"clear") {
        unsafe { LINE_COUNT = 0; VIEW_OFFSET = 0; }
    } else if starts_with(line, b"echo ") {
        push_line(&line[5..]);
    } else if eq_cmd(line, b"reboot") {
        print_line("[reboot stub] ACPI not implemented yet.");
    } else if eq_cmd(line, b"panic") {
        panic!("panic command invoked from shell");
    } else {
        print_line("Unknown command. Type 'help'.");
    }
}

fn redraw_shell_contents() {
    let r = gui::shell_content_rect();
    let x0 = (r.x + 8).max(0) as usize;
    let y0 = (r.y + 8).max(0) as usize;

    gui::cursor_hide();
    caret_off();
    gui::clear_shell_area();

    // small hint line
    gui::draw_text(x0, y0, "Type 'help' for commands", 0x9CA3AF, gui::SHELL_BG_COLOR);

    let y_start = y0 + FONT_H + 10;
    let rows_visible = ((r.h.max(0) as usize).saturating_sub(16) / FONT_H).saturating_sub(3);
    let cols_visible = (r.w.max(0) as usize).saturating_sub(16) / FONT_W;

    unsafe {
        let total = LINE_COUNT.min(MAX_LINES);
        let mut start_line = if total > rows_visible { total - rows_visible } else { 0 };
        if VIEW_OFFSET < total {
            start_line = start_line.saturating_sub(VIEW_OFFSET);
        }

        for row in 0..rows_visible {
            let li = start_line + row;
            if li >= total { break; }
            let idx = li % MAX_LINES;
            let len = LINE_LEN[idx].min(cols_visible);
            let y = y_start + row * FONT_H;
            draw_bytes_line(x0, y, &LINES[idx][..len], gui::SHELL_FG_COLOR, gui::SHELL_BG_COLOR);
        }
    }

    // prompt + input
    let prompt_y = y_start + rows_visible * FONT_H + 4;
    gui::fill_rect(x0, prompt_y, (r.w.max(0) as usize).saturating_sub(16), FONT_H + 2, gui::SHELL_BG_COLOR);

    draw_bytes_line(x0, prompt_y, b"> ", 0x93C5FD, gui::SHELL_BG_COLOR);

    let cols = cols_visible;
    let prompt_len = 2usize;
    let max_text_cols = cols.saturating_sub(prompt_len).max(1);

    unsafe {
        let (start, vis_len) = if INLEN <= max_text_cols { (0usize, INLEN) } else { (INLEN - max_text_cols, max_text_cols) };
        let vis = &INBUF[start..start + vis_len];
        draw_bytes_line(x0 + prompt_len * FONT_W, prompt_y, vis, gui::SHELL_FG_COLOR, gui::SHELL_BG_COLOR);

        // caret at *next* cell so it doesn't invert last glyph
        let caret_col = prompt_len + vis_len;
        caret_set(x0 + caret_col * FONT_W, prompt_y);
    }

    gui::cursor_show();
    gui::cursor_draw();
}

pub fn run_shell() -> ! {
    print_line("Othello shell ready. Type 'help'.");
    redraw_shell_contents();

    let mut shift = false;

    loop {
        // Mouse: poll, drag, wheel scrollback
        if let Some(ms) = mouse::mouse_poll(framebuffer_driver::logical_width() as i32,
                                            framebuffer_driver::logical_height() as i32) {
            caret_off();
            let action = gui::ui_handle_mouse(ms);

            if ms.wheel != 0 {
                unsafe {
                    let total = LINE_COUNT.min(MAX_LINES);
                    if total > 0 {
                        if ms.wheel > 0 { VIEW_OFFSET = (VIEW_OFFSET + 3).min(total.saturating_sub(1)); }
                        else { VIEW_OFFSET = VIEW_OFFSET.saturating_sub(3); }
                        redraw_shell_contents();
                    }
                }
            } else if action == UiAction::ShellMoved {
                redraw_shell_contents();
            } else {
                gui::cursor_draw();
            }
        }

        // Keyboard
        if let Some(sc) = keyboard_poll_scancode() {
            match sc {
                0x2A | 0x36 => { shift = true; continue; } // shift down
                0xAA | 0xB6 => { shift = false; continue; } // shift up
                _ => {}
            }

            if let Some(ch) = scancode_to_ascii(sc, shift) {
                match ch {
                    b'\n' => {
                        caret_off();
                        unsafe {
                            let n = INLEN;
                            push_line(&INBUF[..n]);
                            handle_command(&INBUF[..n]);
                            INLEN = 0;
                        }
                        redraw_shell_contents();
                    }
                    0x08 => {
                        unsafe { if INLEN > 0 { INLEN -= 1; } }
                        redraw_shell_contents();
                    }
                    c if c >= b' ' && c <= b'~' => {
                        unsafe {
                            if INLEN < MAX_LINE - 1 {
                                INBUF[INLEN] = c;
                                INLEN += 1;
                            }
                        }
                        redraw_shell_contents();
                    }
                    _ => {}
                }
            }
        }

        // caret blink
        caret_tick();
    }
}
