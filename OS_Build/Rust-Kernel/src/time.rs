#![allow(dead_code)]

use core::arch::asm;

/// Read Time-Stamp Counter (cycles). Useful for crude profiling / pacing.
#[inline]
pub fn rdtsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags));
    }
    ((hi as u64) << 32) | (lo as u64)
}

/// Hint to the CPU while spinning.
#[inline]
pub fn cpu_pause() {
    unsafe { asm!("pause", options(nomem, nostack, preserves_flags)); }
}

/// Crude busy-wait loop (cycle count is CPU-dependent).
#[inline]
pub fn spin(iter: u64) {
    for _ in 0..iter {
        cpu_pause();
    }
}
