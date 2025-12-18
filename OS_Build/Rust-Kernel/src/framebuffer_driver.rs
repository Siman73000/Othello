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
    (v & 0xFF) as u8
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
        _  => 4,
    };

    // Heuristics:
//  - Layout C (current stage2.asm): u64 fb at +6 and pitch at +14
//  - Layout B (older): pitch:u16 at +6 and u64 fb at +8
//  - Layout A (oldest): u32 fb at +6 (pitch implied)
let fb_addr_c = read_u64(base_ptr.add(6)) as usize;
let pitch_c   = read_u16(base_ptr.add(14)) as usize;

let pitch_b   = read_u16(base_ptr.add(6)) as usize;
let fb_addr_a = read_u32(base_ptr.add(6)) as usize;
let fb_addr_b = read_u64(base_ptr.add(8)) as usize;

let (pitch, fb_addr) = if fb_addr_c != 0 && pitch_c != 0 && pitch_c < 65535 {
    (pitch_c, fb_addr_c)
} else if pitch_b != 0 && pitch_b < 32768 && fb_addr_b != 0 {
    (pitch_b, fb_addr_b)
} else {
    // Layout A fallback: pitch = width * bytespp
    ((width * bytespp) as usize, fb_addr_a)
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

    // Debug print (hex) without full formatting
    serial_write_str("FB: initialized. W=");
    serial_write_dec(w as u64);
    serial_write_str(" H=");
    serial_write_dec(h as u64);
    serial_write_str(" BPP=");
    serial_write_dec(bpp as u64);
    serial_write_str(" PITCH=");
    serial_write_dec(pit as u64);
    serial_write_str(" BASE=0x");
    serial_write_hex(base as u64);
    serial_write_str("\n");
    true
}

fn serial_write_dec(mut v: u64) {
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

fn serial_write_hex(v: u64) {
    let mut started = false;
    for i in (0..16).rev() {
        let nib = ((v >> (i * 4)) & 0xF) as u8;
        if nib != 0 || started || i == 0 {
            started = true;
            let c = if nib < 10 { b'0' + nib } else { b'a' + (nib - 10) };
            crate::serial::serial_write_byte(c);
        }
    }
}

#[inline]
pub fn width() -> usize  { unsafe { FB.map(|f| f.width).unwrap_or(0) } }
#[inline]
pub fn height() -> usize { unsafe { FB.map(|f| f.height).unwrap_or(0) } }

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
            // Little-endian: BB GG RR AA
            // Note: Some display paths treat the high byte as alpha. Use 0xFF to avoid "transparent" pixels.
            let pix = (b as u32) | ((g as u32) << 8) | ((r as u32) << 16) | (0xFFu32 << 24);
            ptr::write_volatile(dst as *mut u32, pix);
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
    let (w, h) = (width(), height());
    fill_rect(0, 0, w, h, color);
}

pub fn fill_rect(x: usize, y: usize, w: usize, h: usize, color: u32) {
    unsafe {
        let f = fb();
        let x2 = (x + w).min(f.width);
        let y2 = (y + h).min(f.height);
        let r = clamp_u32_to_byte(color >> 16);
        let g = clamp_u32_to_byte(color >> 8);
        let b = clamp_u32_to_byte(color);

        if f.bytes_per_pixel == 4 {
            // Little-endian: BB GG RR AA (AA=0xFF)
            let pix = (b as u32) | ((g as u32) << 8) | ((r as u32) << 16) | (0xFFu32 << 24);
            for yy in y..y2 {
                let row_off = yy * f.pitch;
                for xx in x..x2 {
                    let off = row_off + xx * 4;
                    let dst = f.base.add(off);
                    ptr::write_volatile(dst as *mut u32, pix);
                }
            }
        } else {
            for yy in y..y2 {
                let row_off = yy * f.pitch;
                for xx in x..x2 {
                    let off = row_off + xx * 3;
                    let dst = f.base.add(off);
                    ptr::write_volatile(dst.add(0) as *mut u8, b);
                    ptr::write_volatile(dst.add(1) as *mut u8, g);
                    ptr::write_volatile(dst.add(2) as *mut u8, r);
                }
            }
        }
    }
}

pub fn invert_rect(x: usize, y: usize, w: usize, h: usize) {
    let (sw, sh) = (width(), height());
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
///
/// This is used by the GUI for cheap window dragging (no full redraw needed).
pub fn blit_move_rect(src_x: i32, src_y: i32, w: i32, h: i32, dst_x: i32, dst_y: i32) {
    unsafe {
        let f = fb();
        if w <= 0 || h <= 0 { return; }

        // Clip both src and dst to framebuffer bounds, adjusting the other side equally.
        let mut sx = src_x;
        let mut sy = src_y;
        let mut dx = dst_x;
        let mut dy = dst_y;
        let mut ww = w;
        let mut hh = h;

        // If src is negative, shift into range (and shift dst equally)
        if sx < 0 { let sh = -sx; sx = 0; dx += sh; ww -= sh; }
        if sy < 0 { let sh = -sy; sy = 0; dy += sh; hh -= sh; }
        // If dst is negative, shift into range (and shift src equally)
        if dx < 0 { let sh = -dx; dx = 0; sx += sh; ww -= sh; }
        if dy < 0 { let sh = -dy; dy = 0; sy += sh; hh -= sh; }

        let fw = f.width as i32;
        let fh = f.height as i32;

        // Clip right/bottom edges
        ww = ww.min(fw - sx).min(fw - dx);
        hh = hh.min(fh - sy).min(fh - dy);

        if ww <= 0 || hh <= 0 { return; }

        let bpp = f.bytes_per_pixel as i32;
        let row_bytes = (ww * bpp) as usize;

        // Direction to avoid overwrite (memmove already handles overlap, but do it row-safe for caches)
        if dy > sy {
            for yy in (0..hh).rev() {
                let src_off = (sy + yy) as usize * f.pitch + (sx as usize * f.bytes_per_pixel);
                let dst_off = (dy + yy) as usize * f.pitch + (dx as usize * f.bytes_per_pixel);
                let src_ptr = f.base.add(src_off);
                let dst_ptr = f.base.add(dst_off);
                ptr::copy(src_ptr, dst_ptr, row_bytes);
            }
        } else {
            for yy in 0..hh {
                let src_off = (sy + yy) as usize * f.pitch + (sx as usize * f.bytes_per_pixel);
                let dst_off = (dy + yy) as usize * f.pitch + (dx as usize * f.bytes_per_pixel);
                let src_ptr = f.base.add(src_off);
                let dst_ptr = f.base.add(dst_off);
                ptr::copy(src_ptr, dst_ptr, row_bytes);
            }
        }
    }
}
