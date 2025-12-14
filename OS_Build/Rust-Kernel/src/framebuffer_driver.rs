#![allow(dead_code)]
use core::ptr;

use crate::serial_write_str;

/// Raw pointer type for boot video info.
/// Stage2 commonly writes:
///   +0 width:u16, +2 height:u16, +4 bpp:u16, +6 fb_addr:u32
/// Some variants write:
///   +0 width:u16, +2 height:u16, +4 bpp:u16, +6 pitch:u16, +8 fb_addr:u64
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct BootVideoInfoRaw {
    pub width: u16,
    pub height: u16,
    pub bpp: u16,
    // remaining bytes are variant-specific
    pub rest: [u8; 10],
}

#[derive(Clone, Copy)]
pub struct Framebuffer {
    pub base_addr: *mut u8,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub bytes_per_pixel: usize,
}

static mut FB: Option<Framebuffer> = None;

#[inline]
unsafe fn read_u16_unaligned(p: *const u8) -> u16 {
    ptr::read_unaligned(p as *const u16)
}
#[inline]
unsafe fn read_u32_unaligned(p: *const u8) -> u32 {
    ptr::read_unaligned(p as *const u32)
}
#[inline]
unsafe fn read_u64_unaligned(p: *const u8) -> u64 {
    ptr::read_unaligned(p as *const u64)
}

fn is_plausible_fb(addr: u64) -> bool {
    // Common LFB is >= 0xE0000000 on QEMU VBE.
    // Also allow low addresses if identity-mapped.
    if addr == 0 { return false; }
    if addr & 0xFFF != 0 { return false; } // page aligned keeps us away from obvious garbage
    true
}

pub unsafe fn init_from_bootinfo(raw: *const BootVideoInfoRaw) {
    let base = raw as *const u8;

    let w = read_u16_unaligned(base.add(0)) as usize;
    let h = read_u16_unaligned(base.add(2)) as usize;
    let bpp = read_u16_unaligned(base.add(4)) as usize;

    // Try stage2 layout first: fb_addr32 at +6
    let fb32 = read_u32_unaligned(base.add(6)) as u64;

    // Try extended layout: pitch at +6, fb64 at +8
    let pitch16 = read_u16_unaligned(base.add(6)) as usize;
    let fb64 = read_u64_unaligned(base.add(8));

    let (fb_addr, pitch) = if is_plausible_fb(fb32) {
        (fb32, 0usize)
    } else if is_plausible_fb(fb64) {
        (fb64, pitch16)
    } else {
        // Last resort: accept fb32 even if not aligned, but log it.
        serial_write_str("FB WARN: bootinfo fb address looked implausible; using fb32 anyway.\n");
        (fb32, 0usize)
    };

    let bytes_per_pixel = (bpp / 8).max(1);
    let computed_pitch = w.saturating_mul(bytes_per_pixel);
    let pitch = if pitch != 0 { pitch } else { computed_pitch };

    // Log parsed values (helps debug reboot loops)
    serial_write_str("FB: init_from_bootinfo\n");

    FB = Some(Framebuffer {
        base_addr: fb_addr as usize as *mut u8,
        width: w,
        height: h,
        pitch,
        bytes_per_pixel,
    });
}

pub fn logical_width() -> usize { unsafe { FB.map(|f| f.width).unwrap_or(0) } }
pub fn logical_height() -> usize { unsafe { FB.map(|f| f.height).unwrap_or(0) } }

#[inline]
unsafe fn write_px(dst: *mut u8, bpp: usize, color: u32) {
    match bpp {
        4 => ptr::write_volatile(dst as *mut u32, color),
        3 => {
            // 0x00RRGGBB -> B G R
            ptr::write_volatile(dst.add(0), (color & 0xFF) as u8);
            ptr::write_volatile(dst.add(1), ((color >> 8) & 0xFF) as u8);
            ptr::write_volatile(dst.add(2), ((color >> 16) & 0xFF) as u8);
        }
        _ => {}
    }
}

pub fn clear(color: u32) {
    unsafe {
        if let Some(fb) = FB {
            for y in 0..fb.height {
                let row = fb.base_addr.add(y * fb.pitch);
                for x in 0..fb.width {
                    write_px(row.add(x * fb.bytes_per_pixel), fb.bytes_per_pixel, color);
                }
            }
        }
    }
}

pub fn fill_rect(x: usize, y: usize, w: usize, h: usize, color: u32) {
    unsafe {
        if let Some(fb) = FB {
            let x2 = (x + w).min(fb.width);
            let y2 = (y + h).min(fb.height);
            for yy in y..y2 {
                let row = fb.base_addr.add(yy * fb.pitch);
                for xx in x..x2 {
                    write_px(row.add(xx * fb.bytes_per_pixel), fb.bytes_per_pixel, color);
                }
            }
        }
    }
}

pub fn invert_rect(x: usize, y: usize, w: usize, h: usize) {
    unsafe {
        if let Some(fb) = FB {
            if fb.bytes_per_pixel != 4 { return; } // XOR only safe on 32bpp here
            let x2 = (x + w).min(fb.width);
            let y2 = (y + h).min(fb.height);
            for yy in y..y2 {
                let row = fb.base_addr.add(yy * fb.pitch);
                for xx in x..x2 {
                    let p = row.add(xx * 4) as *mut u32;
                    let v = ptr::read_volatile(p);
                    ptr::write_volatile(p, v ^ 0x00FF_FFFF);
                }
            }
        }
    }
}
