#![no_std]
#![no_main]

use core::panic::PanicInfo;

/// Minimal panic handler: just halt the CPU forever.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)); }
    }
}

/// Kernel entry point. Stage 2 will jump here at 0x0010_0000.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    let vga = 0xb8000 as *mut u8;
    let msg = b"Othello kernel entry reached!";

    unsafe {
        // Clear the screen (80x25 text, 2 bytes per cell).
        for i in 0..(80 * 25 * 2) {
            core::ptr::write_volatile(vga.add(i), 0);
        }

        // Write message on the first line in white-on-black.
        for (i, &ch) in msg.iter().enumerate() {
            core::ptr::write_volatile(vga.add(i * 2), ch);
            core::ptr::write_volatile(vga.add(i * 2 + 1), 0x0F);
        }
    }

    // Halt forever so we don't fall into unmapped memory.
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)); }
    }
}
