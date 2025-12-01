extern crate alloc;

mod shell;

use alloc::vec::Vec;
use crate::framebuffer_driver::{
    clear_screen as clear_framebuffer, commit as commit_framebuffer, info as framebuffer_info,
    set_pixel,
};
use crate::network_drivers::poll_input_event;
use crate::MAX_WINDOWS;
use spin::Mutex;
use x86_64::instructions::hlt;
use shell::handle_keypress;
pub use shell::paint_shell;

// Local stub for mouse position
pub fn poll_mouse_position() -> (usize, usize) {
    (0, 0) // Replace with real mouse logic later
}

const BACKGROUND_COLOR: u32 = 0x0a0f1c;
const DEFAULT_BORDER_COLOR: u32 = 0x1f2937;
const DEFAULT_TITLE_COLOR: u32 = 0x1f2a44;
const ACTIVE_TITLE_COLOR: u32 = 0x3347ff;
const HOVER_BORDER_COLOR: u32 = 0x22c55e;
const TITLE_BAR_HEIGHT: usize = 18;
const TITLE_TEXT_COLOR: u32 = 0xe5e7eb;
const SUBDUED_TEXT_COLOR: u32 = 0x9ca3af;
const ACCENT_COLOR: u32 = 0x38bdf8;
const CARD_GRADIENT_TOP: u32 = 0x111827;
const CARD_GRADIENT_BOTTOM: u32 = 0x0b1220;
const WINDOW_BORDER_THICKNESS: usize = 2;
const FONT_WIDTH: usize = 5;
const FONT_HEIGHT: usize = 7;

const FONT_5X7: [[u8; FONT_WIDTH]; 96] = [
    [0x00, 0x00, 0x00, 0x00, 0x00], // 0x20 ' '
    [0x00, 0x00, 0x5f, 0x00, 0x00], // 0x21 '!'
    [0x00, 0x07, 0x00, 0x07, 0x00], // 0x22 '"'
    [0x14, 0x7f, 0x14, 0x7f, 0x14], // 0x23 '#'
    [0x24, 0x2a, 0x7f, 0x2a, 0x12], // 0x24 '$'
    [0x23, 0x13, 0x08, 0x64, 0x62], // 0x25 '%'
    [0x36, 0x49, 0x55, 0x22, 0x50], // 0x26 '&'
    [0x00, 0x05, 0x03, 0x00, 0x00], // 0x27 '\''
    [0x00, 0x1c, 0x22, 0x41, 0x00], // 0x28 '('
    [0x00, 0x41, 0x22, 0x1c, 0x00], // 0x29 ')'
    [0x14, 0x08, 0x3e, 0x08, 0x14], // 0x2a '*'
    [0x08, 0x08, 0x3e, 0x08, 0x08], // 0x2b '+'
    [0x00, 0x50, 0x30, 0x00, 0x00], // 0x2c ','
    [0x08, 0x08, 0x08, 0x08, 0x08], // 0x2d '-'
    [0x00, 0x60, 0x60, 0x00, 0x00], // 0x2e '.'
    [0x20, 0x10, 0x08, 0x04, 0x02], // 0x2f '/'
    [0x3e, 0x51, 0x49, 0x45, 0x3e], // 0x30 '0'
    [0x00, 0x42, 0x7f, 0x40, 0x00], // 0x31 '1'
    [0x42, 0x61, 0x51, 0x49, 0x46], // 0x32 '2'
    [0x21, 0x41, 0x45, 0x4b, 0x31], // 0x33 '3'
    [0x18, 0x14, 0x12, 0x7f, 0x10], // 0x34 '4'
    [0x27, 0x45, 0x45, 0x45, 0x39], // 0x35 '5'
    [0x3c, 0x4a, 0x49, 0x49, 0x30], // 0x36 '6'
    [0x01, 0x71, 0x09, 0x05, 0x03], // 0x37 '7'
    [0x36, 0x49, 0x49, 0x49, 0x36], // 0x38 '8'
    [0x06, 0x49, 0x49, 0x29, 0x1e], // 0x39 '9'
    [0x00, 0x36, 0x36, 0x00, 0x00], // 0x3a ':'
    [0x00, 0x56, 0x36, 0x00, 0x00], // 0x3b ';'
    [0x08, 0x14, 0x22, 0x41, 0x00], // 0x3c '<'
    [0x14, 0x14, 0x14, 0x14, 0x14], // 0x3d '='
    [0x00, 0x41, 0x22, 0x14, 0x08], // 0x3e '>'
    [0x02, 0x01, 0x51, 0x09, 0x06], // 0x3f '?'
    [0x32, 0x49, 0x79, 0x41, 0x3e], // 0x40 '@'
    [0x7e, 0x11, 0x11, 0x11, 0x7e], // 0x41 'A'
    [0x7f, 0x49, 0x49, 0x49, 0x36], // 0x42 'B'
    [0x3e, 0x41, 0x41, 0x41, 0x22], // 0x43 'C'
    [0x7f, 0x41, 0x41, 0x22, 0x1c], // 0x44 'D'
    [0x7f, 0x49, 0x49, 0x49, 0x41], // 0x45 'E'
    [0x7f, 0x09, 0x09, 0x09, 0x01], // 0x46 'F'
    [0x3e, 0x41, 0x49, 0x49, 0x7a], // 0x47 'G'
    [0x7f, 0x08, 0x08, 0x08, 0x7f], // 0x48 'H'
    [0x00, 0x41, 0x7f, 0x41, 0x00], // 0x49 'I'
    [0x20, 0x40, 0x41, 0x3f, 0x01], // 0x4a 'J'
    [0x7f, 0x08, 0x14, 0x22, 0x41], // 0x4b 'K'
    [0x7f, 0x40, 0x40, 0x40, 0x40], // 0x4c 'L'
    [0x7f, 0x02, 0x0c, 0x02, 0x7f], // 0x4d 'M'
    [0x7f, 0x04, 0x08, 0x10, 0x7f], // 0x4e 'N'
    [0x3e, 0x41, 0x41, 0x41, 0x3e], // 0x4f 'O'
    [0x7f, 0x09, 0x09, 0x09, 0x06], // 0x50 'P'
    [0x3e, 0x41, 0x51, 0x21, 0x5e], // 0x51 'Q'
    [0x7f, 0x09, 0x19, 0x29, 0x46], // 0x52 'R'
    [0x46, 0x49, 0x49, 0x49, 0x31], // 0x53 'S'
    [0x01, 0x01, 0x7f, 0x01, 0x01], // 0x54 'T'
    [0x3f, 0x40, 0x40, 0x40, 0x3f], // 0x55 'U'
    [0x1f, 0x20, 0x40, 0x20, 0x1f], // 0x56 'V'
    [0x3f, 0x40, 0x38, 0x40, 0x3f], // 0x57 'W'
    [0x63, 0x14, 0x08, 0x14, 0x63], // 0x58 'X'
    [0x07, 0x08, 0x70, 0x08, 0x07], // 0x59 'Y'
    [0x61, 0x51, 0x49, 0x45, 0x43], // 0x5a 'Z'
    [0x00, 0x7f, 0x41, 0x41, 0x00], // 0x5b '['
    [0x02, 0x04, 0x08, 0x10, 0x20], // 0x5c '\\'
    [0x00, 0x41, 0x41, 0x7f, 0x00], // 0x5d ']'
    [0x04, 0x02, 0x01, 0x02, 0x04], // 0x5e '^'
    [0x80, 0x80, 0x80, 0x80, 0x80], // 0x5f '_'
    [0x00, 0x03, 0x05, 0x00, 0x00], // 0x60 '`'
    [0x20, 0x54, 0x54, 0x54, 0x78], // 0x61 'a'
    [0x7f, 0x48, 0x44, 0x44, 0x38], // 0x62 'b'
    [0x38, 0x44, 0x44, 0x44, 0x20], // 0x63 'c'
    [0x38, 0x44, 0x44, 0x48, 0x7f], // 0x64 'd'
    [0x38, 0x54, 0x54, 0x54, 0x18], // 0x65 'e'
    [0x08, 0x7e, 0x09, 0x01, 0x02], // 0x66 'f'
    [0x0c, 0x52, 0x52, 0x52, 0x3e], // 0x67 'g'
    [0x7f, 0x08, 0x04, 0x04, 0x78], // 0x68 'h'
    [0x00, 0x44, 0x7d, 0x40, 0x00], // 0x69 'i'
    [0x20, 0x40, 0x44, 0x3d, 0x00], // 0x6a 'j'
    [0x7f, 0x10, 0x28, 0x44, 0x00], // 0x6b 'k'
    [0x00, 0x41, 0x7f, 0x40, 0x00], // 0x6c 'l'
    [0x7c, 0x04, 0x18, 0x04, 0x78], // 0x6d 'm'
    [0x7c, 0x08, 0x04, 0x04, 0x78], // 0x6e 'n'
    [0x38, 0x44, 0x44, 0x44, 0x38], // 0x6f 'o'
    [0x7c, 0x14, 0x14, 0x14, 0x08], // 0x70 'p'
    [0x08, 0x14, 0x14, 0x18, 0x7c], // 0x71 'q'
    [0x7c, 0x08, 0x04, 0x04, 0x08], // 0x72 'r'
    [0x48, 0x54, 0x54, 0x54, 0x20], // 0x73 's'
    [0x04, 0x3f, 0x44, 0x40, 0x20], // 0x74 't'
    [0x3c, 0x40, 0x40, 0x20, 0x7c], // 0x75 'u'
    [0x1c, 0x20, 0x40, 0x20, 0x1c], // 0x76 'v'
    [0x3c, 0x40, 0x30, 0x40, 0x3c], // 0x77 'w'
    [0x44, 0x28, 0x10, 0x28, 0x44], // 0x78 'x'
    [0x0c, 0x50, 0x50, 0x50, 0x3c], // 0x79 'y'
    [0x44, 0x64, 0x54, 0x4c, 0x44], // 0x7a 'z'
    [0x00, 0x08, 0x36, 0x41, 0x00], // 0x7b '{'
    [0x00, 0x00, 0x7f, 0x00, 0x00], // 0x7c '|'
    [0x00, 0x41, 0x36, 0x08, 0x00], // 0x7d '}'
    [0x10, 0x08, 0x08, 0x10, 0x08], // 0x7e '~'
    [0x7f, 0x7f, 0x7f, 0x7f, 0x7f], // 0x7f delete block
];

pub type WindowHandle = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowError {
    NoFramebuffer,
    InvalidDimensions,
    BufferTooSmall,
    OutOfWindowSlots,
}

// Window structure
pub struct Window {
    pub id: WindowHandle,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub visible: bool,
    pub color: u32,
    pub title: &'static str,
    pub buffer: &'static mut [u32], // framebuffer for window contents
}

// Global window list protected by a Mutex
pub static WINDOWS: Mutex<[Option<Window>; MAX_WINDOWS]> = Mutex::new([
    None, None, None, None, None, None, None, None, None, None, None, None, None, None,
    None, None,
]);

// Z-order of windows: last item is top-most
static WINDOW_ORDER: Mutex<Vec<WindowHandle>> = Mutex::new(Vec::new());
static ACTIVE_WINDOW: Mutex<Option<WindowHandle>> = Mutex::new(None);

// Create a new window struct (without buffer)
impl Window {
    pub const fn new() -> Self {
        Self {
            id: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            visible: false,
            color: 0x000000,
            title: "",
            buffer: &mut [], // empty slice; replace with actual buffer later
        }
    }
}

fn framebuffer_bounds() -> Option<(usize, usize)> {
    framebuffer_info().and_then(|(w, h)| (w > 0 && h > 0).then_some((w, h)))
}

fn mix_channel(start: u8, end: u8, step: usize, max: usize) -> u8 {
    let s = start as usize;
    let e = end as usize;
    let blended = s + ((e.saturating_sub(s)) * step) / max.max(1);
    blended.min(255) as u8
}

fn lerp_color(start: u32, end: u32, step: usize, max: usize) -> u32 {
    let sr = ((start >> 16) & 0xff) as u8;
    let sg = ((start >> 8) & 0xff) as u8;
    let sb = (start & 0xff) as u8;

    let er = ((end >> 16) & 0xff) as u8;
    let eg = ((end >> 8) & 0xff) as u8;
    let eb = (end & 0xff) as u8;

    let r = mix_channel(sr, er, step, max);
    let g = mix_channel(sg, eg, step, max);
    let b = mix_channel(sb, eb, step, max);

    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

fn lighten(color: u32, amount_pct: u8) -> u32 {
    let factor = amount_pct.min(100) as usize;
    let r = ((color >> 16) & 0xff) as usize;
    let g = ((color >> 8) & 0xff) as usize;
    let b = (color & 0xff) as usize;

    let lift = |c: usize| -> u32 { (c + ((255 - c) * factor) / 100) as u32 };

    (lift(r) << 16) | (lift(g) << 8) | lift(b)
}

fn draw_gradient_rect(
    window: &mut Window,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    top: u32,
    bottom: u32,
) {
    if width == 0 || height == 0 {
        return;
    }
    let max_step = height.saturating_sub(1).max(1);
    for row in 0..height {
        let color = lerp_color(top, bottom, row, max_step);
        for col in 0..width {
            let px = x + col;
            let py = y + row;
            if px < window.width && py < window.height {
                let idx = py * window.width + px;
                if let Some(pixel) = window.buffer.get_mut(idx) {
                    *pixel = color;
                }
            }
        }
    }
}

fn draw_rect(window: &mut Window, x: usize, y: usize, width: usize, height: usize, color: u32) {
    if width == 0 || height == 0 {
        return;
    }
    for row in 0..height {
        for col in 0..width {
            let px = x + col;
            let py = y + row;
            if px < window.width && py < window.height {
                let idx = py * window.width + px;
                if let Some(pixel) = window.buffer.get_mut(idx) {
                    *pixel = color;
                }
            }
        }
    }
}

fn draw_rect_border(
    window: &mut Window,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    thickness: usize,
    color: u32,
) {
    let width = width.min(window.width.saturating_sub(x));
    let height = height.min(window.height.saturating_sub(y));
    if width == 0 || height == 0 {
        return;
    }
    for layer in 0..thickness {
        let inner_x = x.saturating_add(layer);
        let inner_y = y.saturating_add(layer);
        let inner_w = width.saturating_sub(layer * 2);
        let inner_h = height.saturating_sub(layer * 2);
        if inner_w == 0 || inner_h == 0 {
            break;
        }
        for dx in 0..inner_w {
            let top_idx = (inner_y * window.width).saturating_add(inner_x + dx);
            let bottom_idx =
                ((inner_y + inner_h.saturating_sub(1)) * window.width).saturating_add(inner_x + dx);
            if let Some(px) = window.buffer.get_mut(top_idx) {
                *px = color;
            }
            if let Some(px) = window.buffer.get_mut(bottom_idx) {
                *px = color;
            }
        }
        for dy in 0..inner_h {
            let left_idx = (inner_y + dy) * window.width + inner_x;
            let right_idx =
                (inner_y + dy) * window.width + inner_x.saturating_add(inner_w.saturating_sub(1));
            if let Some(px) = window.buffer.get_mut(left_idx) {
                *px = color;
            }
            if let Some(px) = window.buffer.get_mut(right_idx) {
                *px = color;
            }
        }
    }
}

fn draw_char(window: &mut Window, x: usize, y: usize, ch: char, color: u32) {
    if !ch.is_ascii() || ch < ' ' {
        return;
    }
    let glyph_index = (ch as u8 - 0x20) as usize;
    if let Some(columns) = FONT_5X7.get(glyph_index) {
        for (col, byte) in columns.iter().enumerate() {
            for row in 0..FONT_HEIGHT {
                if (byte >> row) & 0x01 != 0 {
                    let px = x + col;
                    let py = y + row;
                    if px < window.width && py < window.height {
                        let idx = py * window.width + px;
                        if let Some(pixel) = window.buffer.get_mut(idx) {
                            *pixel = color;
                        }
                    }
                }
            }
        }
    }
}

fn draw_text(window: &mut Window, x: usize, y: usize, text: &str, color: u32) {
    let mut cursor_x = x;
    for ch in text.chars() {
        if ch == '\n' {
            cursor_x = x;
            continue;
        }
        draw_char(window, cursor_x, y, ch, color);
        cursor_x = cursor_x.saturating_add(FONT_WIDTH + 1);
        if cursor_x >= window.width.saturating_sub(FONT_WIDTH) {
            break;
        }
    }
}

// Create a window
pub fn create_window(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: u32,
    title: &'static str,
    buffer: &'static mut [u32],
) -> Result<WindowHandle, WindowError> {
    let (screen_w, screen_h) = framebuffer_bounds().ok_or(WindowError::NoFramebuffer)?;
    if width == 0 || height == 0 {
        return Err(WindowError::InvalidDimensions);
    }

    let clamped_width = width.min(screen_w.max(1));
    let clamped_height = height.min(screen_h.max(1));
    let pos_x = x.min(screen_w.saturating_sub(clamped_width));
    let pos_y = y.min(screen_h.saturating_sub(clamped_height));

    let expected_len = clamped_width
        .checked_mul(clamped_height)
        .ok_or(WindowError::InvalidDimensions)?;
    if buffer.len() < expected_len {
        return Err(WindowError::BufferTooSmall);
    }

    let mut wins = WINDOWS.lock();

    let mut handle: Option<WindowHandle> = None;
    for (idx, slot) in wins.iter_mut().enumerate() {
        if slot.is_none() {
            handle = Some(idx);
            break;
        }
    }

    let window_id = handle.ok_or(WindowError::OutOfWindowSlots)?;

    let mut window = Window {
        id: window_id,
        x: pos_x,
        y: pos_y,
        width: clamped_width,
        height: clamped_height,
        visible: true,
        color,
        title,
        buffer: &mut buffer[..expected_len],
    };

    prepare_window_surface(&mut window, DEFAULT_TITLE_COLOR, DEFAULT_BORDER_COLOR);

    wins[window_id] = Some(window);

    {
        let mut order = WINDOW_ORDER.lock();
        order.retain(|&id| id != window_id);
        order.push(window_id);
    }

    Ok(window_id)
}

// Handle incoming events
pub fn handle_event(event: GuiEvent) {
    match event {
        GuiEvent::MouseClick { x, y } => {
            if let Some(id) = find_top_window_at(x, y) {
                set_active_window(Some(id));
                let _ = mutate_window(id, |window| {
                    draw_title_bar(window, ACTIVE_TITLE_COLOR);
                    draw_border(window, DEFAULT_BORDER_COLOR);
                });
                promote_window(id);
            }
        }
        GuiEvent::MouseMove { x, y } => {
            let active_id = *ACTIVE_WINDOW.lock();
            if let Some(id) = active_id.or_else(|| find_top_window_at(x, y)) {
                let mut wins = WINDOWS.lock();
                for (idx, win) in wins.iter_mut().enumerate() {
                    if let Some(window) = win {
                        if idx == id {
                            draw_border(window, HOVER_BORDER_COLOR);
                        } else if window.visible {
                            draw_border(window, DEFAULT_BORDER_COLOR);
                        }
                    }
                }
            }
        }
        GuiEvent::KeyPress { key } => {
            if let Some(active) = *ACTIVE_WINDOW.lock() {
                if handle_keypress(active, key) {
                    return;
                }

                let _ = mutate_window(active, |window| match key {
                    'c' => prepare_window_surface(window, DEFAULT_TITLE_COLOR, DEFAULT_BORDER_COLOR),
                    'h' => window.visible = false,
                    's' => window.visible = true,
                    _ => {}
                });
            }
        }
    }
}

fn mutate_window<R>(id: WindowHandle, f: impl FnOnce(&mut Window) -> R) -> Option<R> {
    let mut wins = WINDOWS.lock();
    if let Some(slot) = wins.get_mut(id) {
        if let Some(window) = slot.as_mut() {
            return Some(f(window));
        }
    }
    None
}

fn set_active_window(id: Option<WindowHandle>) {
    let mut active = ACTIVE_WINDOW.lock();
    *active = id;
}

fn promote_window(id: WindowHandle) {
    let mut order = WINDOW_ORDER.lock();
    order.retain(|&w| w != id);
    order.push(id);
}

fn find_top_window_at(x: usize, y: usize) -> Option<WindowHandle> {
    let order_snapshot = {
        let order = WINDOW_ORDER.lock();
        order.clone()
    };
    let wins = WINDOWS.lock();
    order_snapshot
        .iter()
        .rev()
        .copied()
        .find(|&id| {
            wins
                .get(id)
                .and_then(|slot| slot.as_ref())
                .map(|window| window.visible && in_window(x, y, window))
                .unwrap_or(false)
        })
}

// Check if mouse is in window bounds
pub fn in_window(x: usize, y: usize, window: &Window) -> bool {
    let within_x = x >= window.x && x - window.x < window.width;
    let within_y = y >= window.y && y - window.y < window.height;
    within_x && within_y
}

pub fn move_window(handle: WindowHandle, x: usize, y: usize) -> bool {
    let Some((screen_w, screen_h)) = framebuffer_bounds() else {
        return false;
    };

    mutate_window(handle, |window| {
        window.x = x.min(screen_w.saturating_sub(window.width));
        window.y = y.min(screen_h.saturating_sub(window.height));
    })
    .is_some()
}

pub fn close_window(handle: WindowHandle) -> bool {
    let mut wins = WINDOWS.lock();
    let mut closed = false;
    if let Some(slot) = wins.get_mut(handle) {
        closed = slot.take().is_some();
    }

    if closed {
        let mut order = WINDOW_ORDER.lock();
        order.retain(|&id| id != handle);
        let mut active = ACTIVE_WINDOW.lock();
        if active.map(|id| id == handle).unwrap_or(false) {
            *active = order.last().copied();
        }
    }

    closed
}

// Draw a single window
pub fn draw_window(window: &Window) {
    if !window.visible {
        return;
    }
    let (screen_w, screen_h) = match framebuffer_bounds() {
        Some(bounds) => bounds,
        None => return,
    };
    let max_y = window.y.saturating_add(window.height).min(screen_h);
    let max_x = window.x.saturating_add(window.width).min(screen_w);
    for y in window.y..max_y {
        let wy = y - window.y;
        for x in window.x..max_x {
            let wx = x - window.x;
            if let Some(idx) = wy
                .checked_mul(window.width)
                .and_then(|row| row.checked_add(wx))
            {
                if let Some(&color) = window.buffer.get(idx) {
                    set_pixel(x, y, color);
                }
            }
        }
    }
}

// Redraw all windows
pub fn render_windows() {
    let order_snapshot = {
        let order = WINDOW_ORDER.lock();
        order.clone()
    };
    let wins = WINDOWS.lock();
    for &id in order_snapshot.iter() {
        if let Some(window) = wins.get(id).and_then(|slot| slot.as_ref()) {
            draw_window(window);
        }
    }
}

// Draw title bar
pub fn draw_title_bar(window: &mut Window, color: u32) {
    if window.height == 0 || window.width == 0 {
        return;
    }
    let title_height = TITLE_BAR_HEIGHT.min(window.height);
    let highlight = lighten(color, 12);
    draw_gradient_rect(window, 0, 0, window.width, title_height, color, highlight);
    draw_rect(window, 0, title_height.saturating_sub(2), window.width, 2, ACCENT_COLOR);

    let text_y = title_height.saturating_sub(FONT_HEIGHT + 4);
    draw_text(window, 8, text_y, window.title, TITLE_TEXT_COLOR);
}

// Draw border
pub fn draw_border(window: &mut Window, color: u32) {
    let w = window.width;
    let h = window.height;
    if w == 0 || h == 0 {
        return;
    }
    for layer in 0..WINDOW_BORDER_THICKNESS {
        let top_y = layer;
        let bottom_y = h.saturating_sub(1 + layer);
        for x in layer..w.saturating_sub(layer) {
            let top_idx = top_y * w + x;
            let bottom_idx = bottom_y * w + x;
            if let Some(px) = window.buffer.get_mut(top_idx) {
                *px = color;
            }
            if let Some(px) = window.buffer.get_mut(bottom_idx) {
                *px = color;
            }
        }

        for y in layer..h.saturating_sub(layer) {
            let left_idx = y * w + layer;
            let right_idx = y * w + w.saturating_sub(1 + layer);
            if let Some(px) = window.buffer.get_mut(left_idx) {
                *px = color;
            }
            if let Some(px) = window.buffer.get_mut(right_idx) {
                *px = color;
            }
        }
    }
}

// Fill window with solid color
pub fn fill_window(window: &mut Window, color: u32) {
    for pixel in window.buffer.iter_mut() {
        *pixel = color;
    }
}

fn prepare_window_surface(window: &mut Window, title_color: u32, border_color: u32) {
    let body_top = TITLE_BAR_HEIGHT.min(window.height);
    fill_window(window, window.color);
    let gradient_top = lighten(window.color, 8);
    draw_gradient_rect(
        window,
        0,
        body_top,
        window.width,
        window.height.saturating_sub(body_top),
        gradient_top,
        window.color,
    );
    draw_border(window, border_color);
    draw_title_bar(window, title_color);
}

fn draw_stat_card(
    window: &mut Window,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    title: &str,
    value: &str,
    accent: u32,
) {
    draw_gradient_rect(window, x, y, width, height, CARD_GRADIENT_TOP, CARD_GRADIENT_BOTTOM);
    draw_rect_border(window, x, y, width, height, 1, DEFAULT_BORDER_COLOR);
    draw_rect(window, x, y, width, 2, accent);
    draw_text(window, x + 10, y + 6, title, SUBDUED_TEXT_COLOR);
    draw_text(window, x + 10, y + 6 + FONT_HEIGHT + 2, value, TITLE_TEXT_COLOR);
}

pub fn paint_system_monitor(handle: WindowHandle) {
    let _ = mutate_window(handle, |window| {
        prepare_window_surface(window, ACTIVE_TITLE_COLOR, DEFAULT_BORDER_COLOR);
        let padding = 10;
        let card_width = window.width.saturating_sub(padding * 2);
        let mut cursor_y = TITLE_BAR_HEIGHT + padding;

        draw_stat_card(
            window,
            padding,
            cursor_y,
            card_width,
            48,
            "CPU LOAD",
            "42% • realtime scheduler active",
            ACCENT_COLOR,
        );
        cursor_y = cursor_y.saturating_add(56);

        draw_stat_card(
            window,
            padding,
            cursor_y,
            card_width,
            48,
            "MEMORY",
            "128 MiB used / 512 MiB total",
            lighten(ACCENT_COLOR, 15),
        );
        cursor_y = cursor_y.saturating_add(56);

        draw_stat_card(
            window,
            padding,
            cursor_y,
            card_width,
            48,
            "IO LATENCY",
            "0.6 ms avg • DMA enabled",
            0x22c55e,
        );
    });
}

pub fn paint_network_console(handle: WindowHandle) {
    let _ = mutate_window(handle, |window| {
        prepare_window_surface(window, DEFAULT_TITLE_COLOR, DEFAULT_BORDER_COLOR);
        let padding = 10;
        let mut cursor_y = TITLE_BAR_HEIGHT + padding;
        let lines = [
            "[net] device rtl8139 online",
            "[net] link up @ 1Gbps full duplex",
            "[net] hotplug monitor: hdmi+dp ready",
            "[net] scanning for dhcp leases…",
            "[net] assigned 10.0.2.15/24",
            "[net] gateway 10.0.2.2 • mtu 1500",
        ];

        for line in lines.iter() {
            draw_text(window, padding, cursor_y, line, TITLE_TEXT_COLOR);
            cursor_y = cursor_y.saturating_add(FONT_HEIGHT + 4);
        }

        draw_rect(
            window,
            padding,
            TITLE_BAR_HEIGHT.saturating_sub(1),
            window.width.saturating_sub(padding * 2),
            2,
            ACCENT_COLOR,
        );
    });
}

// Redraw entire screen
pub fn redraw_screen() {
    clear_framebuffer(BACKGROUND_COLOR);
    render_windows();
    commit_framebuffer();
}

// The GUI event loop
pub fn start_gui_loop() -> ! {
    let mut last_mouse = poll_mouse_position();
    loop {
        if framebuffer_bounds().is_none() {
            hlt();
            continue;
        }
        if let Some(event) = poll_input_event() {
            handle_event(event);
        }
        let pos = poll_mouse_position();
        if pos != last_mouse {
            handle_event(GuiEvent::MouseMove {
                x: pos.0,
                y: pos.1,
            });
            last_mouse = pos;
        }
        redraw_screen();
        hlt();
    }
}

// Dummy GuiEvent enum for completeness
#[derive(Clone, Copy)]
pub enum GuiEvent {
    MouseClick { x: usize, y: usize },
    MouseMove { x: usize, y: usize },
    KeyPress { key: char },
}
