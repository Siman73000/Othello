#![allow(dead_code)]

// src/browser.rs
//
// Simple desktop web browser app for Othello OS.
// UI: omnibox + status line + scrollable text content.
// Network: crate::net::http::get()

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{gui, net};

const MAX_URL: usize = 256;
const MAX_FETCH: usize = 512 * 1024; // 512 KiB

// UI colors
const C_TOP: u32 = 0x1F1F1F;
const C_URL_BG: u32 = 0x2B2B2B;
const C_BODY: u32 = 0x101010;
const C_TEXT: u32 = 0xEAEAEA;
const C_STATUS_BG: u32 = 0x0E0E0E;
const C_STATUS_TEXT: u32 = 0xBFBFBF;
const C_CARET: u32 = 0xFFFFFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    UrlBar,
    Content,
}

struct BrowserState {
    url: String,
    caret: usize,
    focus: Focus,
    status: String,
    lines: Vec<String>,
    scroll: usize,
}

static mut ST: Option<BrowserState> = None;

fn st_mut() -> &'static mut BrowserState {
    unsafe {
        if ST.is_none() {
            ST = Some(BrowserState {
                url: "http://10.0.2.2:8000/".to_string(),
                caret: 0,
                focus: Focus::UrlBar,
                status: "Ready".to_string(),
                lines: Vec::new(),
                scroll: 0,
            });
        }
        ST.as_mut().unwrap()
    }
}

pub fn reset() {
    let st = st_mut();
    if st.url.is_empty() {
        st.url = "http://10.0.2.2:8000/".to_string();
    }
    st.caret = st.url.len().min(MAX_URL);
    st.focus = Focus::UrlBar;
    st.scroll = 0;
    st.status = "Ready".to_string();
    st.lines.clear();
}

fn normalize_url(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.starts_with("http://") || t.starts_with("https://") {
        return t.to_string();
    }
    // default to http
    format!("http://{t}")
}

fn ensure_net_ready() -> bool {
    let cfg = net::config();
    if cfg.ip != [0, 0, 0, 0] {
        return true;
    }
    let _ = net::dhcp_acquire();
    net::config().ip != [0, 0, 0, 0]
}

fn strip_html_to_lines(body: &[u8], max_lines: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_tag = false;

    for &b in body.iter().take(MAX_FETCH) {
        match b {
            b'<' => {
                in_tag = true;
                continue;
            }
            b'>' => {
                in_tag = false;
                continue;
            }
            _ => {}
        }
        if in_tag {
            continue;
        }

        match b {
            b'\r' => {}
            b'\n' => {
                if !cur.is_empty() {
                    out.push(cur.clone());
                    cur.clear();
                    if out.len() >= max_lines {
                        break;
                    }
                }
            }
            0x20..=0x7E => {
                cur.push(b as char);
                if cur.len() > 160 {
                    out.push(cur.clone());
                    cur.clear();
                    if out.len() >= max_lines {
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    if !cur.is_empty() && out.len() < max_lines {
        out.push(cur);
    }

    if out.is_empty() {
        out.push("(empty)".to_string());
    }

    out
}

fn fetch_now(st: &mut BrowserState) {
    if !ensure_net_ready() {
        st.status = "No IP (DHCP failed?)".to_string();
        st.lines.clear();
        st.lines.push("Network not configured".to_string());
        st.scroll = 0;
        return;
    }

    let url = normalize_url(&st.url);
    if url.is_empty() {
        st.status = "Enter a URL".to_string();
        return;
    }

    st.url = url.clone();
    st.caret = st.url.len().min(MAX_URL);
    st.status = format!("Fetching {url} ...");

    let resp: Vec<u8> = match net::http::get(&url, MAX_FETCH) {
        Ok(r) => {
            let ct = r.content_type.clone().unwrap_or_else(|| "?".to_string());
            st.status = format!("HTTP {} ({})", r.status, ct);
            r.body
        }
        Err(e) => {
            st.lines.clear();
            st.lines.push(format!("Failed to fetch: {url}"));
            st.lines.push(format!("Reason: {e:?}"));
            st.scroll = 0;
            st.status = "Failed".to_string();
            return;
        }
    };

    st.lines = strip_html_to_lines(&resp, 500);
    st.scroll = 0;
    st.status = format!("{} · {} bytes", st.status, resp.len());
}

fn draw_text_ascii_nocursor(mut x: i32, y: i32, s: &str, fg: u32, bg: u32) {
    for &b in s.as_bytes() {
        let ch = match b {
            0x20..=0x7E => b,
            b'\n' | b'\r' | b'\t' => b' ',
            _ => b'.',
        };
        gui::draw_byte_nocursor(x, y, ch, fg, bg);
        x += 8;
    }
}

pub fn render() {
    let st = st_mut();

    gui::begin_paint();

    let x0 = gui::shell_content_left();
    let y0 = gui::shell_content_top();
    let w = gui::shell_content_w();
    let h = gui::shell_content_h();

    // Background
    crate::framebuffer_driver::fill_rect(
        x0.max(0) as usize,
        y0.max(0) as usize,
        w.max(0) as usize,
        h.max(0) as usize,
        C_BODY,
    );

    // Top bar
    crate::framebuffer_driver::fill_rect(
        x0.max(0) as usize,
        y0.max(0) as usize,
        w.max(0) as usize,
        46,
        C_TOP,
    );

    // URL bar
    let url_x = x0 + 12;
    let url_y = y0 + 10;
    let url_w = (w - 24).max(64);
    let url_h = 28;
    gui::fill_round_rect_nocursor(url_x, url_y, url_w, url_h, 8, C_URL_BG);

    draw_text_ascii_nocursor(url_x + 10, url_y + 8, &st.url, C_TEXT, C_URL_BG);

    // caret
    if st.focus == Focus::UrlBar {
        let caret_px = (st.caret as i32) * 8;
        crate::framebuffer_driver::fill_rect(
            (url_x + 10 + caret_px).max(0) as usize,
            (url_y + 6).max(0) as usize,
            2,
            18,
            C_CARET,
        );
    }

    // Status line
    let status_y = y0 + h - 18;
    crate::framebuffer_driver::fill_rect(
        x0.max(0) as usize,
        status_y.max(0) as usize,
        w.max(0) as usize,
        18,
        C_STATUS_BG,
    );
    draw_text_ascii_nocursor(x0 + 10, status_y + 2, &st.status, C_STATUS_TEXT, C_STATUS_BG);

    // Content lines
    let content_y = y0 + 46 + 6;
    let content_h = (h - (46 + 6 + 18)).max(0);

    let mut cy = content_y + 6;
    let max_lines = (content_h / 16).max(1) as usize;
    let start = st.scroll.min(st.lines.len());
    let end = (start + max_lines).min(st.lines.len());

    for i in start..end {
        if cy + 16 > status_y {
            break;
        }
        draw_text_ascii_nocursor(x0 + 10, cy, &st.lines[i], C_TEXT, C_BODY);
        cy += 16;
    }

    gui::end_paint();
}

pub fn handle_char(ch: u8, ctrl: bool) -> bool {
    let st = st_mut();

    // Ctrl+L focuses URL bar
    if ctrl && (ch == b'l' || ch == b'L') {
        st.focus = Focus::UrlBar;
        return true;
    }

    match st.focus {
        Focus::UrlBar => match ch {
            b'\n' => {
                fetch_now(st);
                return true;
            }
            8u8 => {
                // backspace
                if st.caret > 0 && st.caret <= st.url.len() {
                    st.url.remove(st.caret - 1);
                    st.caret -= 1;
                    return true;
                }
            }
            b'\t' => {
                st.focus = Focus::Content;
                return true;
            }
            0x20..=0x7E => {
                if st.url.len() < MAX_URL {
                    let c = ch as char;
                    if st.caret >= st.url.len() {
                        st.url.push(c);
                        st.caret = st.url.len();
                    } else {
                        st.url.insert(st.caret, c);
                        st.caret += 1;
                    }
                    return true;
                }
            }
            _ => {}
        },
        Focus::Content => match ch {
            b'\t' => {
                st.focus = Focus::UrlBar;
                return true;
            }
            _ => {}
        },
    }

    false
}

pub fn handle_ext_scancode(sc: u8) -> bool {
    let st = st_mut();

    match st.focus {
        Focus::UrlBar => match sc {
            0x4B => {
                // left
                if st.caret > 0 {
                    st.caret -= 1;
                    return true;
                }
            }
            0x4D => {
                // right
                if st.caret < st.url.len() {
                    st.caret += 1;
                    return true;
                }
            }
            0x47 => {
                // home
                st.caret = 0;
                return true;
            }
            0x4F => {
                // end
                st.caret = st.url.len();
                return true;
            }
            _ => {}
        },
        Focus::Content => match sc {
            0x48 => {
                // up
                if st.scroll > 0 {
                    st.scroll -= 1;
                    return true;
                }
            }
            0x50 => {
                // down
                if st.scroll + 1 < st.lines.len() {
                    st.scroll += 1;
                    return true;
                }
            }
            0x49 => {
                // page up
                st.scroll = st.scroll.saturating_sub(20);
                return true;
            }
            0x51 => {
                // page down
                if !st.lines.is_empty() {
                    st.scroll = (st.scroll + 20).min(st.lines.len().saturating_sub(1));
                    return true;
                }
            }
            _ => {}
        },
    }

    false
}
