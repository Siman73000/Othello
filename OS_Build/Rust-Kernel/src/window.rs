#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::cell::RefCell;
use crate::display::{set_pixel, clear_screen, commit_framebuffer};
use crate::network_drivers::poll_input_event;
use spin::Mutex;

// Window structure
pub struct Window {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub buffer: Vec<u32>,
    pub title: &'static str,
}

// Global window list protected by a Mutex
static WINDOWS: Mutex<RefCell<Vec<Window>>> = Mutex::new(RefCell::new(Vec::new()));

// Basic UI events
#[derive(Clone, Copy)]
pub enum GuiEvent {
    MouseClick { x: usize, y: usize },
    MouseMove  { x: usize, y: usize },
    KeyPress   { key: char },
}

// Create a window
pub fn create_window(x: usize, y: usize, width: usize, height: usize, color: u32, title: &'static str) {
    let window = Window {
        x,
        y,
        width,
        height,
        buffer: alloc::vec![color; width * height],
        title,
    };

    WINDOWS.lock().borrow_mut().push(window);
}

// Handle incoming events
pub fn handle_event(event: GuiEvent) {
    let mut wins = WINDOWS.lock().borrow_mut();
    for window in wins.iter_mut() {
        match event {
            GuiEvent::MouseClick { x, y } => {
                if in_window(x, y, window) {
                    draw_title_bar(window, 0x3333FF);
                }
            }
            GuiEvent::MouseMove { x, y } => {
                if in_window(x, y, window) {
                    draw_border(window, 0x00FF00);
                }
            }
            GuiEvent::KeyPress { key } => {
                if key == 'c' {
                    fill_window(window, 0x000000); // clear on key 'c'
                }
            }
        }
    }
}

// Check if mouse is in window bounds
pub fn in_window(x: usize, y: usize, window: &Window) -> bool {
    x >= window.x && x < window.x + window.width &&
    y >= window.y && y < window.y + window.height
}

// Draw a single window
pub fn draw_window(window: &Window) {
    for y in 0..window.height {
        for x in 0..window.width {
            let color = window.buffer[y * window.width + x];
            set_pixel(window.x + x, window.y + y, color);
        }
    }
}

// Draw title bar
pub fn draw_title_bar(window: &mut Window, color: u32) {
    for y in 0..16 {
        for x in 0..window.width {
            window.buffer[y * window.width + x] = color;
        }
    }
}

// Draw border
pub fn draw_border(window: &mut Window, color: u32) {
    let w = window.width;
    let h = window.height;
    for x in 0..w {
        window.buffer[x] = color;                    // top
        window.buffer[(h - 1) * w + x] = color;      // bottom
    }
    for y in 0..h {
        window.buffer[y * w] = color;                // left
        window.buffer[y * w + (w - 1)] = color;      // right
    }
}

// Fill window with solid color
pub fn fill_window(window: &mut Window, color: u32) {
    for pixel in window.buffer.iter_mut() {
        *pixel = color;
    }
}

// Redraw all windows
pub fn redraw_screen() {
    clear_screen(0x000000);
    let wins = WINDOWS.lock().borrow();
    for w in wins.iter() {
        draw_window(w);
    }
    commit_framebuffer();
}

// The GUI event loop
pub fn start_gui_loop() -> ! {
    loop {
        if let Some(event) = poll_input_event() {
            handle_event(event);
        }
        redraw_screen();
    }
}
