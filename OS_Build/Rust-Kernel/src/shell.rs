#![allow(dead_code)]

use core::arch::asm;

use crate::{framebuffer_driver as fb, gui, keyboard, login, mouse, net, regedit, editor, fs, time};
use crate::serial_write_str;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppState {
    Login,
    Terminal,
    Regedit,
    Editor,
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
        }
    }
}

static mut APP: AppState = AppState::Login;

fn set_app(app: AppState) {
    // Don't allow entering terminal/regedit without an authenticated session.
    if (app == AppState::Terminal || app == AppState::Regedit || app == AppState::Editor) && !login::is_logged_in() {
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

fn draw_bytes_line_clip_nocursor(x: i32, y: i32, bytes: &[u8], fg: u32, bg: u32, clip_r: i32) {
    if bytes.is_empty() { return; }
    let mut cx = x;
    for &b in bytes {
        if b == b'\n' { break; }
        if cx + CH_W > clip_r { break; }
        if b >= 0x20 && b <= 0x7E {
            gui::draw_byte_nocursor(cx, y, b, fg, bg);
        }
        cx += CH_W;
    }
}

fn draw_bytes_line_nocursor(x: i32, y: i32, bytes: &[u8], fg: u32, bg: u32) {
    let clip_r = x + unsafe { TERM_W };
    draw_bytes_line_clip_nocursor(x, y, bytes, fg, bg, clip_r);
}

fn draw_bytes_line(x: i32, y: i32, bytes: &[u8], fg: u32, bg: u32) {
    gui::begin_paint();
    draw_bytes_line_nocursor(x, y, bytes, fg, bg);
    gui::end_paint();
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
        scroll_if_needed();
        draw_bytes_line(TERM_X, TERM_Y, bytes, fg, BG);
        TERM_Y += CH_H;
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
            print_line(b"Commands: help, clear, net, about, login, reg, tsc, echo <text>, pwd, cd, ls, cat, mkdir, touch, rm, write, append, sync, persist", DIM);
            print_line(b"Tips: click the dock 'T' to hide/show the shell.", DIM);
            print_line(b"      click traffic lights to close/min/max.", DIM);
            None
        }
        b"clear" => {
            term_clear();
            None
        }
        b"net" => {
            let r = net::net_scan();
            for &d in r.devices {
                print_line(d.as_bytes(), FG);
            }
            None
        }
        b"about" => {
            print_line(b"Othello OS - bare-metal Rust (WIP)", OK);
            print_line(b"GUI: framebuffer + PS/2 mouse/keyboard (polled)", DIM);
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
                    // Ensure shell is visible and then run a quick action.
                    if gui::shell_is_visible() {
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
                            3 => {
                                set_app(AppState::Terminal);
                                print_line(b"[dock] about", DIM);
                                let _ = exec_command(b"about");
                                if !gui::shell_is_dragging() { print_prompt_and_input(); }
                            }
                            4 => {
                                // Text Editor: open /home/user/readme.txt
                                let cwd = crate::fs_cmds::cwd();
                                if let Ok(p) = fs::normalize_path(&cwd, "/home/user/readme.txt") {
                                    editor::open_abs(&p);
                                } else {
                                    editor::open_abs("/home/user/readme.txt");
                                }
                                set_app(AppState::Editor);
                            }
                            5 => {
                                // Registry
                                set_app(AppState::Regedit);
                            }
                            _ => {}
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
                            print_line(&line[..n], FG);

                            let req = exec_command(&INBUF[..INLEN]);
                            INLEN = 0;
                            CARET = 0;
                            req
                        };

                        if let Some(next) = req {
                            set_app(next);
                        } else if !gui::shell_is_dragging() {
                            print_prompt_and_input();
                        }
                    }
                    0x08 => { // backspace
                        buf_backspace();
                        if !gui::shell_is_dragging() { print_prompt_and_input(); }
                    }
                    b'\t' => {
                        // For now: tab inserts spaces (simple)
                        buf_insert(b' ');
                        buf_insert(b' ');
                        if !gui::shell_is_dragging() { print_prompt_and_input(); }
                    }
                    _ => {
                        if ch >= 0x20 && ch <= 0x7E {
                            buf_insert(ch);
                            if !gui::shell_is_dragging() { print_prompt_and_input(); }
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
                if !dragging && now.wrapping_sub(last_tsc) > 160_000_000 {
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
