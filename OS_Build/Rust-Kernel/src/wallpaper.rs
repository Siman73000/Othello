// src/wallpaper.rs
#![allow(dead_code)]

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::framebuffer_driver as fb;

// ============================================================
// Foxhole embedded image (RAW RGBA8888)
// ============================================================
//
// PNG cannot be used directly in the kernel.
// Convert once:
//
//   magick ..\wallpapers\fox.png -alpha on -depth 8 rgba:..\wallpapers\fox.rgba
//
const FOX_W: usize = 320;
const FOX_H: usize = 180;
static FOX_RGBA: &[u8] = include_bytes!("../wallpapers/fox.rgba");

// ============================================================
// Public API
// ============================================================

static CURRENT: AtomicUsize = AtomicUsize::new(0);

pub fn count() -> usize {
    WALLPAPERS.len()
}

pub fn current_index() -> usize {
    if WALLPAPERS.is_empty() {
        0
    } else {
        CURRENT.load(Ordering::Relaxed) % WALLPAPERS.len()
    }
}

pub fn current_name() -> &'static str {
    WALLPAPERS
        .get(current_index())
        .map(|w| w.name())
        .unwrap_or("None")
}

pub fn name_at(idx: usize) -> &'static str {
    WALLPAPERS.get(idx).map(|w| w.name()).unwrap_or("None")
}

pub fn set(idx: usize) {
    if !WALLPAPERS.is_empty() {
        CURRENT.store(idx % WALLPAPERS.len(), Ordering::Relaxed);
    }
}

pub fn next() {
    if WALLPAPERS.is_empty() {
        return;
    }
    let n = WALLPAPERS.len();
    let cur = CURRENT.load(Ordering::Relaxed) % n;
    CURRENT.store((cur + 1) % n, Ordering::Relaxed);
}

pub fn prev() {
    if WALLPAPERS.is_empty() {
        return;
    }
    let n = WALLPAPERS.len();
    let cur = CURRENT.load(Ordering::Relaxed) % n;
    CURRENT.store((cur + n - 1) % n, Ordering::Relaxed);
}

pub fn draw_fullscreen() {
    let sw = fb::width();
    let sh = fb::height();
    if sw == 0 || sh == 0 {
        return;
    }
    draw_region(0, 0, sw, sh);
}

pub fn draw_region(x: usize, y: usize, w: usize, h: usize) {
    let sw = fb::width();
    let sh = fb::height();
    if sw == 0 || sh == 0 {
        return;
    }

    // Clip region to screen
    let x0 = x.min(sw);
    let y0 = y.min(sh);
    let x1 = x0.saturating_add(w).min(sw);
    let y1 = y0.saturating_add(h).min(sh);
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    let wp = &WALLPAPERS[current_index()];

    for yy in y0..y1 {
        for xx in x0..x1 {
            let c = wp.sample(xx, yy, sw, sh);
            fb::set_pixel(xx, yy, c);
        }
    }
}

// ============================================================
// Wallpaper Sources
// ============================================================

#[derive(Copy, Clone)]
pub enum Wallpaper {
    Procedural {
        name: &'static str,
        sampler: fn(x: usize, y: usize, screen_w: usize, screen_h: usize) -> u32,
    },
    RawRgba {
        name: &'static str,
        w: usize,
        h: usize,
        rgba: &'static [u8],
    },
}

impl Wallpaper {
    #[inline(always)]
    pub fn name(&self) -> &'static str {
        match self {
            Wallpaper::Procedural { name, .. } => name,
            Wallpaper::RawRgba { name, .. } => name,
        }
    }

    #[inline(always)]
    pub fn sample(&self, x: usize, y: usize, screen_w: usize, screen_h: usize) -> u32 {
        match *self {
            Wallpaper::Procedural { sampler, .. } => sampler(x, y, screen_w, screen_h),
            Wallpaper::RawRgba { w, h, rgba, .. } => {
                sample_raw_rgba_stretch(rgba, w, h, x, y, screen_w, screen_h)
            }
        }
    }
}

// ============================================================
// Built-in wallpaper list (defaults restored + Foxhole)
// ============================================================

pub static WALLPAPERS: &[Wallpaper] = &[
    Wallpaper::Procedural {
        name: "Aurora",
        sampler: aurora_sampler,
    },
    Wallpaper::Procedural {
        name: "Midnight Grid",
        sampler: grid_sampler,
    },
    Wallpaper::RawRgba {
        name: "Foxhole",
        w: FOX_W,
        h: FOX_H,
        rgba: FOX_RGBA,
    },
];

// ============================================================
// Procedural samplers (0x00RRGGBB)
// ============================================================

fn aurora_sampler(x: usize, y: usize, w: usize, h: usize) -> u32 {
    let w = w.max(1);
    let h = h.max(1);

    let fx = (x as u32).saturating_mul(255) / (w as u32).saturating_sub(1).max(1);
    let fy = (y as u32).saturating_mul(255) / (h as u32).saturating_sub(1).max(1);

    let wave = ((x ^ (y * 3)) & 0x7F) as u32; // 0..127

    let r = (20 + (fx / 2) + (wave / 6)).min(255);
    let g = (30 + (fy / 2) + (wave / 3)).min(255);
    let b = (60 + (255u32.saturating_sub(fy) / 2) + (wave / 2)).min(255);

    (r << 16) | (g << 8) | b
}

fn grid_sampler(x: usize, y: usize, w: usize, h: usize) -> u32 {
    let w = w.max(1);
    let h = h.max(1);

    let fx = (x as u32).saturating_mul(255) / (w as u32).saturating_sub(1).max(1);
    let fy = (y as u32).saturating_mul(255) / (h as u32).saturating_sub(1).max(1);

    let mut r = (fx / 18).min(255);
    let mut g = (fy / 16).min(255);
    let mut b = (28 + (fx / 10)).min(255);

    let cell = 96usize;
    let line = 2usize;
    if (x % cell) < line || (y % cell) < line {
        r = (r + 40).min(255);
        g = (g + 55).min(255);
        b = (b + 90).min(255);
    }

    (r << 16) | (g << 8) | b
}

// ============================================================
// Raw RGBA stretch-scaling (nearest neighbor, no crop)
// RGBA8888 -> 0x00RRGGBB (alpha ignored)
// ============================================================

fn sample_raw_rgba_stretch(
    rgba: &'static [u8],
    src_w: usize,
    src_h: usize,
    dx: usize,
    dy: usize,
    dst_w: usize,
    dst_h: usize,
) -> u32 {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return 0;
    }

    let need = src_w.saturating_mul(src_h).saturating_mul(4);
    if rgba.len() < need {
        return 0;
    }

    let sx = (dx.saturating_mul(src_w) / dst_w).min(src_w - 1);
    let sy = (dy.saturating_mul(src_h) / dst_h).min(src_h - 1);

    let i = (sy.saturating_mul(src_w).saturating_add(sx)).saturating_mul(4);

    let r = rgba.get(i + 0).copied().unwrap_or(0) as u32;
    let g = rgba.get(i + 1).copied().unwrap_or(0) as u32;
    let b = rgba.get(i + 2).copied().unwrap_or(0) as u32;

    (r << 16) | (g << 8) | b
}
