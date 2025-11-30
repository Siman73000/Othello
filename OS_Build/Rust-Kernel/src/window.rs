extern crate alloc;

use alloc::vec::Vec;
use crate::framebuffer_driver::{
    clear_screen as clear_framebuffer, commit as commit_framebuffer, info as framebuffer_info,
    set_pixel,
};
use crate::network_drivers::poll_input_event;
use crate::MAX_WINDOWS;
use spin::Mutex;
use x86_64::instructions::hlt;

// Local stub for mouse position
pub fn poll_mouse_position() -> (usize, usize) {
    (0, 0) // Replace with real mouse logic later
}

const BACKGROUND_COLOR: u32 = 0x000000;
const DEFAULT_BORDER_COLOR: u32 = 0x444444;
const DEFAULT_TITLE_COLOR: u32 = 0x222266;
const ACTIVE_TITLE_COLOR: u32 = 0x3333FF;
const HOVER_BORDER_COLOR: u32 = 0x00FF00;
const TITLE_BAR_HEIGHT: usize = 16;

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
    for y in 0..title_height {
        for x in 0..window.width {
            window.buffer[y * window.width + x] = color;
        }
    }
}

// Draw border
pub fn draw_border(window: &mut Window, color: u32) {
    let w = window.width;
    let h = window.height;
    if w == 0 || h == 0 {
        return;
    }
    for x in 0..w {
        if let Some(top) = window.buffer.get_mut(x) {
            *top = color; // top
        }
        let bottom_idx = (h - 1).saturating_mul(w).saturating_add(x);
        if let Some(bottom) = window.buffer.get_mut(bottom_idx) {
            *bottom = color; // bottom
        }
    }
    for y in 0..h {
        let left_idx = y.saturating_mul(w);
        if let Some(left) = window.buffer.get_mut(left_idx) {
            *left = color; // left
        }
        let right_idx = y
            .saturating_mul(w)
            .saturating_add(w.saturating_sub(1));
        if let Some(right) = window.buffer.get_mut(right_idx) {
            *right = color; // right
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
    fill_window(window, window.color);
    draw_border(window, border_color);
    draw_title_bar(window, title_color);
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
