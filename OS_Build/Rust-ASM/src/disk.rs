#![no_std]
#![no_main]
#![feature(asm_const, naked_functions)]

use core::arch::asm;

// Kernel memory offset
const KERNEL_OFFSET: u16 = 0x1000;

// Messages
const MSG_DISK_ERROR: &[u8] = b"Disk read error!\n\0";
const MSG_SECTORS_ERROR: &[u8] = b"Sector mismatch error!\n\0";

#[no_mangle]
pub extern "C" fn disk_load(sectors: u8, drive: u8) {
    unsafe {
        asm!(
            "pusha",                             // Save all registers
            "push dx",                           // Save DX
            "mov ah, 0x02",                      // BIOS: Read sectors function
            "mov al, dh",                        // AL = Number of sectors to read (DH)
            "mov cl, 0x02",                      // Start reading from sector 2
            "mov ch, 0x00",                      // Cylinder 0
            "mov dh, 0x00",                      // Head 0
            // Set ES:BX to 0x0000:KERNEL_OFFSET (physical address 0x1000)
            "mov ax, 0x0000",
            "mov es, ax",
            "mov bx, {kernel_offset}",
            // BIOS interrupt
            "int 0x13",
            "jc {disk_error}",                   // Jump to error handler if CF is set
            // Check if the correct number of sectors was read
            "cmp al, dh",
            "jne {sectors_error}",               // Jump to error handler if mismatch
            // Restore registers and return
            "pop dx",
            "popa",
            "ret",
            disk_error = sym disk_error,
            sectors_error = sym sectors_error,
            kernel_offset = const KERNEL_OFFSET,
            options(noreturn)
        );
    }
}

#[naked]
#[no_mangle]
extern "C" fn disk_error() -> ! {
    unsafe {
        asm!(
            "mov ebx, {msg_disk_error}",
            "call print16",
            "jmp $",
            msg_disk_error = sym MSG_DISK_ERROR.as_ptr(),
            options(noreturn)
        );
    }
}

#[naked]
#[no_mangle]
extern "C" fn sectors_error() -> ! {
    unsafe {
        asm!(
            "mov ebx, {msg_sectors_error}",
            "call print16",
            "jmp $",
            msg_sectors_error = sym MSG_SECTORS_ERROR.as_ptr(),
            options(noreturn)
        );
    }
}
