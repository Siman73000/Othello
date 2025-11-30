use core::arch::x86_64::_rdtsc;
use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::framebuffer_driver::{descriptor as fb_descriptor, Framebuffer};

const HOTPLUG_MIN_CYCLES: u64 = 25_000_000; // ~25ms @1GHz
const DEFAULT_FRAMEBUFFER_BASE: usize = 0xA000_0000;
const CLOCK_ENABLE: u32 = 0x1;

#[derive(Copy, Clone)]
pub struct HdmiRegisters {
    pub clock_base: usize,
    pub phy_base: usize,
    pub audio_base: usize,
    pub debug_base: usize,
    pub hotplug_reg: usize,
    pub pixel_format_reg: usize,
}

#[derive(Copy, Clone)]
pub struct HdmiDescriptor {
    pub framebuffer_base: usize,
    pub registers: HdmiRegisters,
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
    Bpp24,
    Bpp16,
}

#[derive(Copy, Clone)]
pub enum AudioFormat {
    PCM,
    AC3,
    DTS,
}

static HDMI_DESCRIPTOR: Mutex<Option<HdmiDescriptor>> = Mutex::new(None);
static HDMI_MODE: Mutex<Option<DisplayCapabilities>> = Mutex::new(None);
static HDMI_FB: Mutex<Option<FramebufferView>> = Mutex::new(None);
static LAST_HOTPLUG_CYCLE: AtomicU64 = AtomicU64::new(0);

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

fn default_descriptor() -> HdmiDescriptor {
    HdmiDescriptor {
        framebuffer_base: DEFAULT_FRAMEBUFFER_BASE,
        registers: HdmiRegisters {
            clock_base: 0x4000_0200,
            phy_base: 0x4000_0300,
            audio_base: 0x4000_0100,
            debug_base: 0x4000_0400,
            hotplug_reg: 0x4000_0404,
            pixel_format_reg: 0x4000_0410,
        },
        edid_block: None,
    }
}

pub fn configure_from_firmware(desc: HdmiDescriptor) {
    *HDMI_DESCRIPTOR.lock() = Some(desc);
}

fn active_descriptor() -> HdmiDescriptor {
    HDMI_DESCRIPTOR
        .lock()
        .as_ref()
        .copied()
        .unwrap_or_else(default_descriptor)
}

fn validate_descriptor(desc: &HdmiDescriptor) -> bool {
    desc.framebuffer_base != 0
        && desc.registers.clock_base != 0
        && desc.registers.phy_base != 0
        && desc.registers.audio_base != 0
}

fn framebuffer_from_boot() -> Option<FramebufferView> {
    // Prefer bootloader-provided framebuffer details when available to avoid assuming legacy modes.
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
    *HDMI_FB.lock() = view;
}

fn framebuffer_view() -> Option<FramebufferView> {
    HDMI_FB.lock().as_ref().copied()
}

pub fn hdmi_init() {
    let descriptor = active_descriptor();
    if !validate_descriptor(&descriptor) {
        log_error("HDMI descriptor missing required addresses\n");
        return;
    }

    // Reset cached framebuffer so hotplug reconfigure always uses fresh values
    set_framebuffer_view(None);

    setup_hdmi_clock(&descriptor.registers);
    configure_phy(&descriptor.registers);
    let display_capabilities = read_edid(descriptor.edid_block.as_ref());
    configure_display(&descriptor, display_capabilities);
}

pub fn read_edid(block: Option<&[u8; 128]>) -> DisplayCapabilities {
    if let Some(data) = block {
        if let Some(parsed) = parse_edid_block(data) {
            return parsed;
        }
    }
    // Fallback when EDID is missing or malformed
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

    // First detailed timing descriptor (bytes 54..71)
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
        _ => ColorDepth::Bpp24,
    };

    Some(DisplayCapabilities {
        resolution,
        color_depth,
    })
}

pub fn configure_display(descriptor: &HdmiDescriptor, capabilities: DisplayCapabilities) {
    *HDMI_MODE.lock() = Some(capabilities);
    set_timing_parameters(&descriptor.registers, capabilities.resolution);
    configure_pixel_format(&descriptor.registers, capabilities.color_depth);
    allocate_framebuffer(descriptor, capabilities);
}

pub fn allocate_framebuffer(descriptor: &HdmiDescriptor, capabilities: DisplayCapabilities) {
    // Prefer firmware-provided framebuffer when it satisfies the requested mode.
    if let Some(fb) = framebuffer_from_boot() {
        let (req_w, req_h) = resolution_dimensions(capabilities.resolution);
        let req_bpp = bytes_per_pixel(capabilities.color_depth);
        if fb.width >= req_w && fb.height >= req_h && fb.bytes_per_pixel >= req_bpp {
            set_framebuffer_view(Some(fb));
            return;
        } else {
            log_error("Boot framebuffer incompatible with HDMI mode, falling back to descriptor base\n");
        }
    }

    let (width, height) = resolution_dimensions(capabilities.resolution);
    let bytes_per_pixel = bytes_per_pixel(capabilities.color_depth);
    let pitch = width.saturating_mul(bytes_per_pixel);
    let len = pitch.saturating_mul(height);
    if descriptor.framebuffer_base == 0 || len == 0 {
        log_error("HDMI framebuffer parameters invalid\n");
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

pub fn draw_pixel(x: u32, y: u32, color: u32) {
    if let Some(fb) = framebuffer_view() {
        let pixel_width = fb.bytes_per_pixel;
        if x as usize >= fb.width || y as usize >= fb.height {
            return;
        }

        let offset = calculate_pixel_offset(x, y, &fb);
        if offset + pixel_width <= fb.len {
            let color_bytes = color.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(
                    color_bytes.as_ptr(),
                    fb.base.add(offset),
                    pixel_width,
                );
            }
        }
    }
}

pub fn clear_screen(color: u32) {
    if let Some(fb) = framebuffer_view() {
        let pixel_width = fb.bytes_per_pixel;
        let color_bytes = color.to_le_bytes();
        for row in 0..fb.height {
            let row_start = row.saturating_mul(fb.pitch);
            if row_start >= fb.len {
                break;
            }
            let row_slice_len = fb.pitch.min(fb.len.saturating_sub(row_start));
            for col in (0..row_slice_len).step_by(pixel_width) {
                if col + pixel_width > row_slice_len {
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

pub fn check_hotplug_status() -> bool {
    let now = unsafe { _rdtsc() };
    let last = LAST_HOTPLUG_CYCLE.load(Ordering::Relaxed);
    if now.saturating_sub(last) < HOTPLUG_MIN_CYCLES {
        return false;
    }

    LAST_HOTPLUG_CYCLE.store(now, Ordering::Relaxed);
    if hotplug_status() {
        hdmi_init();
        true
    } else {
        false
    }
}

pub fn configure_audio_stream(descriptor: &HdmiDescriptor, format: AudioFormat, sample_rate: u32) {
    format_audio_stream(&descriptor.registers, format);
    set_audio_sample_rate(&descriptor.registers, sample_rate);
}

pub fn set_audio_sample_rate(regs: &HdmiRegisters, sample_rate: u32) {
    unsafe {
        core::ptr::write_volatile(regs.audio_base as *mut u32, sample_rate);
    }
}

pub fn setup_hdmi_clock(regs: &HdmiRegisters) {
    unsafe {
        core::ptr::write_volatile(regs.clock_base as *mut u32, CLOCK_ENABLE);
        core::ptr::write_volatile(regs.clock_base.wrapping_add(0x04) as *mut u32, 148_500_000);
    }
}

pub fn configure_phy(regs: &HdmiRegisters) {
    unsafe {
        core::ptr::write_volatile(regs.phy_base as *mut u32, 0x1);
        core::ptr::write_volatile(regs.phy_base.wrapping_add(0x04) as *mut u32, 0x1234_5678);
    }
}

pub fn calculate_framebuffer_size(resolution: Resolution, color_depth: ColorDepth) -> usize {
    let (width, height) = resolution_dimensions(resolution);
    width * height * bytes_per_pixel(color_depth)
}

fn calculate_pixel_offset(x: u32, y: u32, fb: &FramebufferView) -> usize {
    (y as usize)
        .saturating_mul(fb.pitch)
        .saturating_add(x as usize * fb.bytes_per_pixel)
}

pub fn resolution_dimensions(res: Resolution) -> (usize, usize) {
    match res {
        Resolution::R1920x1080 => (1920, 1080),
        Resolution::R1280x720 => (1280, 720),
        Resolution::R640x480 => (640, 480),
    }
}

fn bytes_per_pixel(depth: ColorDepth) -> usize {
    match depth {
        ColorDepth::Bpp24 => 3,
        ColorDepth::Bpp16 => 2,
    }
}

pub fn log_error(msg: &str) {
    let descriptor = active_descriptor();
    unsafe {
        for byte in msg.bytes() {
            core::ptr::write_volatile(descriptor.registers.debug_base as *mut u8, byte);
        }
    }
}

pub fn set_timing_parameters(regs: &HdmiRegisters, res: Resolution) {
    let (h_total, v_total, h_sync, v_sync) = match res {
        Resolution::R1920x1080 => (2200, 1125, 44, 5),
        Resolution::R1280x720 => (1650, 750, 40, 5),
        Resolution::R640x480 => (800, 525, 96, 2),
    };

    unsafe {
        core::ptr::write_volatile(regs.clock_base.wrapping_add(0x10) as *mut u32, h_total as u32);
        core::ptr::write_volatile(regs.clock_base.wrapping_add(0x14) as *mut u32, v_total as u32);
        core::ptr::write_volatile(regs.clock_base.wrapping_add(0x18) as *mut u32, h_sync as u32);
        core::ptr::write_volatile(regs.clock_base.wrapping_add(0x1C) as *mut u32, v_sync as u32);
    }
}

pub fn configure_pixel_format(regs: &HdmiRegisters, depth: ColorDepth) {
    let value = match depth {
        ColorDepth::Bpp24 => 0x2,
        ColorDepth::Bpp16 => 0x1,
    };
    unsafe {
        core::ptr::write_volatile(regs.pixel_format_reg as *mut u32, value);
    }
}

pub fn hotplug_status() -> bool {
    let descriptor = active_descriptor();
    unsafe {
        let status = core::ptr::read_volatile(descriptor.registers.hotplug_reg as *const u32);
        status & 0x1 == 0x1
    }
}

pub fn format_audio_stream(regs: &HdmiRegisters, format: AudioFormat) {
    unsafe {
        let value = match format {
            AudioFormat::PCM => 0x0,
            AudioFormat::AC3 => 0x1,
            AudioFormat::DTS => 0x2,
        };
        core::ptr::write_volatile(regs.audio_base.wrapping_add(0x04) as *mut u32, value);
    }
}
