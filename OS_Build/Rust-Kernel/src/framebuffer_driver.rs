
use core::ptr;

pub struct Framebuffer {
    pub base_addr: *mut u8,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,            // bytes per row
    pub bytes_per_pixel: usize,  // usually 4 (RGBA)
}

static mut FRAMEBUFFER: Option<Framebuffer> = None;

pub fn init() {
    // Placeholder: in a real kernel, this would come from bootloader info (UEFI GOP, etc.)
    unsafe {
        FRAMEBUFFER = Some(Framebuffer {
            base_addr: 0xA000_0000 as *mut u8, // common legacy framebuffer base
            width: 1024,
            height: 768,
            pitch: 1024 * 4,
            bytes_per_pixel: 4,
        });
    }
    clear_screen(0x000000);
}

pub fn clear_screen(color: u32) {
    unsafe {
        if let Some(fb) = &FRAMEBUFFER {
            for y in 0..fb.height {
                for x in 0..fb.width {
                    set_pixel(x, y, color);
                }
            }
        }
    }
}

pub fn set_pixel(x: usize, y: usize, color: u32) {
    unsafe {
        if let Some(fb) = &FRAMEBUFFER {
            if x >= fb.width || y >= fb.height {
                return;
            }
            let offset = y * fb.pitch + x * fb.bytes_per_pixel;
            let pixel_ptr = fb.base_addr.add(offset) as *mut u32;
            ptr::write_volatile(pixel_ptr, color);
        }
    }
}

pub fn draw_rect(x: usize, y: usize, w: usize, h: usize, color: u32) {
    for yy in y..(y + h) {
        for xx in x..(x + w) {
            set_pixel(xx, yy, color);
        }
    }
}

pub fn commit() {
    // On real hardware, this may flush caches or signal VSync.
    // For now, no action is needed since it's direct memory-mapped.
}

pub fn info() -> Option<(usize, usize)> {
    unsafe {
        FRAMEBUFFER.as_ref().map(|fb| (fb.width, fb.height))
    }
}
