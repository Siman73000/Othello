#![allow(dead_code)]

// src/bootinfo.rs
//
// Boot info helpers shared between the UEFI loader and kernel.
//
// The UEFI loader passes the first argument (RDI) as a pointer to
// `framebuffer_driver::BootVideoInfoRaw`. The loader allocates a whole 4KiB
// page for this, so we store additional boot-time data directly after the
// video struct (at offset 16).

use core::ptr;

use crate::framebuffer_driver::BootVideoInfoRaw;

/// Magic written by the UEFI loader to indicate that the kernel-map payload is present.
pub const BOOT_KERNEL_MAP_MAGIC: u32 = 0x4F54_484B; // 'OTHK'

/// Extra boot info written at (bootinfo_ptr + 16).
///
/// This lets the kernel translate kernel virtual addresses to physical addresses
/// for DMA (RTL8139 RBSTART/TSAD) when the UEFI loader relocates the kernel.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct BootKernelMapRaw {
    pub magic: u32,
    pub version: u16,
    pub _reserved: u16,

    pub kernel_virt_base: u64,
    pub kernel_phys_base: u64,
    pub kernel_size: u64,
}

static mut BOOTINFO_PTR: *const BootVideoInfoRaw = ptr::null();

/// Must be called once at the start of `_start()`.
pub fn init(bootinfo: *const BootVideoInfoRaw) {
    unsafe { BOOTINFO_PTR = bootinfo; }
}

#[inline]
pub fn boot_video_ptr() -> *const BootVideoInfoRaw {
    unsafe { BOOTINFO_PTR }
}

#[inline]
fn bootinfo_base_u8() -> *const u8 {
    boot_video_ptr() as *const u8
}

/// Returns the kernel mapping payload written by the UEFI loader (if present).
pub fn kernel_map() -> Option<BootKernelMapRaw> {
    let base = bootinfo_base_u8();
    if base.is_null() {
        return None;
    }
    unsafe {
        let km = ptr::read_unaligned(base.add(16) as *const BootKernelMapRaw);
        if km.magic == BOOT_KERNEL_MAP_MAGIC { Some(km) } else { None }
    }
}

/// Translate a virtual address to a physical address for DMA.
///
/// - If the UEFI loader provided a kernel map, translate within the kernel's
///   linked virtual range to the relocated physical base.
/// - Otherwise, assume identity mapping.
pub fn virt_to_phys(vaddr: u64) -> u64 {
    if let Some(km) = kernel_map() {
        let start = km.kernel_virt_base;
        let end = start.wrapping_add(km.kernel_size);
        if vaddr >= start && vaddr < end {
            return km.kernel_phys_base.wrapping_add(vaddr - start);
        }
    }
    vaddr
}

/// Convenience for programming 32-bit DMA registers (RTL8139 is 32-bit DMA).
pub fn virt_to_phys_u32(vaddr: u64) -> Option<u32> {
    let p = virt_to_phys(vaddr);
    if p <= (u32::MAX as u64) { Some(p as u32) } else { None }
}
