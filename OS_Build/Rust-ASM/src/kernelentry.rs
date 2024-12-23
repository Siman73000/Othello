#![no_std]
#![no_main]

extern fn kernel_main();

#[no_mangle]
pub fn kernel_entry() -> ! {
    unsafe {
        kernel_main(); // Call the kernel's main function
    }

    loop {
        core::arch::asm!("hlt"); // Halt the CPU
    }
}
