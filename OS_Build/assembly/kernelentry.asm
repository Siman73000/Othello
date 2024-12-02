[bits 32]

global kernel_entry
extern kernel_main

section .text
global _start

kernel_entry:

    call kernel_main

    hlt
    jmp $
