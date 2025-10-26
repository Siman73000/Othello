#![no_std]
#![no_main]

use core::ffi::c_char;

// copies memory from source to dest
pub fn memory_copy(source: *const u8, dest: *mut u8, nbytes: usize) {
    unsafe {
        for i in 0..nbytes {
            // raw pointer math
            *dest.add(i) = *source.add(i);
        }
    }
}

// converts an int to a null-terminated string in a specified radix/base
pub fn int_to_string(int_to_convert: i32, buff: *mut c_char, radix_base: u32) -> *mut c_char {

    // lookup table
    const TABLE: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

    // unsafe due to raw pointer math and dereferencing
    unsafe {
        let mut p = buff;
        let mut n = if int_to_convert < 0 && radix_base == 10 { -(int_to_convert as i32) as u32 } else { int_to_convert as u32 };

        // converts the int to str
        while n >= radix_base {
            *p = TABLE[(n % radix_base) as usize] as c_char;
            p = p.add(1);
            n /= radix_base;
        }
        *p = TABLE[n as usize] as c_char;
        p = p.add(1);

        // adds a negative sign if the number is negative and the base is 10
        if int_to_convert < 0 && radix_base == 10 {
            *p = b'-' as c_char;
            p = p.add(1);
        }

        // null-terminates the string
        *p = 0;

        // reverses the string
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