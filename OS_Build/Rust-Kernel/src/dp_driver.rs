use core::arch::x86_64::_rdtsc;
use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::framebuffer_driver::{descriptor as fb_descriptor, Framebuffer};

const HOTPLUG_MIN_CYCLES: u64 = 25_000_000; // ~25ms @1GHz
const DEFAULT_FRAMEBUFFER_BASE: usize = 0xA000_0000;
const DP_ENABLE: u32 = 0x1;

#[derive(Copy, Clone)]
pub struct DpRegisters {
    pub clock_base: usize,
    pub phy_base: usize,
    pub audio_base: usize,
    pub hotplug_reg: usize,
    pub pixel_format_reg: usize,
    pub debug_reg: usize,
}

#[derive(Copy, Clone)]
pub struct DpDescriptor {
    pub framebuffer_base: usize,
    pub registers: DpRegisters,
    pub edid_block: Option<[u8; 128]>,
}

#[derive(Copy, Clone)]
pub struct DisplayCapabilities {
    pub resolution: Resolution,
    pub color_depth: ColorDepth,
}

#[derive(Copy, Clone)]
pub enum Resolution {
    R1920x1080,
    R1280x720,
    R640x480,
}

#[derive(Copy, Clone)]
pub enum ColorDepth {
    Bpp8 = 1,
    Bpp16 = 2,
    Bpp24 = 3,
    Bpp32 = 4,
}

#[derive(Copy, Clone)]
pub enum AudioFormat {
    PCM,
    AC3,
    DTS,
}

static DP_DESCRIPTOR: Mutex<Option<DpDescriptor>> = Mutex::new(None);
static DP_MODE: Mutex<Option<DisplayCapabilities>> = Mutex::new(None);
static DP_FB: Mutex<Option<FramebufferView>> = Mutex::new(None);
static LAST_HOTPLUG: AtomicU64 = AtomicU64::new(0);

#[derive(Copy, Clone)]
struct FramebufferView {
    base: *mut u8,
    len: usize,
    width: usize,
    height: usize,
    pitch: usize,
    bytes_per_pixel: usize,
}

// Memory-mapped framebuffers are fixed physical regions supplied by firmware; we only cache their
// addresses and do not transfer ownership, so it's safe to mark this view as Send/Sync.
unsafe impl Send for FramebufferView {}
unsafe impl Sync for FramebufferView {}

fn default_descriptor() -> DpDescriptor {
    DpDescriptor {
        framebuffer_base: DEFAULT_FRAMEBUFFER_BASE,
        registers: DpRegisters {
            clock_base: 0x4000_0500,
            phy_base: 0x4000_0600,
            audio_base: 0x4000_0100,
            hotplug_reg: 0x4000_0640,
            pixel_format_reg: 0x4000_0644,
            debug_reg: 0x4000_0648,
        },
        edid_block: None,
    }
}

pub fn configure_from_firmware(desc: DpDescriptor) {
    *DP_DESCRIPTOR.lock() = Some(desc);
}

fn active_descriptor() -> DpDescriptor {
    DP_DESCRIPTOR
        .lock()
        .as_ref()
        .copied()
        .unwrap_or_else(default_descriptor)
}

fn validate_descriptor(desc: &DpDescriptor) -> bool {
    desc.framebuffer_base != 0
        && desc.registers.clock_base != 0
        && desc.registers.phy_base != 0
        && desc.registers.audio_base != 0
}

fn framebuffer_from_boot() -> Option<FramebufferView> {
    fb_descriptor().map(|fb: Framebuffer| FramebufferView {
        base: fb.base_addr,
        len: fb.pitch.saturating_mul(fb.height),
        width: fb.width,
        height: fb.height,
        pitch: fb.pitch,
        bytes_per_pixel: fb.bytes_per_pixel,
    })
}

fn set_framebuffer_view(view: Option<FramebufferView>) {
    *DP_FB.lock() = view;
}

fn framebuffer_view() -> Option<FramebufferView> {
    DP_FB.lock().as_ref().copied()
}

pub fn dp_init() {
    let descriptor = active_descriptor();
    if !validate_descriptor(&descriptor) {
        log_debug("DP descriptor missing required addresses\n");
        return;
    }

    set_framebuffer_view(None);

    setup_dp_clock(&descriptor.registers);
    configure_dp_phy(&descriptor.registers);
    let display_capabilities = read_dp_edid(descriptor.edid_block.as_ref());
    configure_dp_display(&descriptor, display_capabilities);
}

fn read_dp_edid(block: Option<&[u8; 128]>) -> DisplayCapabilities {
    if let Some(data) = block {
        if let Some(parsed) = parse_edid_block(data) {
            return parsed;
        }
    }
    DisplayCapabilities {
        resolution: Resolution::R1280x720,
        color_depth: ColorDepth::Bpp24,
    }
}

fn parse_edid_block(block: &[u8]) -> Option<DisplayCapabilities> {
    if block.len() < 128 {
        return None;
    }
    if block[0] != 0x00 || block[1] != 0xFF || block[2] != 0xFF || block[3] != 0xFF {
        return None;
    }

    let dtd = &block[54..72];
    let pixel_clock = u16::from_le_bytes([dtd[0], dtd[1]]);
    if pixel_clock == 0 {
        return None;
    }
    let h_active = (dtd[2] as u16 | ((dtd[4] as u16 & 0xF0) << 4)) as u32;
    let v_active = (dtd[5] as u16 | ((dtd[7] as u16 & 0xF0) << 4)) as u32;

    let resolution = match (h_active, v_active) {
        (1920, 1080) => Resolution::R1920x1080,
        (1280, 720) => Resolution::R1280x720,
        (640, 480) => Resolution::R640x480,
        _ => return None,
    };

    let color_depth = match (block[20] >> 4) & 0x07 {
        0x1 => ColorDepth::Bpp16,
        0x2 => ColorDepth::Bpp24,
        0x3 => ColorDepth::Bpp32,
        _ => ColorDepth::Bpp24,
    };

    Some(DisplayCapabilities {
        resolution,
        color_depth,
    })
}

pub fn configure_dp_phy(regs: &DpRegisters) {
    unsafe {
        core::ptr::write_volatile(regs.phy_base as *mut u32, DP_ENABLE);
        core::ptr::write_volatile(regs.phy_base.wrapping_add(0x04) as *mut u32, 0x1234_5678);
    }
}

pub fn configure_dp_display(descriptor: &DpDescriptor, capabilities: DisplayCapabilities) {
    *DP_MODE.lock() = Some(capabilities);
    set_dp_timing_parameters(&descriptor.registers, capabilities.resolution);
    configure_dp_pixel_format(&descriptor.registers, capabilities.color_depth);
    allocate_framebuffer(descriptor, capabilities);
}

fn allocate_framebuffer(descriptor: &DpDescriptor, capabilities: DisplayCapabilities) {
    if let Some(fb) = framebuffer_from_boot() {
        let (req_w, req_h) = match capabilities.resolution {
            Resolution::R1920x1080 => (1920usize, 1080usize),
            Resolution::R1280x720 => (1280, 720),
            Resolution::R640x480 => (640, 480),
        };
        let req_bpp = bytes_per_pixel(capabilities.color_depth);
        if fb.width >= req_w && fb.height >= req_h && fb.bytes_per_pixel >= req_bpp {
            set_framebuffer_view(Some(fb));
            return;
        } else {
            log_debug("Boot framebuffer incompatible with DP mode, falling back to descriptor base\n");
        }
    }

    let (width, height) = match capabilities.resolution {
        Resolution::R1920x1080 => (1920usize, 1080usize),
        Resolution::R1280x720 => (1280, 720),
        Resolution::R640x480 => (640, 480),
    };

    let bytes_per_pixel = bytes_per_pixel(capabilities.color_depth);
    let pitch = width.saturating_mul(bytes_per_pixel);
    let len = pitch.saturating_mul(height);
    if descriptor.framebuffer_base == 0 || len == 0 {
        log_debug("DP framebuffer parameters invalid\n");
        return;
    }

    let view = FramebufferView {
        base: descriptor.framebuffer_base as *mut u8,
        len,
        width,
        height,
        pitch,
        bytes_per_pixel,
    };
    set_framebuffer_view(Some(view));
}

fn draw_pixel(x: u32, y: u32, color: u32) {
    if let Some(fb) = framebuffer_view() {
        let pixel_width = fb.bytes_per_pixel;
        if x as usize >= fb.width || y as usize >= fb.height {
            return;
        }

        let offset = calculate_pixel_offset(x, y, &fb);
        if offset + pixel_width <= fb.len {
            let color_bytes = color.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(color_bytes.as_ptr(), fb.base.add(offset), pixel_width);
            }
        }
    }
}

fn clear_screen(color: u32) {
    if let Some(fb) = framebuffer_view() {
        let pixel_width = fb.bytes_per_pixel;
        let color_bytes = color.to_le_bytes();
        for row in 0..fb.height {
            let row_start = row.saturating_mul(fb.pitch);
            if row_start >= fb.len {
                break;
            }
            let row_len = fb.pitch.min(fb.len.saturating_sub(row_start));
            for col in (0..row_len).step_by(pixel_width) {
                if col + pixel_width > row_len {
                    break;
                }
                unsafe {
                    ptr::copy_nonoverlapping(
                        color_bytes.as_ptr(),
                        fb.base.add(row_start + col),
                        pixel_width,
                    );
                }
            }
        }
    }
}

fn check_dp_hotplug_status() -> bool {
    let now = unsafe { _rdtsc() };
    let last = LAST_HOTPLUG.load(Ordering::Relaxed);
    if now.saturating_sub(last) < HOTPLUG_MIN_CYCLES {
        return false;
    }

    LAST_HOTPLUG.store(now, Ordering::Relaxed);
    if dp_hotplug_status() {
        dp_init();
        return true;
    }
    false
}

pub fn configure_dp_pixel_format(regs: &DpRegisters, color_depth: ColorDepth) {
    let value = bytes_per_pixel(color_depth) as u32;
    unsafe {
        core::ptr::write_volatile(regs.pixel_format_reg as *mut u32, value);
    }
}

fn check_dp_debug() -> bool {
    let descriptor = active_descriptor();
    unsafe {
        let value = core::ptr::read_volatile(descriptor.registers.debug_reg as *const u32);
        (value & 0x1) == 0x1
    }
}

fn dp_hotplug_status() -> bool {
    let descriptor = active_descriptor();
    unsafe {
        let value = core::ptr::read_volatile(descriptor.registers.hotplug_reg as *const u32);
        (value & 0x1) == 0x1
    }
}

pub fn configure_dp_audio_stream(descriptor: &DpDescriptor, format: AudioFormat, sample_rate: u32) {
    format_dp_audio_stream(&descriptor.registers, format);
    set_dp_audio_sample_rate(&descriptor.registers, sample_rate);
}

pub fn set_dp_audio_sample_rate(regs: &DpRegisters, sample_rate: u32) {
    unsafe {
        core::ptr::write_volatile(regs.audio_base as *mut u32, sample_rate);
    }
}

pub fn format_dp_audio_stream(regs: &DpRegisters, format: AudioFormat) {
    unsafe {
        let value = match format {
            AudioFormat::PCM => 0x0,
            AudioFormat::AC3 => 0x1,
            AudioFormat::DTS => 0x2,
        };
        core::ptr::write_volatile(regs.audio_base.wrapping_add(0x04) as *mut u32, value);
    }
}

pub fn set_dp_timing_parameters(regs: &DpRegisters, resolution: Resolution) {
    const DP_H_TOTAL_OFF: usize = 0x10;
    const DP_V_TOTAL_OFF: usize = 0x14;
    const DP_H_SYNC_OFF: usize = 0x18;
    const DP_V_SYNC_OFF: usize = 0x1C;

    let (h_total, v_total, h_sync, v_sync) = match resolution {
        Resolution::R1920x1080 => (2200, 1125, 44, 5),
        Resolution::R1280x720 => (1650, 750, 40, 5),
        Resolution::R640x480 => (800, 525, 96, 2),
    };

    unsafe {
        core::ptr::write_volatile(regs.clock_base.wrapping_add(DP_H_TOTAL_OFF) as *mut u32, h_total as u32);
        core::ptr::write_volatile(regs.clock_base.wrapping_add(DP_V_TOTAL_OFF) as *mut u32, v_total as u32);
        core::ptr::write_volatile(regs.clock_base.wrapping_add(DP_H_SYNC_OFF) as *mut u32, h_sync as u32);
        core::ptr::write_volatile(regs.clock_base.wrapping_add(DP_V_SYNC_OFF) as *mut u32, v_sync as u32);
    }
}

fn setup_dp_clock(regs: &DpRegisters) {
    unsafe {
        core::ptr::write_volatile(regs.clock_base as *mut u32, DP_ENABLE);
        core::ptr::write_volatile(regs.clock_base.wrapping_add(0x04) as *mut u32, 148_500_000);
    }
}

fn calculate_framebuffer_size(resolution: Resolution, color_depth: ColorDepth) -> usize {
    let (width, height) = match resolution {
        Resolution::R1920x1080 => (1920, 1080),
        Resolution::R1280x720 => (1280, 720),
        Resolution::R640x480 => (640, 480),
    };

    width * height * bytes_per_pixel(color_depth)
}

fn calculate_pixel_offset(x: u32, y: u32, fb: &FramebufferView) -> usize {
    (y as usize)
        .saturating_mul(fb.pitch)
        .saturating_add(x as usize * fb.bytes_per_pixel)
}

fn bytes_per_pixel(depth: ColorDepth) -> usize {
    depth as usize
}

fn log_debug(msg: &str) {
    let descriptor = active_descriptor();
    unsafe {
        for byte in msg.bytes() {
            core::ptr::write_volatile(descriptor.registers.debug_reg as *mut u8, byte);
        }
    }
}
