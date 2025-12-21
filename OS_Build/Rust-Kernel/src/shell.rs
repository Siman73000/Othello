#![allow(dead_code)]

use core::arch::asm;
use alloc::vec::Vec;

use crate::{framebuffer_driver as fb, gui, keyboard, login, mouse, net, regedit, editor, browser, fs, time};
use crate::serial_write_str;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppState {
    Login,
    Terminal,
    Regedit,
    Editor,
    Browser,
}


/// Which taskbar icon should show the "running" indicator for the active view.
/// 0 = Terminal, 2 = Login/Lock, 5 = Registry
pub fn active_taskbar_index() -> u8 {
    unsafe {
        match APP {
            AppState::Terminal => 0,
            AppState::Login => 2,
            AppState::Regedit => 5,
            AppState::Editor => 4,
            AppState::Browser => 6,
        }
    }
}

static mut APP: AppState = AppState::Login;

fn set_app(app: AppState) {
    // Don't allow entering terminal/regedit without an authenticated session.
    if (app == AppState::Terminal || app == AppState::Regedit || app == AppState::Editor || app == AppState::Browser) && !login::is_logged_in() {
        unsafe { APP = AppState::Login; }
        gui::set_ui_mode(gui::UiMode::Login);
        gui::set_shell_visible(false);
        login::reset();
        login::render_fullscreen();
        return;
    }

    unsafe { APP = app; }
    match app {
        AppState::Login => {
            gui::set_ui_mode(gui::UiMode::Login);
            gui::set_shell_visible(false);
            login::reset();
            login::render_fullscreen();
        }
        AppState::Terminal => {
            gui::set_ui_mode(gui::UiMode::Desktop);
            gui::set_shell_visible(true);
            gui::set_shell_maximized(true);
            gui::set_shell_title("Terminal");
        }
        AppState::Regedit => {
            gui::set_ui_mode(gui::UiMode::Desktop);
            gui::set_shell_visible(true);
            gui::set_shell_maximized(true);
            gui::set_shell_title("Registry");
            regedit::reset();
        }
        AppState::Editor => {
            gui::set_ui_mode(gui::UiMode::Desktop);
            gui::set_shell_visible(true);
            gui::set_shell_maximized(true);
            gui::set_shell_title("Text Editor");
        }
        AppState::Browser => {
            gui::set_ui_mode(gui::UiMode::Desktop);
            gui::set_shell_visible(true);
            gui::set_shell_maximized(true);
            gui::set_shell_title("Browser");
            crate::browser::reset();
        }
    }
    render_active_full();
}

fn render_active_full() {
    unsafe {
        match APP {
            AppState::Login => login::render_fullscreen(),
            AppState::Terminal => render_terminal_full(),
            AppState::Regedit => regedit::render(),
            AppState::Editor => editor::render(),
            AppState::Browser => crate::browser::render(),
        }
    }
}

// -----------------------------------------------------------------------------
// Minimal, direct-to-framebuffer terminal
// -----------------------------------------------------------------------------
//
// The previous “full” terminal stored a large scrollback buffer in static arrays.
// On some boot paging setups, touching large .bss regions can trigger a fault
// (and with our current IDT handler, that looks like the system “freezes”).
//
// This implementation keeps state tiny and draws lines directly into the shell
// content area, with a simple software scroll.

const FG: u32 = gui::SHELL_FG_COLOR;
const BG: u32 = gui::SHELL_BG_COLOR;
const DIM: u32 = 0x94A3B8;
const ERR: u32 = 0xF87171;
const OK: u32  = 0x34D399;

const STATUS_H: i32 = 20;
const PAD: i32 = 8;
const CH_W: i32 = 8;
const CH_H: i32 = 16;

static mut TERM_X: i32 = 0;
static mut TERM_Y0: i32 = 0;
static mut TERM_W: i32 = 0;
static mut TERM_H: i32 = 0;
static mut TERM_Y: i32 = 0;

static mut INBUF: [u8; 160] = [0; 160];
static mut INLEN: usize = 0;
static mut CARET: usize = 0;
static mut CARET_ON: bool = true;

// Scrollback buffer: store printed lines so we can redraw the terminal viewport
// without using blit operations (avoids cursor/restore artifacts).
const SCROLLBACK_MAX_LINES: usize = 10000;
static mut SCROLLBACK: Option<Vec<Vec<u8>>> = None;
// Index of the first scrollback line currently shown in the viewport
static mut SCROLLBACK_POS: usize = 0;
// If true the user has interacted with scrollback (wheel) and we should not
// auto-pin to bottom until the user types or a command is executed.
static mut SCROLLBACK_USER_SCROLLED: bool = false;

fn layout() {
    unsafe {
        let x = gui::shell_content_left();
        let y = gui::shell_content_top();
        let w = gui::shell_content_w();
        let h = gui::shell_content_h();

        TERM_X = x + PAD;
        TERM_Y0 = y + STATUS_H + PAD;
        TERM_W = (w - PAD * 2).max(0);
        TERM_H = (h - STATUS_H - PAD * 2).max(0);
        TERM_Y = TERM_Y0;
    }
}



fn sync_layout_preserve() {
    unsafe {
        // Preserve current cursor position relative to the text region while
        // the window is moved by blitting pixels in gui::ui_handle_mouse().
        let old_y0 = TERM_Y0;
        let old_y  = TERM_Y;

        let x = gui::shell_content_left();
        let y = gui::shell_content_top();
        let w = gui::shell_content_w();
        let h = gui::shell_content_h();

        TERM_X = x + PAD;
        TERM_Y0 = y + STATUS_H + PAD;
        TERM_W = (w - PAD * 2).max(0);
        TERM_H = (h - STATUS_H - PAD * 2).max(0);

        let dy = TERM_Y0 - old_y0;
        TERM_Y = old_y + dy;

        if TERM_H <= 0 {
            TERM_Y = TERM_Y0;
        } else {
            let miny = TERM_Y0;
            let maxy = TERM_Y0 + TERM_H - CH_H;
            TERM_Y = TERM_Y.clamp(miny, maxy.max(miny));
        }
    }
}
#[inline]
fn clear_rect(x: i32, y: i32, w: i32, h: i32, c: u32) {
    if w <= 0 || h <= 0 { return; }
    fb::fill_rect(x.max(0) as usize, y.max(0) as usize, w.max(0) as usize, h.max(0) as usize, c);
}

fn draw_status_bar() {
    if !gui::shell_is_visible() { return; }
    let x = gui::shell_content_left();
    let y = gui::shell_content_top();
    let w = gui::shell_content_w();
    if w <= 0 { return; }
    gui::begin_paint();
    clear_rect(x, y, w, STATUS_H, 0x0B1220);

    // "Terminal - <user>" (if logged in)
    let mut line = [0u8; 64];
    let mut n = 0usize;
    let base = b"Terminal";
    line[..base.len()].copy_from_slice(base);
    n += base.len();
    if login::is_logged_in() {
        let sep = b" - ";
        if n + sep.len() < line.len() {
            line[n..n + sep.len()].copy_from_slice(sep);
            n += sep.len();
        }
        let u = login::current_user_bytes();
        let take = u.len().min(line.len().saturating_sub(n));
        line[n..n + take].copy_from_slice(&u[..take]);
        n += take;
    } else {
        let s = b" (locked)";
        let take = s.len().min(line.len().saturating_sub(n));
        line[n..n + take].copy_from_slice(&s[..take]);
        n += take;
    }

    draw_bytes_line_clip_nocursor(x + PAD, y + 2, &line[..n], 0xE5E7EB, 0x0B1220, x + w - PAD);
    gui::end_paint();
}

fn term_clear() {
    if !gui::shell_is_visible() { return; }
    unsafe {
        clear_rect(TERM_X, TERM_Y0, TERM_W, TERM_H, BG);
        TERM_Y = TERM_Y0;
    }
}

fn draw_bytes_line_clip_nocursor(x: i32, y: i32, bytes: &[u8], fg: u32, bg: u32, clip_r: i32) -> usize {
    // Draws bytes starting at (x,y) with clipping to clip_r, wrapping to next
    // line when the current line width is exceeded. Returns the number of
    // text lines actually drawn (>=1 if bytes non-empty and some glyphs drawn).
    if bytes.is_empty() { return 0; }
    let mut cx = x;
    let mut cy = y;
    let mut lines_drawn: usize = 1;

    for &b in bytes {
        if b == b'\n' { break; }

        // Printable ascii only
        if b >= 0x20 && b <= 0x7E {
            // If the next glyph would overflow the clip, wrap to next line
            if cx + CH_W > clip_r {
                cx = x;
                cy += CH_H;
                lines_drawn += 1;

                // If we would draw past the terminal visible region, stop.
                unsafe {
                    // If TERM_H is 0 we can't determine bounds; stop defensively.
                    if TERM_H <= 0 { break; }
                    let max_y = TERM_Y0 + TERM_H - CH_H;
                    if cy > max_y { break; }
                }
            }

            gui::draw_byte_nocursor(cx, cy, b, fg, bg);
            cx += CH_W;
        } else {
            // Non-printable: treat as space (skip) unless newline handled above
            if cx + CH_W > clip_r {
                cx = x;
                cy += CH_H;
                lines_drawn += 1;
                unsafe {
                    if TERM_H <= 0 { break; }
                    let max_y = TERM_Y0 + TERM_H - CH_H;
                    if cy > max_y { break; }
                }
            }
            cx += CH_W;
        }
    }

    lines_drawn
}

fn draw_bytes_line_nocursor(x: i32, y: i32, bytes: &[u8], fg: u32, bg: u32) -> usize {
    let clip_r = x + unsafe { TERM_W };
    draw_bytes_line_clip_nocursor(x, y, bytes, fg, bg, clip_r)
}

fn draw_bytes_line(x: i32, y: i32, bytes: &[u8], fg: u32, bg: u32) -> usize {
    gui::begin_paint();
    let n = draw_bytes_line_nocursor(x, y, bytes, fg, bg);
    gui::end_paint();
    n
}

fn scroll_if_needed() {
    unsafe {
        if TERM_H <= 0 { return; }
        if TERM_Y + CH_H <= TERM_Y0 + TERM_H { return; }

        // Scroll up by one line inside the terminal text region.
        fb::blit_move_rect(
            TERM_X,
            TERM_Y0 + CH_H,
            TERM_W,
            TERM_H - CH_H,
            TERM_X,
            TERM_Y0,
        );

        // Clear the last line.
        clear_rect(TERM_X, TERM_Y0 + TERM_H - CH_H, TERM_W, CH_H, BG);

        TERM_Y = TERM_Y0 + TERM_H - CH_H;
    }
}

fn print_line(bytes: &[u8], fg: u32) {
    if !gui::shell_is_visible() { return; }
    unsafe {
        // Ensure scrollback exists
        if SCROLLBACK.is_none() {
            SCROLLBACK = Some(Vec::new());
            SCROLLBACK_POS = 0;
        }

        let sb = SCROLLBACK.as_mut().unwrap();

        // Append the raw byte line to scrollback. We'll perform wrapping when
        // redrawing the viewport.
        sb.push(bytes.to_vec());
        if sb.len() > SCROLLBACK_MAX_LINES {
            // drop oldest
            sb.remove(0);
            if SCROLLBACK_POS > 0 { SCROLLBACK_POS = SCROLLBACK_POS.saturating_sub(1); }
        }

        // If the user is at the bottom (showing latest), keep viewport pinned to bottom
        let visible_lines_est = (TERM_H / CH_H).max(1) as usize;
        if sb.len() <= visible_lines_est || SCROLLBACK_POS + visible_lines_est >= sb.len() {
            // show last page
            SCROLLBACK_POS = sb.len().saturating_sub(visible_lines_est);
        }

        // Redraw the terminal viewport from scrollback
        redraw_terminal_viewport();
    }
}

// Force append and scroll viewport to bottom (used when a command is entered)
fn print_line_force(bytes: &[u8], fg: u32) {
    if !gui::shell_is_visible() { return; }
    unsafe {
        if SCROLLBACK.is_none() {
            SCROLLBACK = Some(Vec::new());
            SCROLLBACK_POS = 0;
        }
        let sb = SCROLLBACK.as_mut().unwrap();
        sb.push(bytes.to_vec());
        if sb.len() > SCROLLBACK_MAX_LINES {
            sb.remove(0);
        }
        let visible_lines_est = (TERM_H / CH_H).max(1) as usize;
        SCROLLBACK_POS = sb.len().saturating_sub(visible_lines_est);
        // Reset user-scrolled flag since we pinned to bottom programmatically.
        SCROLLBACK_USER_SCROLLED = false;
        redraw_terminal_viewport();
    }
}

// Ensure the scrollback viewport is pinned to the bottom. Return true if we
// moved the viewport (caller may want to avoid redundant draws).
fn ensure_scrollback_at_bottom(force: bool) -> bool {
    unsafe {
        if SCROLLBACK.is_none() { return false; }
        // If user has scrolled and we are not forcing, do nothing.
        if SCROLLBACK_USER_SCROLLED && !force { return false; }
        let sb = SCROLLBACK.as_ref().unwrap();
        let visible_lines_est = (TERM_H / CH_H).max(1) as usize;
        let new_pos = sb.len().saturating_sub(visible_lines_est);
        if SCROLLBACK_POS != new_pos {
            SCROLLBACK_POS = new_pos;
            // If we are forcing, clear the user-scrolled flag.
            if force { SCROLLBACK_USER_SCROLLED = false; }
            return true;
        }
        return false;
    }
}

// Redraw terminal viewport from scrollback starting at SCROLLBACK_POS.
fn redraw_terminal_viewport_nocursor() {
    if !gui::shell_is_visible() { return; }
    unsafe {
        // Clear terminal area
        clear_rect(TERM_X, TERM_Y0, TERM_W, TERM_H, BG);

        // Draw lines from scrollback starting at SCROLLBACK_POS (nocursor drawing)
        if let Some(sb) = &SCROLLBACK {
            let mut y = TERM_Y0;
            for i in SCROLLBACK_POS..sb.len() {
                if y > TERM_Y0 + TERM_H - CH_H { break; }
                // draw_bytes_line_nocursor returns number of wrapped lines written
                let lines = draw_bytes_line_nocursor(TERM_X, y, &sb[i], FG, BG);
                y += (lines as i32) * CH_H;
            }
            TERM_Y = y;
        } else {
            TERM_Y = TERM_Y0;
        }

        // Draw status/footer without touching cursor state
        draw_status_bar_nocursor();
        print_prompt_and_input_nocursor();
    }
}

fn redraw_terminal_viewport() {
    gui::begin_paint();
    redraw_terminal_viewport_nocursor();
    gui::end_paint();
}

fn draw_status_bar_nocursor() {
    if !gui::shell_is_visible() { return; }
    let x = gui::shell_content_left();
    let y = gui::shell_content_top();
    let w = gui::shell_content_w();
    if w <= 0 { return; }
    // Clear status background
    clear_rect(x, y, w, STATUS_H, 0x0B1220);

    // Build status line similar to draw_status_bar()
    let mut line = [0u8; 64];
    let mut n = 0usize;
    let base = b"Terminal";
    line[..base.len()].copy_from_slice(base);
    n += base.len();
    if login::is_logged_in() {
        let sep = b" - ";
        if n + sep.len() < line.len() {
            line[n..n + sep.len()].copy_from_slice(sep);
            n += sep.len();
        }
        let u = login::current_user_bytes();
        let take = u.len().min(line.len().saturating_sub(n));
        line[n..n + take].copy_from_slice(&u[..take]);
        n += take;
    } else {
        let s = b" (locked)";
        let take = s.len().min(line.len().saturating_sub(n));
        line[n..n + take].copy_from_slice(&s[..take]);
        n += take;
    }

    draw_bytes_line_clip_nocursor(x + PAD, y + 2, &line[..n], 0xE5E7EB, 0x0B1220, x + w - PAD);
}

fn print_prompt_and_input_nocursor() {
    if !gui::shell_is_visible() { return; }
    // NOTE: Do NOT auto-pin scrollback here. This nocursor variant is used
    // during full-viewport redraws (for example from mouse wheel). If we
    // auto-pinned here we'd immediately undo any scroll adjustments made by
    // the wheel. The paint-wrapped `print_prompt_and_input()` handles
    // pinning when appropriate (typing/enter).

    let fx = gui::shell_footer_x();
    let fy = gui::shell_footer_y();
    let fw = gui::shell_footer_w();
    let fh = gui::shell_footer_h();
    if fw <= 0 || fh <= 0 { return; }

    // Clear footer
    clear_rect(fx, fy, fw, fh, BG);

    // prompt: "<dir> $ "
    let cwd = crate::fs_cmds::cwd();
    let name: &str = if cwd.is_empty() { "/" } else { cwd.as_str() };

    // Draw prompt at (fx+6, fy+1)
    let mut px = fx + 6;
    let max_name_chars: usize = 20;
    let name_bytes = name.as_bytes();
    let start = if name_bytes.len() > max_name_chars { name_bytes.len() - max_name_chars } else { 0 };
    for &b in &name_bytes[start..] {
        gui::draw_byte_nocursor(px, fy + 1, b, DIM, BG);
        px += CH_W;
    }
    gui::draw_byte_nocursor(px, fy + 1, b' ', DIM, BG); px += CH_W;
    gui::draw_byte_nocursor(px, fy + 1, b'$', DIM, BG); px += CH_W;
    gui::draw_byte_nocursor(px, fy + 1, b' ', DIM, BG); px += CH_W;

    // input
    let start_x = px;
    unsafe {
        let bytes = &INBUF[..INLEN];
        let clip_r = fx + fw - 4;
        draw_bytes_line_clip_nocursor(start_x, fy + 1, bytes, FG, BG, clip_r);

        // caret
        if CARET_ON {
            let cx = start_x + (CARET as i32) * CH_W;
            if cx < fx + fw - 2 {
                clear_rect(cx, fy + 2, 2, CH_H - 3, FG);
            }
        }
    }
}

/// Scroll the terminal view by `delta` wheel-notches. Positive delta scrolls
/// up (move content down), negative scrolls down (move content up).
pub fn shell_mouse_wheel(delta: i8) {
    if delta == 0 { return; }
    if !crate::gui::shell_is_visible() { return; }
    unsafe {
        // Use GUI paint wrappers to avoid cursor artifacts (they call cursor_restore()/cursor_redraw()).
        gui::begin_paint();

        // delta units -> pixel delta (1 notch == one text line height)
        let mut dp = (delta as i32) * CH_H;

        // Clamp dp to reasonable bounds (so we don't clear whole region)
        let max_scroll = TERM_H - CH_H;
        if max_scroll <= 0 {
            gui::end_paint();
            return;
        }
        dp = dp.clamp(-max_scroll, max_scroll);

        if dp > 0 {
            // Scroll up: move visible block down by dp to reveal top area
            let move_h = (TERM_H - dp).max(0);
            if move_h > 0 {
                fb::blit_move_rect(TERM_X, TERM_Y0, TERM_W, move_h, TERM_X, TERM_Y0 + dp);
            }
            // Clear newly revealed top area
            clear_rect(TERM_X, TERM_Y0, TERM_W, dp, BG);
            TERM_Y += dp;
        } else {
            let ad = (-dp) as i32;
            let move_h = (TERM_H - ad).max(0);
            if move_h > 0 {
                fb::blit_move_rect(TERM_X, TERM_Y0 + ad, TERM_W, move_h, TERM_X, TERM_Y0);
            }
            // Clear newly revealed bottom area
            clear_rect(TERM_X, TERM_Y0 + TERM_H - ad, TERM_W, ad, BG);
            TERM_Y -= ad;
        }

        // Clamp TERM_Y so the next printed line remains in visible region
        let miny = TERM_Y0;
        let maxy = TERM_Y0 + TERM_H - CH_H;
        if TERM_Y < miny { TERM_Y = miny; }
        if TERM_Y > maxy { TERM_Y = maxy; }

        // If we are using scrollback, adjust scroll position and redraw.
        if SCROLLBACK.is_some() {
            let sb = SCROLLBACK.as_ref().unwrap();
            let visible_lines = (TERM_H / CH_H).max(1) as usize;
            let max_start = if sb.len() > visible_lines { sb.len() - visible_lines } else { 0 };
            if delta > 0 {
                // scroll up -> decrease start index
                let dec = delta as usize;
                SCROLLBACK_POS = SCROLLBACK_POS.saturating_sub(dec);
                SCROLLBACK_USER_SCROLLED = true;
            } else if delta < 0 {
                let inc = (-delta) as usize;
                SCROLLBACK_POS = (SCROLLBACK_POS + inc).min(max_start);
                SCROLLBACK_USER_SCROLLED = true;
            }
            redraw_terminal_viewport_nocursor();
        } else {
            // Redraw status/footer since scrolling can change caret/visible lines
            draw_status_bar_nocursor();
            print_prompt_and_input_nocursor();
        }

        gui::end_paint();
    }
}


fn print_str_lines(s: &str, fg: u32) {
    if s.is_empty() { return; }
    for line in s.split('\n') {
        if line.is_empty() { continue; }
        print_line(line.as_bytes(), fg);
    }
}

fn print_prompt_and_input() {
    if !gui::shell_is_visible() { return; }

    let fx = gui::shell_footer_x();
    let fy = gui::shell_footer_y();
    let fw = gui::shell_footer_w();
    let fh = gui::shell_footer_h();
    if fw <= 0 || fh <= 0 { return; }

    // For normal redraws do not force pin to bottom; callers that want to
    // ensure the viewport is at the live bottom should call the
    // `print_prompt_and_input_force()` helper.

    // Clear footer
    gui::begin_paint();
    clear_rect(fx, fy, fw, fh, BG);

    // prompt: "<dir> $ "
    let cwd = crate::fs_cmds::cwd();
    // prompt: full current path (e.g. /bin/test)
    let name: &str = if cwd.is_empty() { "/" } else { cwd.as_str() };

    // Draw prompt at (fx+6, fy+1)
    let mut px = fx + 6;
    // Keep prompt reasonably short to avoid eating the whole line
    let max_name_chars: usize = 20;
    let name_bytes = name.as_bytes();
    let start = if name_bytes.len() > max_name_chars { name_bytes.len() - max_name_chars } else { 0 };
    for &b in &name_bytes[start..] {
        gui::draw_byte_nocursor(px, fy + 1, b, DIM, BG);
        px += CH_W;
    }
    gui::draw_byte_nocursor(px, fy + 1, b' ', DIM, BG); px += CH_W;
    gui::draw_byte_nocursor(px, fy + 1, b'$', DIM, BG); px += CH_W;
    gui::draw_byte_nocursor(px, fy + 1, b' ', DIM, BG); px += CH_W;

    // input
    let start_x = px;
    unsafe {
        let bytes = &INBUF[..INLEN];
        let clip_r = fx + fw - 4;
        draw_bytes_line_clip_nocursor(start_x, fy + 1, bytes, FG, BG, clip_r);

        // caret
        if CARET_ON {
            let cx = start_x + (CARET as i32) * CH_W;
            if cx < fx + fw - 2 {
                clear_rect(cx, fy + 2, 2, CH_H - 3, FG);
            }
        }
    }

    gui::end_paint();
}

fn buf_insert(ch: u8) {
    unsafe {
        if INLEN >= INBUF.len().saturating_sub(1) { return; }
        if CARET > INLEN { CARET = INLEN; }
        for i in (CARET..INLEN).rev() {
            INBUF[i + 1] = INBUF[i];
        }
        INBUF[CARET] = ch;
        INLEN += 1;
        CARET += 1;
    }
}

fn buf_backspace() {
    unsafe {
        if CARET == 0 || INLEN == 0 { return; }
        for i in CARET..INLEN {
            INBUF[i - 1] = INBUF[i];
        }
        INLEN -= 1;
        CARET -= 1;
    }
}

fn buf_delete() {
    unsafe {
        if CARET >= INLEN { return; }
        for i in (CARET + 1)..INLEN {
            INBUF[i - 1] = INBUF[i];
        }
        INLEN -= 1;
    }
}

/// Forceful prompt draw: ensure scrollback is pinned to bottom, redraw
/// viewport if necessary, else draw footer/prompt. Use this when the user
/// types or after a command has executed.
fn print_prompt_and_input_force() {
    if !gui::shell_is_visible() { return; }
    // If we moved the viewport to bottom, redraw whole viewport under paint.
    if ensure_scrollback_at_bottom(true) {
        redraw_terminal_viewport();
        return;
    }
    // Otherwise draw footer normally.
    print_prompt_and_input();
}

fn exec_command(line: &[u8]) -> Option<AppState> {
    // trim leading spaces
    let mut i = 0;
    while i < line.len() && line[i] == b' ' { i += 1; }
    let line = &line[i..];
    if line.is_empty() { return None; }

    // Split cmd + arg (first space)
    let mut sp = 0;
    while sp < line.len() && line[sp] != b' ' { sp += 1; }
    let cmd = &line[..sp];
    let mut arg = &line[sp..];
    while !arg.is_empty() && arg[0] == b' ' { arg = &arg[1..]; }

    // Try filesystem / persistence commands first:
    // pwd, cd, ls, cat, mkdir, touch, rm, write, append, sync, persist
    if let (Ok(cmd_s), Ok(arg_s)) = (core::str::from_utf8(cmd), core::str::from_utf8(arg)) {
        let mut argv: [&str; 16] = [""; 16];
        let mut argc = 0usize;
        for tok in arg_s.split_whitespace() {
            if argc >= argv.len() { break; }
            argv[argc] = tok;
            argc += 1;
        }
        if let Some(out) = crate::fs_cmds::try_handle(cmd_s, &argv[..argc]) {
            if !out.is_empty() {
                print_str_lines(&out, FG);
            }
            return None;
        }
        // Text editor: edit <path>
        if cmd_s == "edit" || cmd_s == "notepad" {
            let path = if argc > 0 { argv[0] } else { "/home/user/readme.txt" };
            let cwd = crate::fs_cmds::cwd();
            match fs::normalize_path(&cwd, path) {
                Ok(abs) => {
                    editor::open_abs(&abs);
                    return Some(AppState::Editor);
                }
                Err(_) => {
                    print_line(b"edit: invalid path", ERR);
                    return None;
                }
            }
        }
    }

    match cmd {
        b"help" => {
            print_line(b"Commands: help, clear, net, ipconfig, dhcp, ipset, ping, about, login, reg, edit, tsc, echo <text>, pwd, cd, ls, cat, mkdir, touch, rm, write, append, sync, persist", DIM);
            print_line(b"Tips: click the dock 'T' to hide/show the shell.", DIM);
            print_line(b"      click traffic lights to close/min/max.", DIM);
            None
        }
        b"clear" => {
            term_clear();
            None
        }
        b"net" => {
            // Initialize NIC (RTL8139) if needed, then list detected adapters.
            net::init();
            let r = net::net_scan();
            if r.devices.is_empty() {
                print_line(b"No network adapters detected.", ERR);
            } else {
                print_line(b"Network adapters:", OK);
                for &d in r.devices {
                    print_line(d.as_bytes(), FG);
                }
            }
            None
        }
        b"ipconfig" | b"ifconfig" => {
            cmd_ipconfig();
            None
        }
        b"dhcp" => {
            net::init();
            match net::dhcp_acquire() {
                Ok(()) => print_line(b"DHCP: bound", OK),
                Err(net::DhcpError::NoNic) => print_line(b"DHCP: no NIC detected", ERR),
                Err(net::DhcpError::Timeout) => print_line(b"DHCP: timeout (no offer/ack)", ERR),
                Err(net::DhcpError::Malformed) => print_line(b"DHCP: malformed reply", ERR),
                Err(net::DhcpError::Nack) => print_line(b"DHCP: NACK (request denied)", ERR),
            }
            None
        }
        b"ipset" => {
            cmd_ipset(arg);
            None
        }
        b"ping" => {
            cmd_ping(arg);
            None
        }
        b"about" => {
            print_line(b"", FG);
            print_line(b"Othello OS", OK);
            print_line(b"", FG);
            print_line(b"UEFI Version 1.0.1", FG);
            print_line(b"Rust Kernel Version 1.2.1", FG);
            print_line(b"", FG);
            print_line(b"Developed by Simon Hamilton.", OK);
            None
        }
        b"login" => {
            // Lock and return to the login screen.
            login::lock();
            Some(AppState::Login)
        }
        b"reg" => Some(AppState::Regedit),
        b"tsc" => {
            let t = time::rdtsc();
            // Build "TSC: <num>" into a small stack buffer.
            let mut buf = [0u8; 48];
            let mut n = 0usize;
            buf[n..n + 5].copy_from_slice(b"TSC: ");
            n += 5;
            n += write_u64_dec(&mut buf[n..], t);
            print_line(&buf[..n], FG);
            None
        }
        b"echo" => {
            if arg.is_empty() { print_line(b"(echo) missing text", ERR); }
            else { print_line(arg, FG); }
            None
        }
        _ => {
            print_line(b"Unknown command. Try: help", ERR);
            None
        }
    }
}

fn write_u64_dec(out: &mut [u8], mut v: u64) -> usize {
    let mut tmp = [0u8; 20];
    let mut n = 0usize;
    if v == 0 { if !out.is_empty() { out[0] = b'0'; return 1; } return 0; }
    while v > 0 && n < tmp.len() {
        tmp[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
    }
    let mut w = 0usize;
    while n > 0 && w < out.len() {
        n -= 1;
        out[w] = tmp[n];
        w += 1;
    }
    w
}


// -----------------------------------------------------------------------------
// Networking commands (ipconfig / dhcp / ipset / ping)
// -----------------------------------------------------------------------------

fn write_u8_dec(out: &mut [u8], v: u8) -> usize {
    if out.is_empty() { return 0; }
    if v >= 100 {
        if out.len() < 3 { return 0; }
        out[0] = b'0' + (v / 100);
        out[1] = b'0' + ((v / 10) % 10);
        out[2] = b'0' + (v % 10);
        3
    } else if v >= 10 {
        if out.len() < 2 { return 0; }
        out[0] = b'0' + (v / 10);
        out[1] = b'0' + (v % 10);
        2
    } else {
        out[0] = b'0' + v;
        1
    }
}

fn write_ipv4(out: &mut [u8], ip: [u8; 4]) -> usize {
    let mut n = 0usize;
    for (i, oct) in ip.iter().enumerate() {
        if i != 0 {
            if n >= out.len() { break; }
            out[n] = b'.';
            n += 1;
        }
        n += write_u8_dec(&mut out[n..], *oct);
        if n >= out.len() { break; }
    }
    n
}

fn write_hex2(out: &mut [u8], v: u8) -> usize {
    if out.len() < 2 { return 0; }
    let hi = (v >> 4) & 0xF;
    let lo = v & 0xF;
    out[0] = if hi < 10 { b'0' + hi } else { b'a' + (hi - 10) };
    out[1] = if lo < 10 { b'0' + lo } else { b'a' + (lo - 10) };
    2
}

fn write_mac(out: &mut [u8], mac: [u8; 6]) -> usize {
    let mut n = 0usize;
    for (i, b) in mac.iter().enumerate() {
        if i != 0 {
            if n >= out.len() { break; }
            out[n] = b'-';
            n += 1;
        }
        if n + 2 > out.len() { break; }
        n += write_hex2(&mut out[n..], *b);
    }
    n
}

fn parse_ipv4_str(s: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut idx = 0usize;
    for part in s.split('.') {
        if idx >= 4 { return None; }
        if part.is_empty() { return None; }
        let mut v: u16 = 0;
        for c in part.bytes() {
            if c < b'0' || c > b'9' { return None; }
            v = v * 10 + (c - b'0') as u16;
            if v > 255 { return None; }
        }
        out[idx] = v as u8;
        idx += 1;
    }
    if idx != 4 { return None; }
    Some(out)
}

fn print_kv(prefix: &[u8], value: &[u8], fg: u32) {
    let mut buf = [0u8; 192];
    let mut n = 0usize;
    let take_p = prefix.len().min(buf.len());
    buf[..take_p].copy_from_slice(&prefix[..take_p]);
    n += take_p;
    let take_v = value.len().min(buf.len().saturating_sub(n));
    buf[n..n + take_v].copy_from_slice(&value[..take_v]);
    n += take_v;
    print_line(&buf[..n], fg);
}

fn print_kv_ipv4(prefix: &[u8], ip: [u8; 4], fg: u32) {
    let mut buf = [0u8; 192];
    let mut n = 0usize;
    let take_p = prefix.len().min(buf.len());
    buf[..take_p].copy_from_slice(&prefix[..take_p]);
    n += take_p;
    n += write_ipv4(&mut buf[n..], ip);
    print_line(&buf[..n], fg);
}

fn cmd_ipconfig() {
    print_line(b"", FG);

    let r = net::net_scan();
    if !r.devices.is_empty() {
        print_line(b"", FG);
        print_line(b"Adapters:", DIM);
        for &d in r.devices {
            print_line(d.as_bytes(), FG);
        }
    }

    let cfg = net::config();
    print_line(b"", FG);
    print_kv(b"   DHCP Enabled . . . . . . . . . . : ", if cfg.dhcp_bound { b"Yes" } else { b"No" }, FG);
    print_kv_ipv4(b"   IPv4 Address. . . . . . . . . . . : ", cfg.ip, FG);
    print_kv_ipv4(b"   Subnet Mask . . . . . . . . . . . : ", cfg.mask, FG);
    print_kv_ipv4(b"   Default Gateway . . . . . . . . . : ", cfg.gateway, FG);
    print_kv_ipv4(b"   DNS Servers . . . . . . . . . . . : ", cfg.dns, FG);
    if cfg.dhcp_bound {
        print_kv_ipv4(b"   DHCP Server . . . . . . . . . . . : ", cfg.server_id, FG);
    }

    if let Some(mac) = net::mac() {
        let mut line = [0u8; 80];
        let mut n = 0usize;
        let key = b"   Physical Address. . . . . . . . . : ";
        line[n..n + key.len()].copy_from_slice(key);
        n += key.len();
        n += write_mac(&mut line[n..], mac);
        print_line(&line[..n], FG);
    }
}

fn cmd_ipset(arg: &[u8]) {
    let arg_s = match core::str::from_utf8(arg) {
        Ok(s) => s,
        Err(_) => { print_line(b"ipset: invalid UTF-8 args", ERR); return; }
    };

    let mut it = arg_s.split_whitespace();
    let first = match it.next() {
        Some(s) => s,
        None => {
            print_line(b"Usage: ipset <ip> <mask> <gw> [dns]", DIM);
            print_line(b"   or: ipset qemu", DIM);
            return;
        }
    };

    if first == "qemu" {
        // QEMU user-mode NAT defaults: IP 10.0.2.15/24, GW 10.0.2.2, DNS 10.0.2.3
        net::set_static_config([10,0,2,15], [255,255,255,0], [10,0,2,2], [10,0,2,3]);
        print_line(b"ipset: qemu defaults applied", OK);
        return;
    }

    let mask_s = it.next();
    let gw_s = it.next();
    let dns_s = it.next();

    if mask_s.is_none() || gw_s.is_none() {
        print_line(b"Usage: ipset <ip> <mask> <gw> [dns]", DIM);
        print_line(b"Example: ipset 10.0.2.15 255.255.255.0 10.0.2.2 10.0.2.3", DIM);
        return;
    }

    let ip = match parse_ipv4_str(first) { Some(v) => v, None => { print_line(b"ipset: invalid IP", ERR); return; } };
    let mask = match parse_ipv4_str(mask_s.unwrap()) { Some(v) => v, None => { print_line(b"ipset: invalid mask", ERR); return; } };
    let gw = match parse_ipv4_str(gw_s.unwrap()) { Some(v) => v, None => { print_line(b"ipset: invalid gateway", ERR); return; } };
    let dns = if let Some(d) = dns_s {
        match parse_ipv4_str(d) { Some(v) => v, None => { print_line(b"ipset: invalid dns", ERR); return; } }
    } else {
        [0,0,0,0]
    };

    net::set_static_config(ip, mask, gw, dns);
    print_line(b"ipset: ok", OK);
}

fn cmd_ping(arg: &[u8]) {
    let arg_s = match core::str::from_utf8(arg) {
        Ok(s) => s,
        Err(_) => { print_line(b"ping: invalid UTF-8 args", ERR); return; }
    };

    let mut it = arg_s.split_whitespace();
    let ip_s = match it.next() {
        Some(s) => s,
        None => { print_line(b"Usage: ping <ip> [count]", DIM); return; }
    };

    let dst = match parse_ipv4_str(ip_s) {
        Some(v) => v,
        None => { print_line(b"ping: invalid IP", ERR); return; }
    };

    let count = it.next().and_then(|c| c.parse::<u32>().ok()).unwrap_or(4).min(20);

    let mut banner = [0u8; 64];
    let mut n = 0usize;
    banner[n..n+5].copy_from_slice(b"PING ");
    n += 5;
    n += write_ipv4(&mut banner[n..], dst);
    print_line(&banner[..n], OK);

    let mut seq: u16 = 1;
    for _ in 0..count {
        match net::ping_once(dst, seq) {
            Ok(r) => {
                let mut line = [0u8; 96];
                let mut p = 0usize;
                line[p..p+11].copy_from_slice(b"Reply from ");
                p += 11;
                p += write_ipv4(&mut line[p..], dst);

                line[p..p+6].copy_from_slice(b": seq=");
                p += 6;
                p += write_u64_dec(&mut line[p..], r.seq as u64);

                line[p..p+5].copy_from_slice(b" ttl=");
                p += 5;
                p += write_u64_dec(&mut line[p..], r.ttl as u64);

                line[p..p+5].copy_from_slice(b" tsc=");
                p += 5;
                p += write_u64_dec(&mut line[p..], r.rtt_tsc);

                print_line(&line[..p], FG);
            }
            Err(net::PingError::Timeout) => {
                print_line(b"Request timed out.", ERR);
            }
            Err(net::PingError::NotConfigured) => {
                print_line(b"ping: no IPv4 config (run dhcp or ipset)", ERR);
                return;
            }
            Err(net::PingError::ArpTimeout) => {
                print_line(b"ping: ARP timeout", ERR);
            }
            Err(_) => {
                print_line(b"ping: failed", ERR);
            }
        }
        seq = seq.wrapping_add(1);
        time::spin(2_000_000);
    }
}

fn render_terminal_full() {
    // Full repaint of terminal view (frame + status + terminal area + footer).
    gui::clear_shell_content_and_frame();
    layout();
    draw_status_bar();
    // Small banner.
    if login::is_logged_in() {
        print_line(b"Othello Terminal", OK);
    } else {
        print_line(b"Othello Terminal (locked)", ERR);
    }
    print_line(b"Type 'help'", DIM);
    print_prompt_and_input();
}

pub fn run_shell() -> ! {
    serial_write_str("KERNEL: shell loop started.\n");

    // Initial paint: boot into full-screen login screen
    unsafe { APP = AppState::Login; }
    gui::set_ui_mode(gui::UiMode::Login);
    gui::set_shell_visible(false);
    login::reset();
    login::render_fullscreen();

    let mut shift = false;
    let mut ctrl = false;
    let mut ext = false;
    let mut last_tsc = time::rdtsc();
    let mut was_dragging = false;
    let mut last_clock_sec: u8 = 0xFF;

    loop {
        // Process all mouse packets (cursor + UI)
        let max_w = fb::width() as i32;
        let max_h = fb::height() as i32;
        while let Some(ms) = mouse::mouse_poll(max_w, max_h) {
            let act = gui::ui_handle_mouse(ms);
            // Mouse wheel: route to active app if cursor is inside the shell content region.
            if ms.wheel != 0 {
                unsafe {
                    if APP == AppState::Browser && gui::point_in_shell_content(ms.x, ms.y) && gui::shell_is_visible() {
                        if crate::browser::handle_wheel(ms.wheel as i32) {
                            crate::browser::render();
                        }
                    }
                }
            }

            match act {
                gui::UiAction::ShellMoved => {
                    // Window moved via blit. Only terminal needs geometry sync.
                    unsafe {
                        if APP == AppState::Terminal {
                            sync_layout_preserve();
                        }
                    }
                }
                gui::UiAction::ShellVisibilityChanged => {
                    if gui::shell_is_visible() {
                        render_active_full();
                    }
                }
                gui::UiAction::ShellResized => {
                    if gui::shell_is_visible() {
                        render_active_full();
                    }
                }
                gui::UiAction::DockLaunch(icon) => {
                    match icon {
                        0 => {
                            // Leftmost dock icon: behave like a real Terminal launcher.
                            // - If Terminal is already active and visible -> minimize the shell.
                            // - Otherwise -> show shell and switch to Terminal.
                            unsafe {
                                let terminal_active = APP == AppState::Terminal;
                                if gui::shell_is_visible() && terminal_active {
                                    gui::set_shell_visible(false);
                                    gui::redraw_all();
                                } else {
                                    if !gui::shell_is_visible() {
                                        gui::set_shell_visible(true);
                                        gui::redraw_all();
                                    }
                                    set_app(AppState::Terminal);
                                    render_active_full();
                                }
                            }
                        }
                        _ => {
                            // For all other dock icons, ensure the shell is visible.
                            if !gui::shell_is_visible() {
                                gui::set_shell_visible(true);
                                gui::redraw_all();
                            }
                            match icon {
                                1 => {
                                    // Network: show in terminal
                                    set_app(AppState::Terminal);
                                    print_line(b"[dock] net", DIM);
                                    let _ = exec_command(b"net");
                                    if !gui::shell_is_dragging() { print_prompt_and_input(); }
                                }
                                2 => {
                                    // Lock/Login
                                    login::lock();
                                    set_app(AppState::Login);
                                }
                                3 | 6 => {
                                    // Browser (taskbar web icon)
                                    set_app(AppState::Browser);
                                }
                                4 => {
                                    // Text Editor: open /home/user/readme.txt
                                    let cwd = crate::fs_cmds::cwd();
                                    if let Ok(p) = fs::normalize_path(&cwd, "/home/user/readme.txt") {
                                        editor::open_abs(&p);
                                        set_app(AppState::Editor);
                                    }
                                }
                                5 => {
                                    // Registry
                                    set_app(AppState::Regedit);
                                }
                                _ => {}
                            }
                        }
                    }
                }

                _ => {}
            }
        }

        // Track drag transitions so we don't draw using stale cached layout.
        let dragging = gui::shell_is_dragging();
        if was_dragging && !dragging {
            // Drag ended: for terminal, keep cached layout in sync.
            unsafe {
                if APP == AppState::Terminal {
                    sync_layout_preserve();
                    if gui::shell_is_visible() && !dragging {
                        print_prompt_and_input();
                    }
                } else if gui::shell_is_visible() {
                    // login/regedit: repaint to avoid artifacts
                    render_active_full();
                }
            }
        }
        was_dragging = dragging;

        // Always drain keyboard scancodes (even when the shell is hidden) so
        // keyboard bytes can't clog the PS/2 output buffer and block mouse input.
        while let Some(sc) = keyboard::keyboard_poll_scancode() {
            if sc == 0xE0 { ext = true; continue; }

            // shift make/break
            if sc == 0x2A || sc == 0x36 { shift = true; continue; }
            if sc == 0xAA || sc == 0xB6 { shift = false; continue; }
            // ctrl make/break
            if sc == 0x1D { ctrl = true; ext = false; continue; }
            if sc == 0x9D { ctrl = false; ext = false; continue; }

            // If we're on the desktop and the shell is hidden, still drain keyboard bytes.
            // The PS/2 controller uses a shared output buffer; leaving a keyboard byte
            // unread can block mouse packets and make the cursor appear frozen.
            if gui::ui_mode() == gui::UiMode::Desktop && !gui::shell_is_visible() {
                ext = false;
                continue;
            }

            if ext {
                ext = false;
                if sc & 0x80 != 0 { continue; }
                unsafe {
                    match APP {
                        AppState::Terminal => {
                            match sc {
                                0x4B => { if CARET > 0 { CARET -= 1; } }, // Left
                                0x4D => { if CARET < INLEN { CARET += 1; } }, // Right
                                0x47 => { CARET = 0; }, // Home
                                0x4F => { CARET = INLEN; }, // End
                                0x53 => { buf_delete(); }, // Delete
                                _ => {}
                            }
                            if !dragging { print_prompt_and_input(); }
                        }
                        AppState::Login => {
                            if login::handle_ext_scancode(sc) {
                                login::render_fullscreen();
                            }
                        }
                        AppState::Regedit => {
                            if regedit::handle_ext_scancode(sc) {
                                regedit::render();
                            }
                        }
                        AppState::Editor => {
                            if let editor::EditorAction::Redraw = editor::handle_ext_scancode(sc, ctrl) {
                                editor::render();
                            }
                        }
                                            AppState::Browser => {
                            if crate::browser::handle_ext_scancode(sc) {
                                crate::browser::render();
                            }
                        }
}
                }
                continue;
            }

            if let Some(ch) = keyboard::scancode_to_ascii(sc, shift) {
                unsafe {
                    match APP {
                        AppState::Login => {
                            let (dirty, outcome) = login::handle_ascii(ch);
                            if dirty { login::render_fullscreen(); }
                            if let login::LoginOutcome::Success = outcome {
                                // Successful auth -> desktop terminal
                                set_app(AppState::Terminal);
                            }
                        }
                        AppState::Regedit => {
                            if regedit::handle_ascii(ch) {
                                regedit::render();
                            }
                        }
                                                AppState::Editor => {
                            match editor::handle_char(ch, ctrl) {
                                editor::EditorAction::None => {}
                                editor::EditorAction::Redraw => editor::render(),
                                editor::EditorAction::Save => {
                                    let ok = editor::save();
                                    editor::set_status_saved(ok);
                                    editor::render();
                                }
                                editor::EditorAction::Exit => {
                                    editor::close();
                                    set_app(AppState::Terminal);
                                }
                            }
                        }
                                                AppState::Browser => {
                            if crate::browser::handle_char(ch, ctrl) {
                                crate::browser::render();
                            }
                        }
AppState::Terminal => match ch {
                    b'\n' => {
                        // Print the entered command as a terminal line and execute.
                        let req = unsafe {
                            let mut line = [0u8; 192];
                            let mut n = 0usize;
                            // include current dir in the echoed prompt: "<dir> $ "
                            {
                                let cwd = crate::fs_cmds::cwd();
    // prompt: full current path (e.g. /bin/test)
    let name: &str = if cwd.is_empty() { "/" } else { cwd.as_str() };

                                let max_name_chars: usize = 20;
                                let nb = name.as_bytes();
                                let start = if nb.len() > max_name_chars { nb.len() - max_name_chars } else { 0 };
                                for &b in &nb[start..] {
                                    if n >= line.len() { break; }
                                    line[n] = b;
                                    n += 1;
                                }
                                if n + 3 <= line.len() {
                                    line[n] = b' '; line[n + 1] = b'$'; line[n + 2] = b' ';
                                    n += 3;
                                }
                            }
                            let take = INLEN.min(line.len().saturating_sub(n));
                            line[n..n + take].copy_from_slice(&INBUF[..take]);
                            n += take;
                            // When a command is entered, force the terminal to
                            // scroll to the bottom so the echoed command and
                            // subsequent output are visible.
                            print_line_force(&line[..n], FG);

                            let req = exec_command(&INBUF[..INLEN]);
                            INLEN = 0;
                            CARET = 0;
                            req
                        };

                        if let Some(next) = req {
                            set_app(next);
                        } else if !gui::shell_is_dragging() {
                            print_prompt_and_input_force();
                        }
                    }
                    0x08 => { // backspace
                        buf_backspace();
                        if !gui::shell_is_dragging() { print_prompt_and_input_force(); }
                    }
                    b'\t' => {
                        // For now: tab inserts spaces (simple)
                        buf_insert(b' ');
                        buf_insert(b' ');
                        if !gui::shell_is_dragging() { print_prompt_and_input_force(); }
                    }
                    _ => {
                        if ch >= 0x20 && ch <= 0x7E {
                            buf_insert(ch);
                            if !gui::shell_is_dragging() { print_prompt_and_input_force(); }
                        }
                    }
                },
                    }
                }
            }
        }

        // caret blink
        // Caret blink (terminal only)
        unsafe {
            if APP == AppState::Terminal {
                let now = time::rdtsc();
                if !dragging && now.wrapping_sub(last_tsc) > 1_500_000_000 { // 160_000_000
                    last_tsc = now;
                    CARET_ON = !CARET_ON;
                    if gui::shell_is_visible() && !dragging {
                        print_prompt_and_input();
                    }
                }
            }
        }


// Update on-screen clock once per RTC second.
// - Login: full-screen re-render (safe)
// - Desktop: redraw taskbar only (won't erase window contents)
{
    let dt = time::rtc_now();
    if dt.second != last_clock_sec {
        last_clock_sec = dt.second;
        if gui::ui_mode() == gui::UiMode::Login {
            login::render_fullscreen();
        } else {
            gui::redraw_taskbar();
        }
    }
}

        unsafe { asm!("pause", options(nomem, nostack, preserves_flags)); }
    }
}
