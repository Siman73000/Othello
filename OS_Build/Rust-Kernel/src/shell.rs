use core::str;

use crate::{keyboard_poll_scancode, scancode_to_ascii};
use crate::gui::{
    draw_char,
    shell_left, shell_top, shell_right, shell_bottom,
    clear_shell_area,
    FONT_W, FONT_H,
    SHELL_FG_COLOR,
};
use crate::net;

// How big the scrollback buffer is (in text lines / columns)
const MAX_ROWS: usize = 512;
const MAX_COLS: usize = 128;
const MAX_CMD_LEN: usize = 80;

const PROMPT: &str = "> ";

// Text ring buffer
static mut LINES: [[u8; MAX_COLS]; MAX_ROWS] = [[0; MAX_COLS]; MAX_ROWS];

// Logical total line count (can grow beyond MAX_ROWS, we mod by MAX_ROWS)
static mut TOTAL_LINES: usize = 0;
// Current logical line we are writing into
static mut CUR_LINE: usize = 0;
// Column within current logical line
static mut CUR_COL: usize = 0;
// How many lines we are scrolled *up* from the bottom.
// 0 = at bottom (live), >0 = viewing history.
static mut VIEW_OFFSET: usize = 0;

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

fn ring_index(line: usize) -> usize {
    line % MAX_ROWS
}

unsafe fn clear_line(idx: usize) {
    for c in 0..MAX_COLS {
        LINES[idx][c] = 0;
    }
}

fn visible_cols_rows() -> (usize, usize) {
    let left = shell_left();
    let top = shell_top();
    let right = shell_right();
    let bottom = shell_bottom();

    let shell_w = right.saturating_sub(left);
    let shell_h = bottom.saturating_sub(top);
    if shell_w == 0 || shell_h == 0 {
        return (0, 0);
    }

    let cols_pixels = shell_w / FONT_W;
    let line_h = FONT_H + 1;
    let rows_pixels = shell_h / line_h;

    let cols = core::cmp::min(cols_pixels, MAX_COLS);
    let rows = core::cmp::min(rows_pixels, MAX_ROWS);

    (cols, rows)
}

unsafe fn new_line() {
    TOTAL_LINES = TOTAL_LINES.saturating_add(1);
    CUR_LINE = TOTAL_LINES.saturating_sub(1);
    CUR_COL = 0;
    let idx = ring_index(CUR_LINE);
    clear_line(idx);

    // When writing, always follow the tail
    VIEW_OFFSET = 0;
}

unsafe fn print_str_internal(s: &str) {
    let (visible_cols, _) = visible_cols_rows();
    let target_cols = if visible_cols == 0 { MAX_COLS } else { visible_cols };

    for b in s.bytes() {
        if b == b'\n' {
            new_line();
            continue;
        }
        if CUR_COL >= target_cols {
            new_line();
        }
        let line_idx = ring_index(CUR_LINE);
        if CUR_COL < MAX_COLS {
            LINES[line_idx][CUR_COL] = b;
            CUR_COL += 1;
        }
    }
}

fn print_str(s: &str) {
    unsafe {
        print_str_internal(s);
    }
}

fn print_line(s: &str) {
    unsafe {
        print_str_internal(s);
        new_line();
    }
}

// -----------------------------------------------------------------------------
// Scrolling
// -----------------------------------------------------------------------------

fn scroll_up() {
    let (_, visible_rows) = visible_cols_rows();
    if visible_rows == 0 {
        return;
    }

    unsafe {
        let total = core::cmp::min(TOTAL_LINES, MAX_ROWS);
        if total <= visible_rows {
            VIEW_OFFSET = 0;
            return;
        }
        let max_offset = total - visible_rows;
        if VIEW_OFFSET < max_offset {
            VIEW_OFFSET += 1;
        }
    }
}

fn scroll_down() {
    unsafe {
        if VIEW_OFFSET > 0 {
            VIEW_OFFSET -= 1;
        }
    }
}

// -----------------------------------------------------------------------------
// Rendering
// -----------------------------------------------------------------------------

fn render(cmd_buf: &[u8; MAX_CMD_LEN], cmd_len: usize) {
    clear_shell_area();

    let left = shell_left();
    let top = shell_top();
    let right = shell_right();
    let bottom = shell_bottom();
    let shell_w = right.saturating_sub(left);
    let shell_h = bottom.saturating_sub(top);

    if shell_w == 0 || shell_h == 0 {
        return;
    }

    let line_h = FONT_H + 1;
    let (visible_cols, visible_rows) = visible_cols_rows();
    if visible_cols == 0 || visible_rows == 0 {
        return;
    }

    let mut total_lines;
    let mut view_offset;
    unsafe {
        total_lines = TOTAL_LINES;
        view_offset = VIEW_OFFSET;
    }

    // Clamp view_offset if buffer smaller than expected
    let max_history_lines = core::cmp::min(total_lines, MAX_ROWS);
    let max_offset = max_history_lines.saturating_sub(visible_rows);
    if view_offset > max_offset {
        view_offset = max_offset;
        unsafe {
            VIEW_OFFSET = view_offset;
        }
    }

    // Determine logical line range to display
    let (first_line, last_line) = if total_lines == 0 {
        (0usize, 0usize)
    } else {
        let bottom_logical = total_lines
            .saturating_sub(1)
            .saturating_sub(view_offset);
        let first = bottom_logical.saturating_sub(visible_rows - 1);
        (first, bottom_logical)
    };

    // Draw scrollback
    let mut screen_row = 0usize;
    let mut line = first_line;
    while screen_row < visible_rows && line <= last_line && line < total_lines {
        let ring = ring_index(line);
        let y = top + screen_row * line_h;

        for col in 0..visible_cols {
            let byte = unsafe { LINES[ring][col] };
            let ch = if byte == 0 { ' ' } else { byte as char };
            let x = left + col * FONT_W;
            draw_char(x, y, ch, SHELL_FG_COLOR);
        }

        screen_row += 1;
        line += 1;
    }

    // Draw live prompt + input only when at bottom
    if view_offset == 0 {
        let prompt_row = if visible_rows == 0 {
            0
        } else {
            visible_rows - 1
        };
        let y = top + prompt_row * line_h;

        // "> "
        let mut col = 0usize;
        for ch in PROMPT.chars() {
            if col >= visible_cols {
                break;
            }
            let x = left + col * FONT_W;
            draw_char(x, y, ch, SHELL_FG_COLOR);
            col += 1;
        }

        // command buffer
        for i in 0..cmd_len {
            if col >= visible_cols {
                break;
            }
            let ch = cmd_buf[i] as char;
            let x = left + col * FONT_W;
            draw_char(x, y, ch, SHELL_FG_COLOR);
            col += 1;
        }
    }
}

// -----------------------------------------------------------------------------
// Command handling
// -----------------------------------------------------------------------------

fn execute_command(cmd: &str) {
    let mut parts = cmd.split_whitespace();
    let Some(name) = parts.next() else {
        return;
    };

    match name {
        "help" => {
            print_line("Available commands:");
            print_line("  help        - show this help");
            print_line("  clear       - clear terminal");
            print_line("  net.scan    - run simple network scan");
            print_line("  scroll.up   - scroll one line up");
            print_line("  scroll.down - scroll one line down");
        }
        "clear" => {
            unsafe {
                for r in 0..MAX_ROWS {
                    clear_line(r);
                }
                TOTAL_LINES = 0;
                CUR_LINE = 0;
                CUR_COL = 0;
                VIEW_OFFSET = 0;
                new_line();
            }
        }
        "net.scan" => {
            // Uses your existing net::net_scan()
            net::net_scan();
        }
        "scroll.up" => {
            scroll_up();
        }
        "scroll.down" => {
            scroll_down();
        }
        "" => {}
        _ => {
            print_line("Unknown command. Type 'help' for a list.");
        }
    }
}

// -----------------------------------------------------------------------------
// Main shell loop
// -----------------------------------------------------------------------------

pub fn run_shell() -> ! {
    unsafe {
        // Init buffer
        for r in 0..MAX_ROWS {
            clear_line(r);
        }
        TOTAL_LINES = 0;
        CUR_LINE = 0;
        CUR_COL = 0;
        VIEW_OFFSET = 0;
        new_line();
    }

    print_line("Welcome to Othello OS shell.");
    print_line("Type 'help' for commands.");
    let mut cmd_buf = [0u8; MAX_CMD_LEN];
    let mut cmd_len: usize = 0;

    render(&cmd_buf, cmd_len);

    loop {
        let mut updated = false;

        if let Some(sc) = keyboard_poll_scancode() {
            if let Some(ch) = scancode_to_ascii(sc) {
                match ch {
                    '\n' | '\r' => {
                        // Turn command buffer into &str
                        let cmd = str::from_utf8(&cmd_buf[..cmd_len]).unwrap_or("");
                        // Echo command into scrollback
                        print_str(PROMPT);
                        print_str(cmd);
                        unsafe { new_line(); }
                        execute_command(cmd);

                        // Reset buffer
                        for b in cmd_buf.iter_mut() {
                            *b = 0;
                        }
                        cmd_len = 0;
                        updated = true;
                    }
                    '\x08' | '\x7F' => {
                        // Backspace
                        if cmd_len > 0 {
                            cmd_len -= 1;
                            cmd_buf[cmd_len] = 0;
                            updated = true;
                        }
                    }
                    _ => {
                        // Regular printable
                        if cmd_len < MAX_CMD_LEN - 1 {
                            cmd_buf[cmd_len] = ch as u8;
                            cmd_len += 1;
                            updated = true;
                        }
                    }
                }
            }
        }

        if updated {
            render(&cmd_buf, cmd_len);
        }
    }
}
