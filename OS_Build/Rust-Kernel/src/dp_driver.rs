use x86_64::instructions::port::Port;
use core::ptr::read_volatile;

const FRAMEBUFFER_BASE: usize = 0xA0000000;
const AUDIO_SAMPLE_RATE_REG: usize = 0x4000_0100;
const DP_CLOCK_BASE: usize = 0x4000_0500;
const DP_ENABLE: u32 = 0x1;
const DP_FREQUENCY_REG: usize = DP_CLOCK_BASE + 0x04;
const DP_PHY_BASE: usize = 0x4000_0600;
const DP_PHY_CONFIG_REG: usize = DP_PHY_BASE + 0x04;
const DP_DEBUG_OUTPUT_BASE: usize = 0x1234;
static mut FRAMEBUFFER: Option<&mut [u8]> = None;

struct DisplayCapabilities {
    resolution: Resolution,
    color_depth: ColorDepth,
}

enum Resolution {
    R1920x1080,
    R1280x720,
    R640x480,
}

pub enum ColorDepth {
    Bpp8  = 0,
    Bpp16 = 1,
    Bpp24 = 2,
    Bpp32 = 3,
}

enum AudioFormat {
    PCM,
    AC3,
    DTS,
}

pub fn dp_init() {
    setup_dp_clock();
    configure_dp_phy();
    let display_capabilities = read_dp_edid();
    configure_dp_display(display_capabilities);
}

fn read_dp_edid() -> DisplayCapabilities {
    DisplayCapabilities {
        resolution: Resolution::R1920x1080,
        color_depth: ColorDepth::Bpp24,
    }
}

pub fn configure_dp_phy() {
    unsafe {
        core::ptr::write_volatile(DP_PHY_BASE as *mut u32, DP_ENABLE);
        core::ptr::write_volatile(DP_PHY_CONFIG_REG as *mut u32, 0x12345678);
    }
}

pub fn configure_dp_display(capabilities: DisplayCapabilities) {
    set_dp_timing_parameters(capabilities.resolution);
    configure_dp_pixel_format(capabilities.color_depth);
}

fn allocate_framebuffer(resolution: Resolution, color_depth: ColorDepth) {
    let size = calculate_framebuffer_size(resolution, color_depth);
    unsafe {
        FRAMEBUFFER = Some(core::slice::from_raw_parts_mut(FRAMEBUFFER_BASE as *mut u8, size));
    }
}

fn draw_pixel(x: u32, y: u32, color: u32) {
    let offset = calculate_pixel_offset(x, y);
    unsafe {
        if let Some(buffer) = FRAMEBUFFER.as_mut() {
            buffer[offset..offset + 4].copy_from_slice(&color.to_le_bytes());
        }
    }
}

fn clear_screen(color: u32) {
    unsafe {
        if let Some(buffer) = FRAMEBUFFER.as_mut(){
            for chunk in buffer.chunks_exact_mut(4) {
                chunk.copy_from_slice(&color.to_le_bytes());
            }
        }
    }
}

fn check_dp_hotplug_status() -> bool {
    if dp_hotplug_status() {
        dp_init();
        return true;
    }
    return false;
}

const DP_PIXEL_FORMAT_REG: u16 = 0x60; // Replace with actual port address

pub fn configure_dp_pixel_format(color_depth: ColorDepth) {
    // Safety: writing directly to hardware port
    unsafe {
        let mut dp_port: Port<u8> = Port::new(DP_PIXEL_FORMAT_REG);
        dp_port.write(color_depth as u8);
    }
}

fn check_dp_debug() -> bool {
    unsafe {
        // Convert the usize address into a pointer
        let ptr = DP_DEBUG_OUTPUT_BASE as *const u32;
        let value = read_volatile(ptr); // Read from hardware
        (value & 0x1) == 0x1
    }
}

fn dp_hotplug_status() -> bool {
    unsafe {
        let ptr = DP_DEBUG_OUTPUT_BASE as *const u32; // cast usize to pointer
        let value = read_volatile(ptr);
        if value & 0x1 == 0x1 {
            return true;
        }
    }
    false
}

pub fn configure_dp_audio_stream(format: AudioFormat, sample_rate: u32) {
    format_dp_audio_stream(format);
    set_dp_audio_sample_rate(sample_rate);
}

pub fn set_dp_audio_sample_rate(sample_rate: u32) {
    unsafe {
        core::ptr::write_volatile(AUDIO_SAMPLE_RATE_REG as *mut u32, sample_rate);
    }
}

pub fn format_dp_audio_stream(format: AudioFormat) {
    unsafe {
        let reg_ptr = AUDIO_SAMPLE_RATE_REG as *mut u32;
        let value = match format {
            AudioFormat::PCM => 0x0,
            AudioFormat::AC3 => 0x1,
            AudioFormat::DTS => 0x2,
        };
        core::ptr::write_volatile(reg_ptr, value);
    }
}

pub fn set_dp_timing_parameters(resolution: Resolution) {
    // Base addresses for horizontal and vertical timing registers (example values)
    const DP_H_TOTAL_REG: usize = DP_CLOCK_BASE + 0x10;
    const DP_V_TOTAL_REG: usize = DP_CLOCK_BASE + 0x14;
    const DP_H_SYNC_REG: usize = DP_CLOCK_BASE + 0x18;
    const DP_V_SYNC_REG: usize = DP_CLOCK_BASE + 0x1C;

    // Timing parameters (H_total, V_total, H_sync, V_sync)
    let (h_total, v_total, h_sync, v_sync) = match resolution {
        Resolution::R1920x1080 => (2200, 1125, 44, 5),
        Resolution::R1280x720 => (1650, 750, 40, 5),
        Resolution::R640x480 => (800, 525, 96, 2),
    };

    unsafe {
        // Write horizontal and vertical total pixels
        core::ptr::write_volatile(DP_H_TOTAL_REG as *mut u32, h_total);
        core::ptr::write_volatile(DP_V_TOTAL_REG as *mut u32, v_total);

        // Write horizontal and vertical sync widths
        core::ptr::write_volatile(DP_H_SYNC_REG as *mut u32, h_sync);
        core::ptr::write_volatile(DP_V_SYNC_REG as *mut u32, v_sync);
    }
}


fn setup_dp_clock() {
    unsafe {
        core::ptr::write_volatile(DP_CLOCK_BASE as *mut u32, DP_ENABLE);
        core::ptr::write_volatile(DP_FREQUENCY_REG as *mut u32, 148_500_000);
    }
}

fn calculate_framebuffer_size(resolution: Resolution, color_depth: ColorDepth) -> usize {
    let (width, height) = match resolution {
        Resolution::R1920x1080 => (1920, 1080),
        Resolution::R1280x720  => (1280, 720),
        Resolution::R640x480   => (640, 480),
    };
    
    let bytes_per_pixel = match color_depth {
        ColorDepth::Bpp8  => 1,
        ColorDepth::Bpp16 => 2,
        ColorDepth::Bpp24 => 3,
        ColorDepth::Bpp32 => 4,
    };
    
    width * height * bytes_per_pixel
}

fn calculate_pixel_offset(x: u32, y: u32) -> usize {
    (y * 1920 + x) as usize * 4
}

fn log_error(msg: &str) {
    unsafe {
        for byte in msg.bytes() {
            core::ptr::write_volatile(DP_DEBUG_OUTPUT_BASE as *mut u8, byte);
        }
    }
}