#![no_main]
#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::mem::size_of;
use core::ptr;

use uefi::boot::{self, AllocateType, MemoryType};
use uefi::cstr16;
use uefi::fs::FileSystem;
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;

/// Boot video info passed to the kernel (matches the kernel's
/// `framebuffer_driver::BootVideoInfoRaw` layout expectations).
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct BootVideoInfoRaw {
    width: u16,
    height: u16,
    bpp: u16,
    fb_addr: u64,
    pitch: u16,
}

/// Extra boot info written directly after `BootVideoInfoRaw`.
/// The kernel uses this to translate kernel VAs to PAs for DMA.
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct BootKernelMapRaw {
    magic: u32,
    version: u16,
    _reserved: u16,
    kernel_virt_base: u64,
    kernel_phys_base: u64,
    kernel_size: u64,
}

const BOOT_KERNEL_MAP_MAGIC: u32 = 0x4F54_484B; // 'OTHK'

// --- Minimal ELF64 definitions (enough for PT_LOAD)
#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Ehdr {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

const PT_LOAD: u32 = 1;

#[inline]
fn align_down(x: u64, a: u64) -> u64 {
    x & !(a - 1)
}
#[inline]
fn align_up(x: u64, a: u64) -> u64 {
    (x + (a - 1)) & !(a - 1)
}

// --- Paging helpers (x86_64)
//const PML4_ENTRIES: usize = 512;
const PAGE_SIZE: u64 = 4096;
const PAGE_2M: u64 = 2 * 1024 * 1024;

const PTE_P: u64 = 1 << 0;
const PTE_W: u64 = 1 << 1;
const PTE_PS: u64 = 1 << 7; // huge page

#[repr(align(4096))]
struct PageTable([u64; 512]);

unsafe fn alloc_pt() -> *mut PageTable {
    let p = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1)
        .expect("alloc page table")
        .as_ptr() as *mut PageTable;
    ptr::write_bytes(p as *mut u8, 0, 4096);
    p
}

unsafe fn map_2m(pd: *mut PageTable, v: u64, p: u64, flags: u64) {
    let idx = ((v >> 21) & 0x1ff) as usize;
    (*pd).0[idx] = (p & !(PAGE_2M - 1)) | flags | PTE_PS;
}

unsafe fn map_4k(pml4: *mut PageTable, v: u64, p: u64, flags: u64) {
    let pml4i = ((v >> 39) & 0x1ff) as usize;
    let pdpti = ((v >> 30) & 0x1ff) as usize;
    let pdi   = ((v >> 21) & 0x1ff) as usize;
    let pti   = ((v >> 12) & 0x1ff) as usize;

    // PML4 -> PDPT
    if (*pml4).0[pml4i] & PTE_P == 0 {
        let pdpt = alloc_pt() as u64;
        (*pml4).0[pml4i] = pdpt | PTE_P | PTE_W;
    }
    let pdpt = ((*pml4).0[pml4i] & !0xfff) as *mut PageTable;

    // PDPT -> PD
    if (*pdpt).0[pdpti] & PTE_P == 0 {
        let pd = alloc_pt() as u64;
        (*pdpt).0[pdpti] = pd | PTE_P | PTE_W;
    }
    let pd = ((*pdpt).0[pdpti] & !0xfff) as *mut PageTable;

    // PD -> PT (must be 4k table, not huge)
    if (*pd).0[pdi] & PTE_P == 0 || ((*pd).0[pdi] & PTE_PS) != 0 {
        let pt = alloc_pt() as u64;
        (*pd).0[pdi] = pt | PTE_P | PTE_W;
    }
    let pt = ((*pd).0[pdi] & !0xfff) as *mut PageTable;

    (*pt).0[pti] = (p & !0xfff) | flags;
}

/// Build a page table that:
///  - identity maps 0..4GiB using 2MiB pages
///  - overrides the kernel virtual range with 4KiB pages mapping to the
///    relocated physical kernel base
unsafe fn build_pagetables_identity4g_with_kernel_override(
    kernel_virt_base: u64,
    kernel_virt_end: u64,
    kernel_phys_base: u64,
) -> u64 {
    let pml4 = alloc_pt();

    // PDPT[0] and PDs for 0..4GiB
    let pdpt = alloc_pt();
    (*pml4).0[0] = (pdpt as u64) | PTE_P | PTE_W;

    for gi in 0..4usize { // 4 * 1GiB
        let pd = alloc_pt();
        (*pdpt).0[gi] = (pd as u64) | PTE_P | PTE_W;
        let base = (gi as u64) * (1u64 << 30);
        for mi in 0..512usize {
            let v = base + (mi as u64) * PAGE_2M;
            map_2m(pd, v, v, PTE_P | PTE_W);
        }
    }

    // Override kernel pages (4K) so VAs point to relocated physical.
    let k_start = align_down(kernel_virt_base, PAGE_SIZE);
    let k_end = align_up(kernel_virt_end, PAGE_SIZE);
    let mut v = k_start;
    while v < k_end {
        let p = kernel_phys_base + (v - k_start);
        map_4k(pml4, v, p, PTE_P | PTE_W);
        v += PAGE_SIZE;
    }

    pml4 as u64
}

unsafe fn jump_to_kernel_sysv_cr3(entry: u64, bootinfo_ptr: u64, stack_top: u64, cr3: u64) -> ! {
    use core::arch::asm;
    asm!(
        "cli",
        "mov cr3, {cr3}",
        "mov rsp, {stack}",
        "mov rbp, 0",
        "mov rdi, {bootinfo}",
        "jmp {entry}",
        entry = in(reg) entry,
        bootinfo = in(reg) bootinfo_ptr,
        stack = in(reg) stack_top,
        cr3 = in(reg) cr3,
        options(noreturn)
    );
}

#[entry]
fn main() -> Status {
    if let Err(e) = uefi::helpers::init() {
        return e.status();
    }

    log::info!("Othello UEFI loader: starting");

    // --- Read the kernel ELF from the boot volume ---
    let fs_proto = match boot::get_image_file_system(boot::image_handle()) {
        Ok(p) => p,
        Err(e) => {
            log::error!("get_image_file_system failed: {:?}", e);
            return Status::LOAD_ERROR;
        }
    };

    let mut fs = FileSystem::new(fs_proto);

    // FAT paths in UEFI use backslashes.
    let kernel: Vec<u8> = match fs.read(cstr16!("\\kernel.elf")) {
        Ok(b) => b,
        Err(e) => {
            log::error!("Failed to read \\kernel.elf: {:?}", e);
            return Status::NOT_FOUND;
        }
    };

    drop(fs);

    // --- Parse ELF header ---
    if kernel.len() < size_of::<Elf64Ehdr>() {
        log::error!("kernel.elf too small");
        return Status::LOAD_ERROR;
    }
    let eh: Elf64Ehdr = unsafe { ptr::read_unaligned(kernel.as_ptr() as *const Elf64Ehdr) };

    if eh.e_ident[0] != 0x7F || eh.e_ident[1] != b'E' || eh.e_ident[2] != b'L' || eh.e_ident[3] != b'F' {
        log::error!("kernel.elf: bad ELF magic");
        return Status::LOAD_ERROR;
    }
    if eh.e_ident[4] != 2 || eh.e_ident[5] != 1 {
        log::error!("kernel.elf: unsupported class/endianness");
        return Status::UNSUPPORTED;
    }
    if eh.e_machine != 0x3E {
        log::error!("kernel.elf: unsupported machine: {:#x}", eh.e_machine);
        return Status::UNSUPPORTED;
    }

    let phoff = eh.e_phoff as usize;
    let phentsz = eh.e_phentsize as usize;
    let phnum = eh.e_phnum as usize;

    if phoff + phentsz * phnum > kernel.len() {
        log::error!("kernel.elf: program headers out of range");
        return Status::LOAD_ERROR;
    }

    // First pass: compute linked virtual range of all PT_LOAD segments
    let mut k_min: u64 = u64::MAX;
    let mut k_max: u64 = 0;

    for i in 0..phnum {
        let off = phoff + i * phentsz;
        let ph: Elf64Phdr = unsafe { ptr::read_unaligned(kernel.as_ptr().add(off) as *const Elf64Phdr) };
        if ph.p_type != PT_LOAD || ph.p_memsz == 0 {
            continue;
        }
        k_min = k_min.min(ph.p_vaddr);
        k_max = k_max.max(ph.p_vaddr + ph.p_memsz);
    }

    if k_min == u64::MAX || k_max <= k_min {
        log::error!("kernel.elf: no loadable segments");
        return Status::LOAD_ERROR;
    }

    let k_min_aligned = align_down(k_min, PAGE_SIZE);
    let k_max_aligned = align_up(k_max, PAGE_SIZE);
    let k_size = k_max_aligned - k_min_aligned;
    let k_pages = (k_size / PAGE_SIZE) as usize;

    // Allocate one contiguous physical region for the kernel image.
    let kernel_phys_base = match boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, k_pages) {
        Ok(p) => p.as_ptr() as u64,
        Err(e) => {
            log::error!("alloc kernel image failed: {:?}", e);
            return Status::OUT_OF_RESOURCES;
        }
    };

    // Second pass: copy each PT_LOAD into the allocated physical region.
    for i in 0..phnum {
        let off = phoff + i * phentsz;
        let ph: Elf64Phdr = unsafe { ptr::read_unaligned(kernel.as_ptr().add(off) as *const Elf64Phdr) };
        if ph.p_type != PT_LOAD || ph.p_memsz == 0 {
            continue;
        }

        let filesz = ph.p_filesz as usize;
        let memsz = ph.p_memsz as usize;
        let file_off = ph.p_offset as usize;

        if file_off + filesz > kernel.len() {
            log::error!("segment {} out of file bounds", i);
            return Status::LOAD_ERROR;
        }

        let dst_phys = kernel_phys_base + (ph.p_vaddr - k_min_aligned);
        unsafe {
            let dst = dst_phys as *mut u8;
            ptr::copy_nonoverlapping(kernel.as_ptr().add(file_off), dst, filesz);
            if memsz > filesz {
                ptr::write_bytes(dst.add(filesz), 0, memsz - filesz);
            }
        }
    }

    let entry = eh.e_entry;

    // --- Setup GOP and build boot video info ---
    let gop_handle = match boot::get_handle_for_protocol::<GraphicsOutput>() {
        Ok(h) => h,
        Err(e) => {
            log::error!("No GOP handle: {:?}", e);
            return Status::UNSUPPORTED;
        }
    };

    let mut gop = match boot::open_protocol_exclusive::<GraphicsOutput>(gop_handle) {
        Ok(g) => g,
        Err(e) => {
            log::error!("open GOP failed: {:?}", e);
            return Status::UNSUPPORTED;
        }
    };

    let mode = gop.current_mode_info();
    let res = mode.resolution();
    let stride = mode.stride();
    let mut fb = gop.frame_buffer();

    let fb_base = fb.as_mut_ptr() as u64;
    let pitch_bytes = (stride as u64 * 4) as u16; // GOP uses 32bpp in OVMF/QEMU (BGRX)

    // Allocate 1 page for boot info (video + kernel map payload).
    let bi_ptr_u8 = match boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1) {
        Ok(p) => p.as_ptr() as *mut u8,
        Err(e) => {
            log::error!("alloc bootinfo failed: {:?}", e);
            return Status::OUT_OF_RESOURCES;
        }
    };

    unsafe {
        // BootVideoInfoRaw at offset 0
        ptr::write_unaligned(
            bi_ptr_u8 as *mut BootVideoInfoRaw,
            BootVideoInfoRaw {
                width: res.0 as u16,
                height: res.1 as u16,
                bpp: 32,
                fb_addr: fb_base,
                pitch: pitch_bytes,
            },
        );

        // BootKernelMapRaw at offset 16
        ptr::write_unaligned(
            bi_ptr_u8.add(16) as *mut BootKernelMapRaw,
            BootKernelMapRaw {
                magic: BOOT_KERNEL_MAP_MAGIC,
                version: 1,
                _reserved: 0,
                kernel_virt_base: k_min_aligned,
                kernel_phys_base,
                kernel_size: k_size,
            },
        );
    }

    // --- Build paging (identity 4GiB + kernel override) ---
    let cr3 = unsafe { build_pagetables_identity4g_with_kernel_override(k_min_aligned, k_max_aligned, kernel_phys_base) };

    // Kernel stack (64 KiB)
    let stack_pages = 16usize;
    let stack_base = match boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, stack_pages) {
        Ok(p) => p.as_ptr() as u64,
        Err(e) => {
            log::error!("alloc stack failed: {:?}", e);
            return Status::OUT_OF_RESOURCES;
        }
    };
    let stack_top = stack_base + (stack_pages as u64 * 4096);

    let bootinfo_ptr = bi_ptr_u8 as u64;

    // IMPORTANT: Drop GOP before ExitBootServices.
    drop(fb);
    drop(gop);

    log::info!(
        "Loaded kernel.elf; entry={:#x} kvirt=[{:#x}..{:#x}) kphys_base={:#x} cr3={:#x}",
        entry,
        k_min_aligned,
        k_max_aligned,
        kernel_phys_base,
        cr3
    );

    unsafe {
        let _mmap = boot::exit_boot_services(None);
        jump_to_kernel_sysv_cr3(entry, bootinfo_ptr, stack_top, cr3);
    }
}
