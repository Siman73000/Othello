
use core::ptr;

#[derive(Clone, Copy, Debug)]
pub struct Framebuffer {
    pub base_addr: *mut u8,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,            // bytes per row
    pub bytes_per_pixel: usize,  // usually 4 (RGBA)
}

#[derive(Clone, Copy, Debug)]
pub struct BootFramebuffer {
    pub base_addr: usize,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub bytes_per_pixel: usize,
}

#[derive(Debug)]
pub enum FramebufferError {
    MissingInfo,
    InvalidPitch,
    ZeroDimensions,
}

static mut FRAMEBUFFER: Option<Framebuffer> = None;

pub fn init() {
    // Placeholder: in a real kernel, this should be called with actual bootloader info.
    let _ = init_from_boot_info(BootFramebuffer {
        base_addr: 0xA000_0000, // common legacy framebuffer base
        width: 1024,
        height: 768,
        pitch: 1024 * 4,
        bytes_per_pixel: 4,
    });
}

pub fn init_from_boot_info(info: BootFramebuffer) -> Result<(), FramebufferError> {
    if info.base_addr == 0 {
        return Err(FramebufferError::MissingInfo);
    }
    if info.width == 0 || info.height == 0 || info.bytes_per_pixel == 0 {
        return Err(FramebufferError::ZeroDimensions);
    }
    if info.pitch < info.width.saturating_mul(info.bytes_per_pixel) {
        return Err(FramebufferError::InvalidPitch);
    }

    unsafe {
        FRAMEBUFFER = Some(Framebuffer {
            base_addr: info.base_addr as *mut u8,
            width: info.width,
            height: info.height,
            pitch: info.pitch,
            bytes_per_pixel: info.bytes_per_pixel,
        });
    }

    clear_screen(0x000000);
    Ok(())
}

pub fn clear_screen(color: u32) {
    unsafe {
        if let Some(fb) = &FRAMEBUFFER {
            let stride = fb.pitch;
            let bytes_per_pixel = fb.bytes_per_pixel;
            let row_span = fb.width * bytes_per_pixel;
            let color_bytes = color.to_le_bytes();

            for row in 0..fb.height {
                let row_start = fb.base_addr.add(row * stride);
                let row_slice = core::slice::from_raw_parts_mut(row_start, row_span);
                for chunk in row_slice.chunks_exact_mut(bytes_per_pixel) {
                    chunk.copy_from_slice(&color_bytes[..bytes_per_pixel]);
                }
            }
        }
    }
}

pub fn blit_rect(
    dst_x: usize,
    dst_y: usize,
    width: usize,
    height: usize,
    src_stride: usize,
    src: &[u8],
) {
    unsafe {
        if let Some(fb) = &FRAMEBUFFER {
            let bytes_per_pixel = fb.bytes_per_pixel;
            let row_len = width.saturating_mul(bytes_per_pixel);
            for row in 0..height {
                let dst_y_pos = dst_y + row;
                if dst_y_pos >= fb.height { break; }
                let src_offset = row.saturating_mul(src_stride);
                if src_offset + row_len > src.len() { break; }

                let dst_row_start = fb.base_addr.add(dst_y_pos * fb.pitch);
                if dst_x >= fb.width { continue; }
                let dst_offset = dst_x.saturating_mul(bytes_per_pixel);
                let dst_slice = core::slice::from_raw_parts_mut(dst_row_start.add(dst_offset), row_len.min(fb.pitch.saturating_sub(dst_offset)));
                let src_slice = &src[src_offset..src_offset + row_len.min(src.len() - src_offset)];
                let copy_len = dst_slice.len().min(src_slice.len());
                dst_slice[..copy_len].copy_from_slice(&src_slice[..copy_len]);
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
    unsafe {
        if let Some(fb) = &FRAMEBUFFER {
            let bytes_per_pixel = fb.bytes_per_pixel;
            let row_len = w.saturating_mul(bytes_per_pixel);
            let color_bytes = color.to_le_bytes();

            for row in 0..h {
                let dst_y_pos = y + row;
                if dst_y_pos >= fb.height { break; }
                if x >= fb.width { continue; }

                let dst_row_start = fb.base_addr.add(dst_y_pos * fb.pitch);
                let dst_offset = x.saturating_mul(bytes_per_pixel);
                let dst_slice = core::slice::from_raw_parts_mut(
                    dst_row_start.add(dst_offset),
                    row_len.min(fb.pitch.saturating_sub(dst_offset)),
                );

                for chunk in dst_slice.chunks_exact_mut(bytes_per_pixel) {
                    chunk.copy_from_slice(&color_bytes[..bytes_per_pixel]);
                }
            }
        }
    }
}

pub fn commit() {
    // On real hardware, this may flush caches or signal VSync.
    // For now, no action is needed since it's direct memory-mapped.
}

pub fn info() -> Option<(usize, usize)> {
    unsafe { FRAMEBUFFER.as_ref().map(|fb| (fb.width, fb.height)) }
}

pub fn descriptor() -> Option<Framebuffer> {
    unsafe { FRAMEBUFFER }
}
