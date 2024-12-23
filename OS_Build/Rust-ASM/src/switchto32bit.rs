#![no_std]
#![no_main]

extern "C" {
    static gdt_descriptor: u64;
    static CODE_SEG: u16;
    static DATA_SEG: u16;
    static BEGIN_32BIT: u32;
}

use core::arch::naked_asm;
const CR0_PROTECTED_MODE: u32 = 0x1;

#[no_mangle]
pub extern "C" fn switch_to_32bit() {
    unsafe {
        // 1. Disable interrupts (CLI)
        naked_naked_asm!("cli");

        // 2. Load GDT descriptor
        naked_naked_asm!("lgdt [{}]", in(reg) &gdt_descriptor);

        // 3. Enable protected mode (set bit 0 of CR0)
        let mut cr0: u32;
        naked_naked_asm!("mov {}, cr0", out(reg) cr0);
        cr0 |= CR0_PROTECTED_MODE;  // Set the PE bit to 1 (Protected Mode)
        naked_naked_asm!("mov cr0, {}", in(reg) cr0);

        // 4. Far jump to CODE_SEG:init_32bit (jump to protected mode code)
        naked_naked_asm!("jmp {}, init_32bit", in(reg) CODE_SEG);

        // 32-bit initialization code (init_32bit)
        // This part will execute once the far jump happens and we enter 32-bit mode
        init_32bit();
    }
}

// 32-bit initialization function after switching to protected mode
fn init_32bit() {
    unsafe {
        // Print message or halt to check if transition is successful
        naked_asm!("mov eax, 0x1");
        naked_asm!("int 0x80");  // OS or BIOS interrupt for testing

        // Setup segment registers and stack
        naked_asm!("mov ax, {}", in(reg) DATA_SEG);
        naked_asm!("mov ds, ax");
        naked_asm!("mov ss, ax");
        naked_asm!("mov es, ax");
        naked_asm!("mov fs, ax");
        naked_asm!("mov gs, ax");

        // Set up stack pointer
        naked_asm!("mov ebp, 0x90000");
        naked_asm!("mov esp, ebp");

        // Call BEGIN_32BIT to transition to the next phase of the bootloader or kernel
        naked_asm!("call BEGIN_32BIT");
    }
}
