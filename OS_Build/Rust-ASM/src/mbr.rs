#![no_std]
#![no_main]
#![feature(naked_functions)]

// Panic handler
use core::panic::PanicInfo;
use core::arch::naked_asm;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe { core::arch::asm!("hlt"); }
    }
}

// Entry point
#[no_mangle]
#[naked]
pub extern "C" fn start() {
    unsafe {
        naked_asm!(
            "cli",                      // Clear interrupts
            "mov [{boot_drive}], dl",   // Store boot drive in a static variable
            "mov bp, 0x9000",           // Setup stack pointer
            "mov sp, bp",
            "jmp real_mode_entry",      // Jump to real-mode entry
            boot_drive = sym BOOT_DRIVE,
        );
    }
}



// Boot drive storage
#[link_section = ".data"]
static mut BOOT_DRIVE: u8 = 0;

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text
    .global real_mode_entry
real_mode_entry:
    call load_kernel
    call switch_to_protected_mode
    jmp $
    "#);

global_asm!(
    r#"
    .section .text
    .global switch_to_protected_mode
switch_to_protected_mode:
    lgdt [gdt_descriptor]
    mov eax, cr0
    or eax, 0x1
    mov cr0, eax
    jmp 0x08:protected_mode_entry
    "#);

// Boot sector padding and signature
#[link_section = ".boot"]
#[used]
static BOOT_SECTOR_PADDING: [u8; 510 - 2] = [0; 510 - 2];

#[link_section = ".boot"]
#[used]
static BOOT_SIGNATURE: [u8; 2] = [0x55, 0xAA];
