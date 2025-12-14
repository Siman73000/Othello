#![allow(dead_code)]
use core::arch::asm;

use crate::{framebuffer_driver as fb, gui, keyboard, mouse, net};

const MAX_LINE: usize = 128;
const MAX_LINES: usize = 64;

static mut LINES: [[u8; MAX_LINE]; MAX_LINES] = [[0; MAX_LINE]; MAX_LINES];
static mut LENS: [usize; MAX_LINES] = [0; MAX_LINES];
static mut LINE_COUNT: usize = 0;

static mut INBUF: [u8; MAX_LINE] = [0; MAX_LINE];
static mut INLEN: usize = 0;

static mut CARET_ON: bool = true;
static mut LAST_TSC: u64 = 0;

#[inline]
fn rdtsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe { asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags)); }
    ((hi as u64) << 32) | (lo as u64)
}

fn push_line(s: &[u8]) {
    unsafe {
        if LINE_COUNT < MAX_LINES {
            let i = LINE_COUNT;
            let n = s.len().min(MAX_LINE);
            LINES[i][..n].copy_from_slice(&s[..n]);
            LENS[i] = n;
            LINE_COUNT += 1;
        } else {
            // scroll up
            for i in 1..MAX_LINES {
                LINES[i-1] = LINES[i];
                LENS[i-1] = LENS[i];
            }
            let n = s.len().min(MAX_LINE);
            LINES[MAX_LINES-1][..n].copy_from_slice(&s[..n]);
            LENS[MAX_LINES-1] = n;
        }
    }
}

fn clear_lines() {
    unsafe {
        LINE_COUNT = 0;
        for i in 0..MAX_LINES { LENS[i] = 0; }
    }
}

fn draw_prompt_and_input() {
    let fg = gui::SHELL_FG_COLOR;
    let bg = gui::SHELL_BG_COLOR;

    let px = gui::shell_footer_x() + 4;
    let py = gui::shell_footer_y() + 1;
    // footer background
    fb::fill_rect(gui::shell_footer_x() as usize, gui::shell_footer_y() as usize, gui::shell_footer_w() as usize, gui::shell_footer_h() as usize, bg);
    gui::draw_text(px, py, "> ", fg, bg);

    unsafe {
        // draw input text
        let mut x = px + 16;
        for i in 0..INLEN {
            let ch = INBUF[i];
            let s = [ch];
            gui::draw_text(x, py, core::str::from_utf8(&s).unwrap_or("?"), fg, bg);
            x += 8;
        }

        // caret
        if CARET_ON {
            fb::fill_rect(x as usize, (py + 2) as usize, 8, 12, 0x38BDF8);
        }
    }
}

fn redraw_terminal() {
    gui::clear_shell_content();

    let fg = gui::SHELL_FG_COLOR;
    let bg = gui::SHELL_BG_COLOR;

    let x0 = gui::shell_content_left();
    let y0 = gui::shell_content_top();
    let max_lines_vis = (gui::shell_content_h() as usize / 16).max(1);

    unsafe {
        let start = if LINE_COUNT > max_lines_vis { LINE_COUNT - max_lines_vis } else { 0 };
        let mut y = y0;
        for i in start..LINE_COUNT {
            let n = LENS[i];
            if n == 0 { y += 16; continue; }
            // draw line
            let bytes = &LINES[i][..n];
            // split to printable
            let mut cx = x0;
            for &b in bytes {
                if b == b'\n' || b == b'\r' { continue; }
                let s = [b];
                gui::draw_text(cx, y, core::str::from_utf8(&s).unwrap_or("?"), fg, bg);
                cx += 8;
                if cx >= x0 + gui::shell_content_w() - 8 { break; }
            }
            y += 16;
            if y >= y0 + gui::shell_content_h() - 16 { break; }
        }
    }

    draw_prompt_and_input();
}

fn exec_command(line: &[u8]) {
    let s = core::str::from_utf8(line).unwrap_or("");
    let s = s.trim();

    if s.is_empty() { return; }

    if s == "help" {
        push_line(b"Commands: help, clear, echo <text>, net");
    } else if s == "clear" {
        clear_lines();
    } else if s.starts_with("echo ") {
        push_line(s[5..].as_bytes());
    } else if s == "net" {
        let r = net::net_scan();
        for &d in r.devices {
            push_line(d.as_bytes());
        }
    } else {
        push_line(b"Unknown command. Try: help");
    }
}

pub fn run_shell() -> ! {
    // Draw initial UI + a welcome
    push_line(b"Othello Shell (mouse: drag title bar). Type 'help'.");
    redraw_terminal();

    unsafe { LAST_TSC = rdtsc(); }

    let mut shift = false;

    loop {
        // Process all mouse packets for smoothness
        let max_w = fb::width() as i32;
        let max_h = fb::height() as i32;
        while let Some(ms) = mouse::mouse_poll(max_w, max_h) {
            let act = gui::ui_handle_mouse(ms);
            if act == gui::UiAction::ShellMoved {
                // Repaint terminal elements inside window at new location (content + footer)
                redraw_terminal();
            }
        }

        // Keyboard
        while let Some(sc) = keyboard::keyboard_poll_scancode() {
            // shift make/break: 0x2A/0x36 down, 0xAA/0xB6 up
            if sc == 0x2A || sc == 0x36 { shift = true; continue; }
            if sc == 0xAA || sc == 0xB6 { shift = false; continue; }

            if let Some(ch) = keyboard::scancode_to_ascii(sc, shift) {
                unsafe {
                    match ch {
                        b'\n' => {
                            // commit
                            push_line(&INBUF[..INLEN]);
                            exec_command(&INBUF[..INLEN]);
                            INLEN = 0;
                            redraw_terminal();
                        }
                        0x08 => {
                            if INLEN > 0 {
                                INLEN -= 1;
                                draw_prompt_and_input();
                            }
                        }
                        _ => {
                            if INLEN < MAX_LINE {
                                INBUF[INLEN] = ch;
                                INLEN += 1;
                                draw_prompt_and_input();
                            }
                        }
                    }
                }
            }
        }

        // caret blink (rough): toggle every ~200M cycles; tweak if needed
        let now = rdtsc();
        unsafe {
            if now.wrapping_sub(LAST_TSC) > 200_000_000 {
                LAST_TSC = now;
                CARET_ON = !CARET_ON;
                draw_prompt_and_input();
            }
        }
    }
}
