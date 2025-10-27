extern crate alloc;

use crate::display::{set_pixel, clear_screen, commit_framebuffer};
use crate::network_drivers::poll_input_event;
use crate::MAX_WINDOWS;
use core::convert::TryInto;
use spin::Mutex;

// Local stub for mouse position
pub fn poll_mouse_position() -> (usize, usize) {
    (0, 0) // Replace with real mouse logic later
}

// Window structure
pub struct Window {
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
pub static WINDOWS: Mutex<[Option<Window>; MAX_WINDOWS]> = Mutex::new({
    let mut arr: [Option<Window>; MAX_WINDOWS] =
        unsafe { core::mem::MaybeUninit::uninit().assume_init() };
    let mut i = 0;
    while i < MAX_WINDOWS {
        arr[i] = None;
        i += 1;
    }
    arr
});

// Create a new window struct (without buffer)
impl Window {
    pub const fn new() -> Self {
        Self {
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

// Create a window
pub fn create_window(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: u32,
    title: &'static str,
    buffer: &'static mut [u32],
) {
    let window = Window {
        x: x.try_into().unwrap(),
        y: y.try_into().unwrap(),
        width: width.try_into().unwrap(),
        height: height.try_into().unwrap(),
        visible: true,
        color,
        title,
        buffer,
    };

    let mut wins = WINDOWS.lock();
    for slot in wins.iter_mut() {
        if slot.is_none() {
            *slot = Some(window);
            return;
        }
    }

    panic!("Maximum number of windows reached");
}

// Handle incoming events
pub fn handle_event(event: GuiEvent) {
    let (mouse_x, mouse_y) = poll_mouse_position();
    let mut wins = WINDOWS.lock();
    for window_opt in wins.iter_mut() {
        if let Some(window) = window_opt {
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
            set_pixel(window.x + x, window.y + y, color, 0);
        }
    }
}

// Redraw all windows
pub fn render_windows() {
    let (mouse_x, mouse_y) = poll_mouse_position();
    let mut wins = WINDOWS.lock();
    for window_opt in wins.iter_mut() {
        if let Some(window) = window_opt {
            if in_window(mouse_x, mouse_y, window) {
                draw_title_bar(window, 0x3333FF);
                draw_border(window, 0x00FF00);
                fill_window(window, 0x000000);
            }
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

// Redraw entire screen
pub fn redraw_screen() {
    clear_screen();
    let wins = WINDOWS.lock();
    for window_opt in wins.iter() {
        if let Some(window) = window_opt {
            draw_window(window);
        }
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

// Dummy GuiEvent enum for completeness
#[derive(Clone, Copy)]
pub enum GuiEvent {
    MouseClick { x: usize, y: usize },
    MouseMove { x: usize, y: usize },
    KeyPress { key: char },
}
