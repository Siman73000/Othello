#![no_std]
#![no_main]
#![feature(asm_const, naked_functions)]

use core::arch::asm;

// Constants
const KERNEL_OFFSET: u16 = 0x1000;

// Messages
const MSG_16BIT_MODE: &[u8] = b"Started in 16-bit Real Mode\n\0";
const MSG_32BIT_MODE: &[u8] = b"Landed in 32-bit Protected Mode\n\0";
const MSG_LOAD_KERNEL: &[u8] = b"Loading kernel into memory\n\0";

// Boot drive storage
static mut BOOT_DRIVE: u8 = 0;

#[naked]
#[no_mangle]
pub extern "C" fn start() -> ! {
    unsafe {
        asm!(
            // Save boot drive number
            "mov [{boot_drive}], dl",
            // Set up stack
            "mov bp, 0x9000",
            "mov sp, bp",
            // Print message
            "mov bx, {msg_16bit}",
            "call print16",
            "call print16_nl",
            // Load kernel
            "call load_kernel",
            // Switch to 32-bit mode
            "call switchto32bit",
            // Infinite loop
            "jmp $",
            boot_drive = sym BOOT_DRIVE,
            msg_16bit = sym MSG_16BIT_MODE.as_ptr(),
            options(noreturn)
        );
    }
}

#[no_mangle]
pub extern "C" fn load_kernel() {
    unsafe {
        asm!(
            // Print message
            "mov bx, {msg_load_kernel}",
            "call print16",
            "call print16_nl",
            // Load kernel
            "mov bx, {kernel_offset}",
            "mov edx, 32",
            "mov dl, [{boot_drive}]",
            "call disk_load",
            msg_load_kernel = sym MSG_LOAD_KERNEL.as_ptr(),
            kernel_offset = const KERNEL_OFFSET,
            boot_drive = sym BOOT_DRIVE,
            options(noreturn)
        );
    }
}

#[naked]
#[no_mangle]
pub extern "C" fn begin_32bit() -> ! {
    unsafe {
        asm!(
            // Print message
            "mov ebx, {msg_32bit}",
            "call print32",
            // Jump to kernel
            "call {kernel_offset}",
            "jmp $",
            msg_32bit = sym MSG_32BIT_MODE.as_ptr(),
            kernel_offset = const KERNEL_OFFSET,
            options(noreturn)
        );
    }
}

// BIOS Parameter Block (BPB) padding to 510 bytes
#[link_section = ".boot"]
#[used]
static BOOT_SECTOR_PADDING: [u8; 510] = [0; 510 - 2]; // Subtract space for boot signature

// Boot sector signature
#[link_section = ".boot"]
#[used]
static BOOT_SIGNATURE: [u8; 2] = [0x55, 0xAA];


/* 

mod helpers {
    pub mod utils; // Declare the utils module inside the helpers folder
}

fn main() {
    helpers::utils::print_message(); // Call the function
}

*/