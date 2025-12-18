#![allow(dead_code)]

//! Simple desktop web browser app.
//!
//! UI: "Chrome-ish" omnibox + buttons (keyboard-driven for now).
//! Rendering: text-only (basic HTML stripping).
//!
//! Network:
//! - HTTP via in-kernel DNS+TCP
//! - HTTPS via optional host-side proxy (see net::http and tools/https_proxy.py)

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{gui, net};

const MAX_URL: usize = 256;
const MAX_FETCH: usize = 512 * 1024; // 512 KiB

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
    scroll: usize, // line offset
    history: Vec<String>,
    hist_idx: usize,
}

static mut STATE: Option<BrowserState> = None;

fn state_mut() -> &'static mut BrowserState {
    unsafe {
        if STATE.is_none() {
            STATE = Some(BrowserState {
                url: "https://example.com/".to_string(),
                caret: 0,
                focus: Focus::UrlBar,
                status: "Ready".to_string(),
                lines: Vec::new(),
                scroll: 0,
                history: Vec::new(),
                hist_idx: 0,
            });
        }
        STATE.as_mut().unwrap()
    }
}

fn bytes_to_lossy_string(bytes: &[u8]) -> String {
    let mut s = String::new();
    for &b in bytes {
        match b {
            0x20..=0x7E | b'\n' | b'\r' | b'\t' => s.push(b as char),
            _ => s.push('?'),
        }
    }
    s
}

fn html_to_text(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut in_tag = false;
    let mut tag_buf: [u8; 16] = [0; 16];
    let mut tag_len = 0usize;

    for &b in bytes {
        if !in_tag {
            if b == b'<' {
                in_tag = true;
                tag_len = 0;
            } else {
                out.push(b);
            }
        } else if b == b'>' {
            in_tag = false;
            let tag = core::str::from_utf8(&tag_buf[..tag_len]).unwrap_or("").to_ascii_lowercase();
            if tag.starts_with("br") || tag.starts_with("/p") || tag.starts_with("p") ||
               tag.starts_with("/div") || tag.starts_with("div") ||
               tag.starts_with("li") || tag.starts_with("/li") ||
               tag.starts_with("tr") || tag.starts_with("/tr") ||
               tag.starts_with("h1") || tag.starts_with("h2") || tag.starts_with("h3") || tag.starts_with("/h") {
                out.push(b'\n');
            }
        } else {
            if tag_len < tag_buf.len() {
                tag_buf[tag_len] = b;
                tag_len += 1;
            }
        }
    }
    out
}

fn safe_boundary(s: &str, idx: usize) -> usize {
    let mut i = idx.min(s.len());
    while i > 0 && !s.is_char_boundary(i) { i -= 1; }
    i
}

fn wrap_lines(text: &str, max_cols: usize) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.replace("\r\n", "\n").split('\n') {
        let mut line = raw.trim_end().to_string();
        if line.is_empty() {
            out.push(String::new());
            continue;
        }
        while line.len() > max_cols && max_cols > 8 {
            let cut = safe_boundary(&line, max_cols);
            let split_at = line[..cut].rfind(' ').unwrap_or(cut);
            out.push(line[..split_at].trim_end().to_string());
            line = line[split_at..].trim_start().to_string();
        }
        out.push(line);
    }
    out
}

fn ensure_network() -> bool {
    let cfg = net::config();
    if cfg.ip != [0,0,0,0] { return true; }
    net::dhcp_acquire();
    net::config().ip != [0,0,0,0]
}

fn push_history(url: &str) {
    let st = state_mut();
    if st.hist_idx + 1 < st.history.len() {
        st.history.truncate(st.hist_idx + 1);
    }
    st.history.push(url.to_string());
    st.hist_idx = st.history.len().saturating_sub(1);
}

fn go_back() -> bool {
    let st = state_mut();
    if st.hist_idx == 0 || st.history.is_empty() { return false; }
    st.hist_idx -= 1;
    st.url = st.history[st.hist_idx].clone();
    st.caret = st.url.len();
    navigate_current()
}

fn go_forward() -> bool {
    let st = state_mut();
    if st.history.is_empty() { return false; }
    if st.hist_idx + 1 >= st.history.len() { return false; }
    st.hist_idx += 1;
    st.url = st.history[st.hist_idx].clone();
    st.caret = st.url.len();
    navigate_current()
}

fn reload() -> bool { navigate_current() }

fn navigate_current() -> bool {
    let st = state_mut();
    if st.url.is_empty() { return false; }

    st.status = "Loading...".to_string();
    render();

    if !ensure_network() {
        st.status = "Network not configured (DHCP failed)".to_string();
        st.lines = vec!["No network. Run `dhcp` in Terminal, then retry.".to_string()];
        st.scroll = 0;
        return true;
    }

    let url = st.url.clone();
    push_history(&url);

    let resp = match net::http::get(&url, MAX_FETCH) {
        Ok(r) => r,
        Err(e) => {
            st.status = "Fetch failed".to_string();
            let reason = match e {
                net::http::HttpError::Dns => "Dns".to_string(),
                net::http::HttpError::Parse => "Parse".to_string(),
                net::http::HttpError::RedirectLoop => "RedirectLoop".to_string(),
                net::http::HttpError::UnsupportedScheme => "UnsupportedScheme".to_string(),
                net::http::HttpError::Tcp(te) => format!("Tcp ({:?})", te),
            };
            st.lines = vec![format!("Failed to fetch: {}", url), format!("Reason: {}", reason)];
            st.scroll = 0;
            return true;
        }
    };

    st.status = format!("{} ({} bytes)", resp.status, resp.body.len());

    let mut body = resp.body;
    let ct = resp.content_type.unwrap_or_else(|| "application/octet-stream".to_string());
    if ct.to_ascii_lowercase().contains("text/html") {
        body = html_to_text(&body);
    }

    let text = bytes_to_lossy_string(&body);
    let w = gui::shell_content_w() as u32;
    let cols = (w as usize / 8).saturating_sub(2).max(20);
    st.lines = wrap_lines(&text, cols);
    st.scroll = 0;

    true
}

pub fn reset() {
    let st = state_mut();
    st.url = "https://example.com/".to_string();
    st.caret = st.url.len();
    st.focus = Focus::UrlBar;
    st.status = "Ready".to_string();
    st.lines = vec![
        "Welcome to Othello Browser (text mode)".to_string(),
        "".to_string(),
        "Type a URL and press Enter.".to_string(),
        "Examples:".to_string(),
        "  http://example.com/".to_string(),
        "  https://example.com/  (uses optional host-side HTTPS proxy)".to_string(),
        "".to_string(),
        "Tips: Ctrl+L focus URL, Ctrl+R reload, Tab switch focus, arrows scroll.".to_string(),
    ];
    st.scroll = 0;
}

fn draw_button(x: i32, y: i32, w: i32, h: i32, label: &str) {
    gui::fill_round_rect_nocursor(x, y, w, h, 6, 0x2B2B2B);
    gui::draw_text(x + 8, y + 8, label, 0xFFFFFF, 0x2B2B2B);
}

pub fn render() {
    gui::clear_shell_content_and_frame();
    gui::begin_paint();

    let st = state_mut();

    let x0 = gui::shell_content_left();
    let y0 = gui::shell_content_top();
    let w  = gui::shell_content_w();
    let h  = gui::shell_content_h();

    let top_h: i32 = 40;
    let status_h: i32 = 22;

    gui::fill_round_rect_nocursor(x0, y0, w, top_h, 10, 0x1F1F1F);

    let bx = x0 + 10;
    let by = y0 + 7;
    draw_button(bx, by, 28, 26, "<");
    draw_button(bx + 34, by, 28, 26, ">");
    draw_button(bx + 68, by, 34, 26, "R");

    let ux = bx + 110;
    let mut uw = w - (ux - x0) - 10;
    if uw < 0 { uw = 0; }
    gui::fill_round_rect_nocursor(ux, by, uw, 26, 10, 0x2B2B2B);

    let mut url_draw = st.url.clone();
    if url_draw.len() > 80 {
        url_draw = format!("…{}", &url_draw[url_draw.len()-79..]);
    }
    gui::draw_text(ux + 10, by + 7, &url_draw, 0xFFFFFF, 0x2B2B2B);

    if st.focus == Focus::UrlBar {
        let caret_x = (ux + 10) + (st.caret.min(80) as i32) * 8;
        gui::draw_text(caret_x, by + 7, "|", 0xFFFFFF, 0x2B2B2B);
    }

    let content_y = y0 + top_h + 6;
    let mut content_h = h - (top_h + status_h + 12);
    if content_h < 0 { content_h = 0; }
    gui::fill_round_rect_nocursor(x0, content_y, w, content_h, 10, 0x101010);

    let max_lines = (content_h as usize / 16).saturating_sub(1).max(1);
    let start = st.scroll.min(st.lines.len());
    let end = (start + max_lines).min(st.lines.len());
    let mut y = content_y + 10;

    for line in &st.lines[start..end] {
        gui::draw_text(x0 + 12, y, line, 0xEAEAEA, 0x101010);
        y += 16;
    }

    let sy = y0 + h - status_h;
    gui::fill_round_rect_nocursor(x0, sy, w, status_h, 10, 0x1F1F1F);
    gui::draw_text(x0 + 12, sy + 4, &st.status, 0xBFBFBF, 0x1F1F1F);

    gui::end_paint();
}

pub fn handle_char(ch: u8, ctrl: bool) -> bool {
    let st = state_mut();

    if ctrl {
        match ch {
            b'l' | b'L' => { st.focus = Focus::UrlBar; st.caret = st.url.len(); return true; }
            b'r' | b'R' => { return reload(); }
            _ => {}
        }
    }

    match ch {
        b'\t' => {
            st.focus = if st.focus == Focus::UrlBar { Focus::Content } else { Focus::UrlBar };
            true
        }
        8 => {
            if st.focus == Focus::UrlBar && st.caret > 0 && !st.url.is_empty() {
                st.url.remove(st.caret - 1);
                st.caret -= 1;
                true
            } else { false }
        }
        13 | b'\n' => {
            if st.focus == Focus::UrlBar {
                st.caret = st.url.len();
                navigate_current()
            } else { false }
        }
        b => {
            if st.focus == Focus::UrlBar && st.url.len() < MAX_URL && b >= 0x20 && b <= 0x7E {
                st.url.insert(st.caret, b as char);
                st.caret += 1;
                true
            } else { false }
        }
    }
}

pub fn handle_ext_scancode(sc: u8) -> bool {
    let st = state_mut();
    match sc {
        0x4B => {
            if st.focus == Focus::UrlBar && st.caret > 0 { st.caret -= 1; return true; }
        }
        0x4D => {
            if st.focus == Focus::UrlBar && st.caret < st.url.len() { st.caret += 1; return true; }
        }
        0x47 => {
            if st.focus == Focus::UrlBar { st.caret = 0; return true; }
        }
        0x4F => {
            if st.focus == Focus::UrlBar { st.caret = st.url.len(); return true; }
        }
        0x48 => {
            if st.focus == Focus::Content && st.scroll > 0 { st.scroll -= 1; return true; }
        }
        0x50 => {
            if st.focus == Focus::Content && st.scroll + 1 < st.lines.len() { st.scroll += 1; return true; }
        }
        0x49 => {
            if st.focus == Focus::Content { st.scroll = st.scroll.saturating_sub(10); return true; }
        }
        0x51 => {
            if st.focus == Focus::Content { st.scroll = (st.scroll + 10).min(st.lines.len().saturating_sub(1)); return true; }
        }
        0x53 => {
            if st.focus == Focus::UrlBar && st.caret < st.url.len() { st.url.remove(st.caret); return true; }
        }
        _ => {}
    }
    false
}

pub fn on_button(index: u8) -> bool {
    match index {
        0 => go_back(),
        1 => go_forward(),
        2 => reload(),
        _ => false,
    }
}
