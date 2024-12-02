section .data
global gdt_descriptor

extern DATA_SEG
extern CODE_SEG
align 8
gdt_start:
    dq 0x0              ; Null descriptor

gdt_code:
    dw 0xffff           ; Limit 16 bits low
    dw 0x0              ; Base 16 bits low

    db 0x0              ; Base next 8 bits
    db 10011010b        ; Access code segment

    db 11001111b        ; Granularity byte aka pain and suffering
    db 0x0              ; Base 8 bits high

gdt_info:
    dw 0xffff           ; Limit 16 bits low
    dw 0x0              ; Base 16 bits low

    db 0x0              ; Base next 8 bits
    db 10010010b        ; Access data segment

    db 11001111b        ; Granularity byte (oh not the consiquences of my own actions :o)
    db 0x0              ; Base 8 bits high

gdt_end:

gdt_descriptor:
    dw gdt_end - gdt_start - 1   ; Limit size of gdt
    dd gdt_start                 ; Base address of gdt

CODE_SEG equ (gdt_code - gdt_start) * 8   ; Code segment selector
DATA_SEG equ (gdt_info - gdt_start) * 8   ; Data segment selector