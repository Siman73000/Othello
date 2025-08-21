#![no_main]
#![no_std]

use core::cell::RefCell;
use std::io;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
}

struct Window {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    buffer: Vec<u32>,
}

thread_local! {
    static WINDOWS: RefCell<Vec<Window>> = RefCell::new(Vec::new());
}

enum Event {
    MouseClick { x: usize, y: usize },
    MouseMove { x: usize, y: usize },
    KeyPress { key: char },
}

fn create_window(x: usize, y: usize, width: usize, height: usize, color: u32) -> Window {
    Window {
        x,
        y,
        width,
        height,
        buffer: vec![color; width * height],
    }
}

fn event_loop() {
    loop {
        if let Some(event) = poll_event() {
            handle_event(event);
        }
        redraw_screen();
    }
}

fn handle_event(event: Event) {
    match event {
        Event::MouseClick { x, y } => {
            for window in WINDOWS.borrow().iter() {
                if x >= window.x && x < window.x + window.width && y >= window.y && y < window.y + window.height {
                    let mut input_string = String::new();
                    io::stdin().read_line(&mut input_string).unwrap();
                    println!("Mouse clicked at ({}, {}) in window at ({}, {})", x, y, window.x, window.y);
                    for i in 0..window.buffer.len() {
                        window.buffer[i] = 0x000000;
                    }
                }
            }
        }
        Event::MouseMove { x, y } => {
            for window in WINDOWS.borrow().iter() {
                if x >= window.x && x < window.x + window.width && y >= window.y && y < window.y + window.height {
                    let mut input_string = String::new();
                    io::stdin().read_line(&mut input_string).unwrap();
                    println!("Mouse moved to ({}, {}) in window at ({}, {})", x, y, window.x, window.y);
                    for i in 0..window.buffer.len() {
                        window.buffer[i] = 0xFFFFFF;
                    }
                }
            }
        }
        Event::KeyPress { key } => {
            for window in WINDOWS.borrow().iter() {
                let mut input = String::new();
                io::stdin().read_line(&mut input_string).unwrap();
            }
        }
    }
}

fn draw_window(window: &Window) {
    for y in 0..window.height {
        for x in 0..window.width {
            let color = window.buffer[y * window.width + x];
            set_pixel(window.x + x, window.y + y, color);
        }
    }
}

fn redraw_screen() {
    clear_framebuffer();
    for window in WINDOWS.borrow().iter() {
        draw_window(window);
    }
    commit_framebuffer();
}