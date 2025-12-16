#![allow(dead_code)]
// src/portio.rs
// Minimal x86 port I/O

#[inline(always)]
pub unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nostack, preserves_flags));
}
#[inline(always)]
pub unsafe fn inb(port: u16) -> u8 {
    let mut v: u8;
    core::arch::asm!("in al, dx", in("dx") port, out("al") v, options(nostack, preserves_flags));
    v
}
#[inline(always)]
pub unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") val, options(nostack, preserves_flags));
}
#[inline(always)]
pub unsafe fn inw(port: u16) -> u16 {
    let mut v: u16;
    core::arch::asm!("in ax, dx", in("dx") port, out("ax") v, options(nostack, preserves_flags));
    v
}
#[inline(always)]
pub unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") val, options(nostack, preserves_flags));
}
#[inline(always)]
pub unsafe fn inl(port: u16) -> u32 {
    let mut v: u32;
    core::arch::asm!("in eax, dx", in("dx") port, out("eax") v, options(nostack, preserves_flags));
    v
}

/// tiny delay for ATA (400ns) via port 0x80
#[inline(always)]
pub unsafe fn io_wait() {
    outb(0x80, 0);
}
