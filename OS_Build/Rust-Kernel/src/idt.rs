//! Minimal IDT so faults don't triple-fault/reset.
//!
//! Stage2 brings us into long mode, but (by design) we haven't installed an IDT yet.
//! Any CPU exception (e.g. page fault) will otherwise cause a triple-fault and QEMU
//! will look like it is "rebooting" / flashing.

#![allow(dead_code)]

use core::arch::asm;
use core::ptr;

use crate::serial_write_str;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    off_lo: u16,
    sel: u16,
    ist: u8,
    type_attr: u8,
    off_mid: u16,
    off_hi: u32,
    zero: u32,
}

impl IdtEntry {
    const fn missing() -> Self {
        Self {
            off_lo: 0,
            sel: 0,
            ist: 0,
            type_attr: 0,
            off_mid: 0,
            off_hi: 0,
            zero: 0,
        }
    }

    fn set_handler(&mut self, handler: u64, selector: u16) {
        self.off_lo = handler as u16;
        self.off_mid = (handler >> 16) as u16;
        self.off_hi = (handler >> 32) as u32;

        // Use the *current* CS selector instead of assuming 0x08.
        self.sel = selector;

        // IST=0 (no dedicated stack yet)
        self.ist = 0;

        // Present | DPL=0 | Interrupt Gate (0xE)
        self.type_attr = 0x8E;
        self.zero = 0;
    }
}

#[repr(C, packed)]
struct Idtr {
    limit: u16,
    base: u64,
}

static mut IDT: [IdtEntry; 256] = [IdtEntry::missing(); 256];

/// Very small "catch-all" handler.
///
/// We don't attempt to return (no iretq), we just stop the CPU. This is enough
/// to prevent the reset loop and makes it obvious that we hit a fault.
#[no_mangle]
pub extern "C" fn isr_halt_forever() -> ! {
    // Best-effort serial message; if the fault is recurring you will at least
    // see this once before the CPU halts.
    serial_write_str("\n*** CPU EXCEPTION / IRQ: halted (IDT installed) ***\n");
    loop {
        unsafe {
            asm!("cli; hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

/// Install a minimal IDT that routes *all* vectors to `isr_halt_forever`.
pub fn init() {
    unsafe {
        let cs: u16;
        asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags));

        let handler = isr_halt_forever as usize as u64;
        for i in 0..256 {
            IDT[i].set_handler(handler, cs);
        }

        let idtr = Idtr {
            limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
            base: ptr::addr_of!(IDT) as u64,
        };

        asm!("lidt [{0}]", in(reg) &idtr, options(readonly, nostack, preserves_flags));
    }
}
