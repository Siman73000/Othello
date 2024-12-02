#![no_std]
#![no_main]

use core::ffi::c_char;

fn memory_copy(source: *const u8, dest: *mut u8, nbytes: usize) {
    unsafe {
        for i in 0..nbytes {
            *dest.add(i) = *source.add(i);
        }
    }
}

fn int_to_string(v: i32, buff: *mut c_char, radix_base: u32) -> *mut c_char {
    const TABLE: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

    unsafe {
        let mut p = buff;
        let mut n = if v < 0 && radix_base == 10 { -(v as i32) as u32 } else { v as u32 };

        while n >= radix_base {
            *p = TABLE[(n % radix_base) as usize] as c_char;
            p = p.add(1);
            n /= radix_base;
        }
        *p = TABLE[n as usize] as c_char;
        p = p.add(1);

        if v < 0 && radix_base == 10 {
            *p = b'-' as c_char;
            p = p.add(1);
        }

        *p = 0;

        let mut start = buff;
        let mut end = p.offset(-1);
        while start < end {
            let tmp = *start;
            *start = *end;
            *end = tmp;
            start = start.add(1);
            end = end.offset(-1);
        }

        buff
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}