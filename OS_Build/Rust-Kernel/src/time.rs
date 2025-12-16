#![allow(dead_code)]

use core::arch::asm;

/// Read Time-Stamp Counter (cycles). Useful for crude profiling / pacing.
#[inline]
pub fn rdtsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags));
    }
    ((hi as u64) << 32) | (lo as u64)
}

/// Hint to the CPU while spinning.
#[inline]
pub fn cpu_pause() {
    unsafe { asm!("pause", options(nomem, nostack, preserves_flags)); }
}

/// Crude busy-wait loop (cycle count is CPU-dependent).
#[inline]
pub fn spin(iter: u64) {
    for _ in 0..iter {
        cpu_pause();
    }
}

// =============================================================================
// CMOS RTC (real-time clock)
// - Works on PC-compatible systems / QEMU.
// - Reads RTC via ports 0x70/0x71.
// - No allocations.
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
    value
}

#[inline]
unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

#[inline]
fn bcd_to_bin(v: u8) -> u8 {
    (v & 0x0F) + ((v >> 4) * 10)
}

#[inline]
fn cmos_read(reg: u8) -> u8 {
    // Disable NMI by setting bit 7.
    unsafe {
        outb(0x70, reg | 0x80);
        inb(0x71)
    }
}

#[inline]
fn rtc_updating() -> bool {
    (cmos_read(0x0A) & 0x80) != 0
}

fn read_rtc_once() -> (u8, u8, u8, u8, u8, u8, u8) {
    // (sec, min, hour, day, month, year2, century)
    while rtc_updating() {}
    let sec = cmos_read(0x00);
    let min = cmos_read(0x02);
    let hour = cmos_read(0x04);
    let day = cmos_read(0x07);
    let mon = cmos_read(0x08);
    let yr  = cmos_read(0x09);
    let cen = cmos_read(0x32); // may be 0 on some BIOSes
    (sec, min, hour, day, mon, yr, cen)
}

/// Read current date/time from the CMOS RTC.
///
/// Notes:
/// - RTC values may be local time or UTC depending on BIOS/firmware settings.
/// - We convert BCD -> binary if needed.
/// - We normalize 12h -> 24h if needed.
pub fn rtc_now() -> DateTime {
    // Read until stable (two consecutive reads match).
    let mut a = read_rtc_once();
    let mut b = read_rtc_once();
    while a != b {
        a = b;
        b = read_rtc_once();
    }
    let (mut sec, mut min, mut hour, mut day, mut mon, mut yr, mut cen) = b;

    let status_b = cmos_read(0x0B);
    let is_binary = (status_b & 0x04) != 0;
    let is_24h    = (status_b & 0x02) != 0;

    if !is_binary {
        sec = bcd_to_bin(sec);
        min = bcd_to_bin(min);
        // hour is special in 12h mode (bit7 may be PM flag)
        let pm = (hour & 0x80) != 0;
        hour = bcd_to_bin(hour & 0x7F) | if pm { 0x80 } else { 0 };
        day = bcd_to_bin(day);
        mon = bcd_to_bin(mon);
        yr  = bcd_to_bin(yr);
        if cen != 0 { cen = bcd_to_bin(cen); }
    }

    // Convert 12h -> 24h if needed
    if !is_24h {
        let pm = (hour & 0x80) != 0;
        hour &= 0x7F;
        if pm {
            if hour != 12 { hour = hour.wrapping_add(12); }
        } else {
            if hour == 12 { hour = 0; }
        }
    }

    let year: u16 = if cen != 0 {
        (cen as u16) * 100 + (yr as u16)
    } else {
        // Best-effort fallback: assume 2000-2069 for 00-69, else 1970-1999.
        if yr < 70 { 2000 + yr as u16 } else { 1900 + yr as u16 }
    };

    DateTime { year, month: mon, day, hour, minute: min, second: sec }
}

/// Format as ASCII: "MM/DD/YYYY HH:MM:SS" (19 chars).
/// Returns number of bytes written.
pub fn format_datetime(buf: &mut [u8; 32], dt: DateTime) -> usize {
    let mut i = 0usize;

    fn push2(buf: &mut [u8; 32], i: &mut usize, v: u8) {
        let tens = (v / 10) as u8;
        let ones = (v % 10) as u8;
        buf[*i] = b'0' + tens; *i += 1;
        buf[*i] = b'0' + ones; *i += 1;
    }
    fn push4(buf: &mut [u8; 32], i: &mut usize, v: u16) {
        let d0 = ((v / 1000) % 10) as u8;
        let d1 = ((v / 100) % 10) as u8;
        let d2 = ((v / 10) % 10) as u8;
        let d3 = (v % 10) as u8;
        buf[*i] = b'0' + d0; *i += 1;
        buf[*i] = b'0' + d1; *i += 1;
        buf[*i] = b'0' + d2; *i += 1;
        buf[*i] = b'0' + d3; *i += 1;
    }

    push2(buf, &mut i, dt.month);
    buf[i] = b'/'; i += 1;
    push2(buf, &mut i, dt.day);
    buf[i] = b'/'; i += 1;
    push4(buf, &mut i, dt.year);

    buf[i] = b' '; i += 1;

    push2(buf, &mut i, dt.hour);
    buf[i] = b':'; i += 1;
    push2(buf, &mut i, dt.minute);
    buf[i] = b':'; i += 1;
    push2(buf, &mut i, dt.second);

    i
}
