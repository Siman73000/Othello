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

use crate::{gui, net, web};

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
                url: "http://neverssl.com/".to_string(),
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

/// Draw ASCII text without touching cursor save/restore.
/// Must be called inside a `gui::begin_paint()/end_paint()` region.
fn draw_text_nc(x: i32, y: i32, text: &str, fg: u32, bg: u32) {
    let mut cx = x;
    let mut cy = y;
    for &b in text.as_bytes() {
        if b == b'\n' {
            cx = x;
            cy += 16;
            continue;
        }
        // Our font is ASCII-only; map non-printables to '?'.
        let ch = if (0x20..=0x7E).contains(&b) { b } else { b'?' };
        gui::draw_byte_nocursor(cx, cy, ch, fg, bg);
        cx += 8;
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
            let tag = core::str::from_utf8(&tag_buf[..tag_len])
                .unwrap_or("")
                .to_ascii_lowercase();
            if tag.starts_with("br")
                || tag.starts_with("/p")
                || tag.starts_with("p")
                || tag.starts_with("/div")
                || tag.starts_with("div")
                || tag.starts_with("li")
                || tag.starts_with("/li")
                || tag.starts_with("tr")
                || tag.starts_with("/tr")
                || tag.starts_with("h1")
                || tag.starts_with("h2")
                || tag.starts_with("h3")
                || tag.starts_with("/h")
            {
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
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
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
    if cfg.ip != [0, 0, 0, 0] {
        return true;
    }
    let _ = net::dhcp_acquire();
    net::config().ip != [0, 0, 0, 0]
}

fn push_history(st: &mut BrowserState, url: &str) {
    if st.history.is_empty() {
        st.history.push(url.to_string());
        st.hist_idx = 0;
        return;
    }

    if st.hist_idx < st.history.len() && st.history[st.hist_idx] == url {
        return;
    }

    if st.hist_idx + 1 < st.history.len() {
        st.history.truncate(st.hist_idx + 1);
    }
    st.history.push(url.to_string());
    st.hist_idx = st.history.len().saturating_sub(1);
}

fn go_back() -> bool {
    let ok = {
        let st = state_mut();
        if st.hist_idx == 0 || st.history.is_empty() {
            false
        } else {
            st.hist_idx -= 1;
            st.url = st.history[st.hist_idx].clone();
            st.caret = st.url.len();
            true
        }
    };
    if ok {
        navigate_current(false);
    }
    false
}

fn go_forward() -> bool {
    let ok = {
        let st = state_mut();
        if st.history.is_empty() || st.hist_idx + 1 >= st.history.len() {
            false
        } else {
            st.hist_idx += 1;
            st.url = st.history[st.hist_idx].clone();
            st.caret = st.url.len();
            true
        }
    };
    if ok {
        navigate_current(false);
    }
    false
}

fn reload() -> bool {
    navigate_current(false);
    false
}

fn navigate_current(push_hist: bool) {
    let url = {
        let st = state_mut();
        if st.url.is_empty() {
            return;
        }
        st.status = "Loading...".to_string();
        st.scroll = 0;
        st.url.clone()
    };

    // show loading state
    render();

    if !ensure_network() {
        let st = state_mut();
        st.status = "Network not configured (DHCP failed)".to_string();
        st.lines = vec!["No network. Run `dhcp` in Terminal, then retry.".to_string()];
        st.scroll = 0;
        render();
        return;
    }

    if push_hist {
        let st = state_mut();
        push_history(st, &url);
    }

    let resp = net::http::get(&url, MAX_FETCH);

    match resp {
        Ok(resp) => {
            let st = state_mut();
            st.status = format!("{} ({} bytes)", resp.status, resp.body.len());

            let mut body = resp.body;
            let ct = resp
                .content_type
                .unwrap_or_else(|| "application/octet-stream".to_string());
            let ct = ct.to_ascii_lowercase();
            let w = gui::shell_content_w() as u32;
            let cols = (w as usize / 8).saturating_sub(2).max(20);

            if ct.contains("text/html") {
                let mut page = web::html::parse(&body);
                // Parse CSS from <style> blocks
                let mut rules = alloc::vec::Vec::new();
                for css_text in &page.style_texts {
                    let mut r = web::css::parse_stylesheet(css_text);
                    rules.append(&mut r);
                }
                // Run tiny JS subset (document.write)
                web::js::run_scripts(&mut page.doc, &page.script_texts);

                st.lines = web::layout::render_text_lines(&page.doc, &rules, cols);
            } else {
                let text = bytes_to_lossy_string(&body);
                st.lines = wrap_lines(&text, cols);
            }
            st.scroll = 0;
        }
        Err(e) => {
            let st = state_mut();
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
        }
    }

    render();
}

pub fn reset() {
    let st = state_mut();
    st.url = "http://neverssl.com/".to_string();
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

    // seed history so back/forward don't underflow
    st.history.clear();
    st.hist_idx = 0;
    push_history(st, &st.url.clone());
}

fn draw_button(x: i32, y: i32, w: i32, h: i32, label: &str) {
    gui::fill_round_rect_nocursor(x, y, w, h, 6, 0x2B2B2B);
    draw_text_nc(x + 8, y + 8, label, 0xFFFFFF, 0x2B2B2B);
}

fn render_topbar() {
    let st = state_mut();

    let x0 = gui::shell_content_left();
    let y0 = gui::shell_content_top();
    let w = gui::shell_content_w();

    let top_h: i32 = 40;

    // repaint only the top chrome
    gui::fill_round_rect_nocursor(x0, y0, w, top_h, 10, 0x1F1F1F);

    let bx = x0 + 10;
    let by = y0 + 7;
    draw_button(bx, by, 28, 26, "<");
    draw_button(bx + 34, by, 28, 26, ">");
    draw_button(bx + 68, by, 34, 26, "R");

    let ux = bx + 110;
    let mut uw = w - (ux - x0) - 10;
    if uw < 0 {
        uw = 0;
    }
    gui::fill_round_rect_nocursor(ux, by, uw, 26, 10, 0x2B2B2B);

    // Avoid non-ASCII glyphs; keep it simple.
    let mut url_draw = st.url.clone();
    if url_draw.len() > 80 {
        // keep tail
        let keep = 77usize;
        let start = url_draw.len().saturating_sub(keep);
        url_draw = format!("...{}", &url_draw[start..]);
    }
    draw_text_nc(ux + 10, by + 7, &url_draw, 0xFFFFFF, 0x2B2B2B);

    if st.focus == Focus::UrlBar {
        // caret position in drawn URL (best-effort)
        let caret_cols = st.caret.min(80) as i32;
        let caret_x = (ux + 10) + caret_cols * 8;
        draw_text_nc(caret_x, by + 7, "|", 0xFFFFFF, 0x2B2B2B);
    }
}

fn render_content() {
    let st = state_mut();

    let x0 = gui::shell_content_left();
    let y0 = gui::shell_content_top();
    let w = gui::shell_content_w();
    let h = gui::shell_content_h();

    let top_h: i32 = 40;
    let status_h: i32 = 22;

    let content_y = y0 + top_h + 6;
    let mut content_h = h - (top_h + status_h + 12);
    if content_h < 0 {
        content_h = 0;
    }

    gui::fill_round_rect_nocursor(x0, content_y, w, content_h, 10, 0x101010);

    let max_lines = (content_h as usize / 16).saturating_sub(1).max(1);
    let start = st.scroll.min(st.lines.len());
    let end = (start + max_lines).min(st.lines.len());
    let mut y = content_y + 10;

    for line in &st.lines[start..end] {
        draw_text_nc(x0 + 12, y, line, 0xEAEAEA, 0x101010);
        y += 16;
    }
}

fn render_status() {
    let st = state_mut();

    let x0 = gui::shell_content_left();
    let y0 = gui::shell_content_top();
    let w = gui::shell_content_w();
    let h = gui::shell_content_h();

    let status_h: i32 = 22;
    let sy = y0 + h - status_h;

    gui::fill_round_rect_nocursor(x0, sy, w, status_h, 10, 0x1F1F1F);
    draw_text_nc(x0 + 12, sy + 4, &st.status, 0xBFBFBF, 0x1F1F1F);
}

pub fn render() {
    gui::begin_paint();
    gui::clear_shell_content_and_frame_nocursor();

    render_topbar();
    render_content();
    render_status();

    gui::end_paint();
}

pub fn handle_char(ch: u8, ctrl: bool) -> bool {
    if ctrl {
        match ch {
            // Focus URL bar (like Ctrl+L)
            b'l' | b'L' => {
                {
                    let st = state_mut();
                    st.focus = Focus::UrlBar;
                    st.caret = st.url.len();
                }
                gui::begin_paint();
                render_topbar();
                gui::end_paint();
                return false;
            }
            // Reload
            b'r' | b'R' => {
                reload();
                return false;
            }
            _ => {}
        }
    }

    match ch {
        // Toggle focus (URL bar <-> content)
        b'\t' => {
            {
                let st = state_mut();
                st.focus = if st.focus == Focus::UrlBar {
                    Focus::Content
                } else {
                    Focus::UrlBar
                };
                if st.focus == Focus::UrlBar {
                    st.caret = st.url.len();
                }
            }
            gui::begin_paint();
            render_topbar();
            gui::end_paint();
            false
        }

        // Backspace
        8 => {
            let mut did = false;
            {
                let st = state_mut();
                if st.focus == Focus::UrlBar && st.caret > 0 && !st.url.is_empty() {
                    st.url.remove(st.caret - 1);
                    st.caret -= 1;
                    did = true;
                }
            }
            if did {
                gui::begin_paint();
                render_topbar();
                gui::end_paint();
            }
            false
        }

        // Enter
        13 | b'\n' => {
            let focus_url = { state_mut().focus == Focus::UrlBar };
            if focus_url {
                // normalize caret to end
                {
                    let st = state_mut();
                    st.caret = st.url.len();
                }
                navigate_current(true);
            }
            false
        }

        // Printable chars
        b => {
            let mut did = false;
            {
                let st = state_mut();
                if st.focus == Focus::UrlBar && st.url.len() < MAX_URL && b >= 0x20 && b <= 0x7E {
                    st.url.insert(st.caret, b as char);
                    st.caret += 1;
                    did = true;
                }
            }
            if did {
                gui::begin_paint();
                render_topbar();
                gui::end_paint();
            }
            false
        }
    }
}


pub fn handle_wheel(delta: i32) -> bool {
    let st = state_mut();
    if st.lines.is_empty() { return false; }

    // Positive delta typically means scroll up.
    if delta > 0 {
        let step = (delta as usize).saturating_mul(3);
        st.scroll = st.scroll.saturating_sub(step);
    } else {
        let step = ((-delta) as usize).saturating_mul(3);
        st.scroll = (st.scroll + step).min(st.lines.len().saturating_sub(1));
    }
    true
}

pub fn handle_ext_scancode(sc: u8) -> bool {
    match sc {
        // Left
        0x4B => {
            let mut did = false;
            {
                let st = state_mut();
                if st.focus == Focus::UrlBar && st.caret > 0 {
                    st.caret -= 1;
                    did = true;
                }
            }
            if did {
                gui::begin_paint();
                render_topbar();
                gui::end_paint();
            }
            return false;
        }

        // Right
        0x4D => {
            let mut did = false;
            {
                let st = state_mut();
                if st.focus == Focus::UrlBar && st.caret < st.url.len() {
                    st.caret += 1;
                    did = true;
                }
            }
            if did {
                gui::begin_paint();
                render_topbar();
                gui::end_paint();
            }
            return false;
        }

        // Home
        0x47 => {
            let mut did = false;
            {
                let st = state_mut();
                if st.focus == Focus::UrlBar {
                    st.caret = 0;
                    did = true;
                }
            }
            if did {
                gui::begin_paint();
                render_topbar();
                gui::end_paint();
            }
            return false;
        }

        // End
        0x4F => {
            let mut did = false;
            {
                let st = state_mut();
                if st.focus == Focus::UrlBar {
                    st.caret = st.url.len();
                    did = true;
                }
            }
            if did {
                gui::begin_paint();
                render_topbar();
                gui::end_paint();
            }
            return false;
        }

        // Up (content scroll)
        0x48 => {
            let mut did = false;
            {
                let st = state_mut();
                if st.focus == Focus::Content && st.scroll > 0 {
                    st.scroll -= 1;
                    did = true;
                }
            }
            if did {
                gui::begin_paint();
                render_content();
                gui::end_paint();
            }
            return false;
        }

        // Down (content scroll)
        0x50 => {
            let mut did = false;
            {
                let st = state_mut();
                if st.focus == Focus::Content && st.scroll + 1 < st.lines.len() {
                    st.scroll += 1;
                    did = true;
                }
            }
            if did {
                gui::begin_paint();
                render_content();
                gui::end_paint();
            }
            return false;
        }

        // PageUp
        0x49 => {
            let mut did = false;
            {
                let st = state_mut();
                if st.focus == Focus::Content {
                    st.scroll = st.scroll.saturating_sub(10);
                    did = true;
                }
            }
            if did {
                gui::begin_paint();
                render_content();
                gui::end_paint();
            }
            return false;
        }

        // PageDown
        0x51 => {
            let mut did = false;
            {
                let st = state_mut();
                if st.focus == Focus::Content && !st.lines.is_empty() {
                    st.scroll = (st.scroll + 10).min(st.lines.len().saturating_sub(1));
                    did = true;
                }
            }
            if did {
                gui::begin_paint();
                render_content();
                gui::end_paint();
            }
            return false;
        }

        // Delete
        0x53 => {
            let mut did = false;
            {
                let st = state_mut();
                if st.focus == Focus::UrlBar && st.caret < st.url.len() {
                    st.url.remove(st.caret);
                    did = true;
                }
            }
            if did {
                gui::begin_paint();
                render_topbar();
                gui::end_paint();
            }
            return false;
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
