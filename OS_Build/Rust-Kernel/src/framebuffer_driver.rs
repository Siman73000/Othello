#![allow(dead_code)]
use core::ptr;

use crate::serial_write_str;

/// Boot video info written by Stage2 at physical 0x0000_9000.
///
/// Supported layouts:
///  A) +0 width:u16, +2 height:u16, +4 bpp:u16, +6 fb_addr:u32
///  B) +0 width:u16, +2 height:u16, +4 bpp:u16, +6 pitch:u16, +8 fb_addr:u64
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct BootVideoInfoRaw {
    pub width: u16,
    pub height: u16,
    pub bpp: u16,
    // The rest is variant-specific; we read unaligned by offsets.
    pub _rest: [u8; 10],
}

#[derive(Clone, Copy, Debug)]
pub struct Framebuffer {
    pub base: *mut u8,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,            // bytes per scanline
    pub bytes_per_pixel: usize,  // 3 or 4
}

static mut FB: Option<Framebuffer> = None;

#[inline]
unsafe fn read_u16(p: *const u8) -> u16 { ptr::read_unaligned(p as *const u16) }
#[inline]
unsafe fn read_u32(p: *const u8) -> u32 { ptr::read_unaligned(p as *const u32) }
#[inline]
unsafe fn read_u64(p: *const u8) -> u64 { ptr::read_unaligned(p as *const u64) }

#[inline]
fn clamp_u32_to_byte(v: u32) -> u8 {
    let v = v & 0xFF;
    v as u8
}

pub fn is_ready() -> bool {
    unsafe { FB.is_some() }
}

/// Initialize framebuffer from Stage2 boot info.
pub unsafe fn init_from_bootinfo(info: *const BootVideoInfoRaw) -> bool {
    if info.is_null() { return false; }

    let base_ptr = info as *const u8;

    let width  = read_u16(base_ptr.add(0)) as usize;
    let height = read_u16(base_ptr.add(2)) as usize;
    let bpp    = read_u16(base_ptr.add(4)) as usize;

    let bytespp = match bpp {
        24 => 3,
        32 => 4,
        _ => 4,
    };

    // Heuristics: A uses u32 fb at +6; B uses pitch:u16 at +6 and u64 fb at +8
    let pitch_b = read_u16(base_ptr.add(6)) as usize;
    let fb_addr_a = read_u32(base_ptr.add(6)) as usize;
    let fb_addr_b = read_u64(base_ptr.add(8)) as usize;

    let (pitch, fb_addr) = if pitch_b != 0 && pitch_b < 32768 && fb_addr_b != 0 {
        (pitch_b, fb_addr_b)
    } else {
        (width.saturating_mul(bytespp), fb_addr_a)
    };

    // Plausibility checks
    let plausible = width >= 320 && width <= 8192
        && height >= 200 && height <= 8192
        && (bpp == 24 || bpp == 32)
        && pitch >= width.saturating_mul(bytespp)
        && fb_addr >= 0x0010_0000;

    let (w, h, pit, base) = if plausible {
        (width, height, pitch, fb_addr)
    } else {
        serial_write_str("FB: boot info not plausible; using fallback 1024x768x32 @ 0xE0000000.\n");
        (1024usize, 768usize, 1024usize * 4usize, 0xE000_0000usize)
    };

    FB = Some(Framebuffer {
        base: base as *mut u8,
        width: w,
        height: h,
        pitch: pit,
        bytes_per_pixel: bytespp,
    });

    // Debug print (hex) without formatting machinery: print width/height/bpp/base low.
    serial_write_str("FB: initialized. W=");
    serial_write_dec(w as u64);
    serial_write_str(" H=");
    serial_write_dec(h as u64);
    serial_write_str(" BPP=");
    serial_write_dec(bpp as u64);
    serial_write_str(" BASE=0x");
    serial_write_hex(base as u64);
    serial_write_str("\n");
    true
}

fn serial_write_dec(mut v: u64) {
    // minimal decimal writer
    let mut buf = [0u8; 20];
    let mut i = 0usize;
    if v == 0 {
        crate::serial::serial_write_byte(b'0');
        return;
    }
    while v != 0 && i < buf.len() {
        let d = (v % 10) as u8;
        buf[i] = b'0' + d;
        i += 1;
        v /= 10;
    }
    while i > 0 {
        i -= 1;
        crate::serial::serial_write_byte(buf[i]);
    }
}

fn serial_write_hex(mut v: u64) {
    let mut started = false;
    for i in (0..16).rev() {
        let nib = ((v >> (i*4)) & 0xF) as u8;
        if nib != 0 || started || i == 0 {
            started = true;
            let c = if nib < 10 { b'0' + nib } else { b'a' + (nib - 10) };
            crate::serial::serial_write_byte(c);
        }
    }
}

#[inline]
pub fn logical_width() -> usize  { unsafe { if let Some(f) = FB { f.width } else { 0 } } }
#[inline]
pub fn logical_height() -> usize { unsafe { if let Some(f) = FB { f.height } else { 0 } } }
#[inline]
pub fn width() -> usize { logical_width() }
#[inline]
pub fn height() -> usize { logical_height() }

#[inline]
fn fb() -> Framebuffer {
    unsafe { FB.expect("FB not initialized") }
}

#[inline]
pub fn set_pixel(x: usize, y: usize, color: u32) {
    unsafe {
        let f = fb();
        if x >= f.width || y >= f.height { return; }
        let off = y * f.pitch + x * f.bytes_per_pixel;
        let dst = f.base.add(off);
        let r = clamp_u32_to_byte(color >> 16);
        let g = clamp_u32_to_byte(color >> 8);
        let b = clamp_u32_to_byte(color);

        if f.bytes_per_pixel == 4 {
            // Little-endian: BB GG RR 00
            ptr::write_volatile(dst as *mut u32, (b as u32) | ((g as u32) << 8) | ((r as u32) << 16));
        } else {
            ptr::write_volatile(dst.add(0) as *mut u8, b);
            ptr::write_volatile(dst.add(1) as *mut u8, g);
            ptr::write_volatile(dst.add(2) as *mut u8, r);
        }
    }
}

#[inline]
pub fn get_pixel(x: usize, y: usize) -> u32 {
    unsafe {
        let f = fb();
        if x >= f.width || y >= f.height { return 0; }
        let off = y * f.pitch + x * f.bytes_per_pixel;
        let src = f.base.add(off);
        if f.bytes_per_pixel == 4 {
            ptr::read_volatile(src as *const u32) & 0x00FF_FFFF
        } else {
            let b = ptr::read_volatile(src.add(0) as *const u8) as u32;
            let g = ptr::read_volatile(src.add(1) as *const u8) as u32;
            let r = ptr::read_volatile(src.add(2) as *const u8) as u32;
            b | (g << 8) | (r << 16)
        }
    }
}

pub fn clear(color: u32) {
    let (w, h) = (logical_width(), logical_height());
    fill_rect(0, 0, w, h, color);
}

pub fn fill_rect(x: usize, y: usize, w: usize, h: usize, color: u32) {
    unsafe {
        let f = fb();
        let x2 = (x + w).min(f.width);
        let y2 = (y + h).min(f.height);
        for yy in y..y2 {
            let row_off = yy * f.pitch;
            for xx in x..x2 {
                let off = row_off + xx * f.bytes_per_pixel;
                let dst = f.base.add(off);
                let r = clamp_u32_to_byte(color >> 16);
                let g = clamp_u32_to_byte(color >> 8);
                let b = clamp_u32_to_byte(color);

                if f.bytes_per_pixel == 4 {
                    ptr::write_volatile(dst as *mut u32, (b as u32) | ((g as u32) << 8) | ((r as u32) << 16));
                } else {
                    ptr::write_volatile(dst.add(0) as *mut u8, b);
                    ptr::write_volatile(dst.add(1) as *mut u8, g);
                    ptr::write_volatile(dst.add(2) as *mut u8, r);
                }
            }
        }
    }
}

pub fn invert_rect(x: usize, y: usize, w: usize, h: usize) {
    let (sw, sh) = (logical_width(), logical_height());
    if sw == 0 || sh == 0 { return; }
    let x2 = (x + w).min(sw);
    let y2 = (y + h).min(sh);
    for yy in y..y2 {
        for xx in x..x2 {
            let c = get_pixel(xx, yy);
            set_pixel(xx, yy, (!c) & 0x00FF_FFFF);
        }
    }
}

/// Overlap-safe rectangular move within the framebuffer (pixel coords).
pub fn blit_move_rect(src_x: i32, src_y: i32, w: i32, h: i32, dst_x: i32, dst_y: i32) {
    unsafe {
        let f = fb();
        if w <= 0 || h <= 0 { return; }

        // Clamp to framebuffer bounds for both src and dst.
        let sx = src_x.clamp(0, f.width as i32);
        let sy = src_y.clamp(0, f.height as i32);
        let dx = dst_x.clamp(0, f.width as i32);
        let dy = dst_y.clamp(0, f.height as i32);

        let mut ww = w;
        let mut hh = h;

        ww = ww.min((f.width as i32 - sx).max(0));
        ww = ww.min((f.width as i32 - dx).max(0));
        hh = hh.min((f.height as i32 - sy).max(0));
        hh = hh.min((f.height as i32 - dy).max(0));

        if ww <= 0 || hh <= 0 { return; }

        let bpp = f.bytes_per_pixel;
        let row_bytes = ww as usize * bpp;

        // Copy order: vertical direction only. ptr::copy is memmove-safe for overlap.
        if dy > sy {
            for yy in (0..hh).rev() {
                let src_off = (sy + yy) as usize * f.pitch + sx as usize * bpp;
                let dst_off = (dy + yy) as usize * f.pitch + dx as usize * bpp;
                let src = f.base.add(src_off);
                let dst = f.base.add(dst_off);
                core::ptr::copy(src, dst, row_bytes);
            }
        } else {
            for yy in 0..hh {
                let src_off = (sy + yy) as usize * f.pitch + sx as usize * bpp;
                let dst_off = (dy + yy) as usize * f.pitch + dx as usize * bpp;
                let src = f.base.add(src_off);
                let dst = f.base.add(dst_off);
                core::ptr::copy(src, dst, row_bytes);
            }
        }
    }
}
