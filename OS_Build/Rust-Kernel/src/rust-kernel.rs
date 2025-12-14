#![no_std]
#![no_main]

use core::fmt::{self, Write};
use core::panic::PanicInfo;

// Submodules
mod serial;
mod keyboard;
mod mouse;
mod framebuffer_driver;
mod font;
mod gui;
mod shell;
mod net;
mod login;

// Re-export so other modules can `use crate::serial_write_str;`
pub use serial::serial_write_str;

// Serial fmt bridge
struct SerialWriter;
impl Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        crate::serial_write_str(s);
        Ok(())
    }
}
fn serial_write_fmt(args: fmt::Arguments) { let _ = SerialWriter.write_fmt(args); }

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    crate::serial_write_str("\n*** KERNEL PANIC ***\n");

    // Rust 1.91+: returns PanicMessage<'_>, not Option
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

    crate::serial_write_str("System halted.\n");
    loop {}
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    serial::serial_init();
    serial_write_str("Othello kernel: _start reached (long mode).\n");

    unsafe {
        // Stage2 writes boot video info at physical 0x9000.
        gui::init_from_bootloader(0x0000_9000 as *const framebuffer_driver::BootVideoInfoRaw);
    }

    mouse::mouse_init();
    shell::run_shell()
}
