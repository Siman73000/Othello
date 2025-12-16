#![no_std]
#![no_main]

#[macro_use]
extern crate alloc;

use core::arch::asm;
use core::fmt::{self, Write};
use core::panic::PanicInfo;

// Submodules
mod serial;
mod keyboard;
mod mouse;
mod idt;
mod framebuffer_driver;
mod font;
mod gui;
mod editor;
mod shell;
mod net;
mod login;
mod registry;
mod regedit;
mod time;

// Filesystem + persistence
mod heap;
mod portio;
mod crc32;
mod ata;
mod persist;
mod fs;
mod fs_cmds;

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
    // Stage2 may enter long mode with IF=1. Until we install an IDT/PIC,
    // any hardware IRQ or CPU exception will triple-fault -> reboot loop.
    unsafe { asm!("cli", options(nomem, nostack, preserves_flags)); }

    serial::serial_init();
    serial_write_str("Othello kernel: _start reached (long mode).\n");

    // Install a very small IDT that halts on *any* exception/IRQ.
    // This stops reboot-loops and gives us a stable place to debug.
    idt::init();

    // Stage2 writes boot video info at physical 0x9000.
    gui::init_from_bootloader(0x0000_9000 as *const framebuffer_driver::BootVideoInfoRaw);

    serial_write_str("KERNEL: after GUI init.\n");

    // Initialize registry state (in-memory for now).
    registry::init();

    // Filesystem (RAM overlay) + persistent backing store (IDE tail log)
    fs_cmds::init_cwd();
    if persist::init().is_ok() {
        let _ = persist::mount_into_ramfs();
    }

    // If persistence is empty, seed a default layout.
    {
        let fsg = fs::FS.lock();
        let has_etc = fsg.exists("/etc");
        drop(fsg);
        if !has_etc {
            fs::init_default_layout();
            let _ = persist::sync_dirty(); // optional: write initial layout
        }
    }


    serial_write_str("KERNEL: input init...\n");
    keyboard::keyboard_init();
    mouse::mouse_init();

    serial_write_str("KERNEL: entering shell.\n");
    shell::run_shell()
}
