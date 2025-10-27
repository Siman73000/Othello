[BITS 64]

global kernel_entry      ; Exported symbol for linker
extern kernel_main       ; Rust kernel entry point

section .text

kernel_entry:
    ; --------------------------
    ; Set up segment registers
    ; --------------------------
    mov ax, 0x10         ; DATA_SEG selector (from GDT)
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; --------------------------
    ; Set up stack pointer
    ; --------------------------
    mov rsp, 0x90000

    ; --------------------------
    ; Call Rust kernel main
    ; --------------------------
    call kernel_main

    ; --------------------------
    ; Halt CPU when kernel_main returns
    ; --------------------------
.halt_loop:
    hlt
    jmp .halt_loop