#![no_std]
#![no_main]
#![feature(naked_functions)]

use core::arch::naked_asm;
use core::panic::PanicInfo;

// Kernel memory offset
static KERNEL_OFFSET: u16 = 0x1000;

// Messages
static MSG_DISK_ERROR: &[u8] = b"Disk read error!\n\0";
static MSG_SECTORS_ERROR: &[u8] = b"Sector mismatch error!\n\0";
static MSG_PANIC: &[u8] = b"Kernel panic!\n\0";

#[no_mangle]
#[naked]
pub extern "C" fn disk_load(_sectors: u8, _drive: u8) {
    unsafe {
        naked_asm!(
            .code16,
            "pusha",
            "push dx",
            "mov ah, 0x02",
            "mov al, dh",
            "mov cl, 0x02",
            "mov ch, 0x00",
            "mov dh, 0x00",
            "mov ax, 0x0000",
            "mov es, ax",
            "mov bx, {kernel_offset}",
            "int 0x13",
            "jc {disk_error}",
            "cmp al, dh",
            "jne {sectors_error}",
            "pop dx",
            "popa",
            "ret",
            disk_error = sym disk_error,
            sectors_error = sym sectors_error,
            kernel_offset = const KERNEL_OFFSET,
        );
    }
}

#[no_mangle]
#[naked]
extern "C" fn disk_error() -> ! {
    unsafe {
        naked_asm!(
            "mov ebx, {msg_disk_error}",
            "call print16",
            "jmp $",
            msg_disk_error = sym MSG_DISK_ERROR,
        );
    }
}

#[no_mangle]
#[naked]
extern "C" fn sectors_error() -> ! {
    unsafe {
        naked_asm!(
            "mov ebx, {msg_sectors_error}",
            "call print16",
            "jmp $",
            msg_sectors_error = sym MSG_SECTORS_ERROR,
        );
    }
}

use core::arch::asm;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    unsafe {
        asm!(
            "mov ebx, {msg_panic}",
            "call print16",
            msg_panic = sym MSG_PANIC,
        );
    }

    loop {
        unsafe {
            asm!("hlt");
        }
    }
}
