[bits 64]

global kernel_entry
extern kernel_main

section .text
global _start

kernel_entry:
    ; Set up segment registers for 64-bit mode
    mov ax, 0x10             ; DATA_SEG selector (64-bit)
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; Set up the stack for 64-bit mode
    mov rsp, 0x90000         ; Adjust to your stack location

    ; Call the kernel's main function
    call kernel_main

    ; Halt the CPU after returning from kernel_main
    hlt
    jmp $
