#![no_std]
#![no_main]

use core::fmt::{self, Write};
use core::panic::PanicInfo;

// Submodules
mod serial;
mod keyboard;
mod gui;
mod shell;
mod mouse;
mod net;
mod login;

// Re-exports so other modules can `use crate::...`
pub use serial::serial_write_str;
pub use keyboard::{keyboard_poll_scancode, scancode_to_ascii};

// -----------------------------------------------------------------------------
// serial::fmt bridge
// -----------------------------------------------------------------------------

struct SerialWriter;

impl Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        serial_write_str(s);
        Ok(())
    }
}

fn serial_write_fmt(args: fmt::Arguments) {
    let _ = SerialWriter.write_fmt(args);
}

// -----------------------------------------------------------------------------
// Panic handler (Rust 1.83 API)
// -----------------------------------------------------------------------------

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_write_str("\n*** KERNEL PANIC ***\n");

    // In Rust 1.83, `message()` returns `PanicMessage<'_>`, not an Option.
    let msg = info.message();
    serial_write_fmt(format_args!("Message: {msg}\n"));

    if let Some(loc) = info.location() {
        serial_write_fmt(format_args!(
            "Location: {}:{}:{}\n",
            loc.file(),
            loc.line(),
            loc.column()
        ));
    }

    serial_write_str("System halted.\n");

    // For now just spin; you can add `hlt` via inline asm later if you want.
    loop {}
}

// -----------------------------------------------------------------------------
// Kernel entry
// -----------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Initialize serial *first* so we can see early logs via `-serial stdio`.
    serial::serial_init();
    serial_write_str("Othello kernel: _start reached (long mode).\n");

    // Initialize GUI + mouse
    gui::init_desktop();
    mouse::mouse_init();

    // Optional login screen (stub):
    // login::show_login_screen();

    // Enter shell main loop
    shell::run_shell()
}
