#![no_std]
#![no_main]

extern "C" {
    fn kernel_main();
}

#[no_mangle]
pub extern "C" fn kernel_entry() -> ! {
    unsafe {
        kernel_main(); // Call the kernel's main function
    }

    loop {
        core::arch::asm!("hlt"); // Halt the CPU
    }
}
