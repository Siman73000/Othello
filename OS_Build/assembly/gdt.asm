;
;   The GDT File (Global Descriptor Table) x86 based data structure
;
;   Defines memory segments for the CPU
;   Enforces memory Access Control RWE
;   Enables x86 memory protection
;   Transitions between 16bit real mode, 32bit protected mode, and 64bit long mode
;
;
;
;   Descriptor Layout for 32bit Protected Mode
;
;   0-15    : Seg Limit (low 16 bits)
;   16-31   : Base Address (low 16 bits)
;   32-39   : Base Address (middle 8 bits)
;   40-43   : Access Byte
;   44-47   : Flags and Seg Limit (high 4 bits)
;   48-55   : Base Address (high 8 bits)
;   56-63   : Reserved for Future Uses
;
;____________________________________________________________________________________
;
;
;   Access Byte Bit Breakdown
;
;   0       : Accessed | Set by CPU when seg is accessed
;   1       : Write/Read | Data Write, Code Read
;   2       : Direction/Conforming | Expands down data or conforms code
;   3       : Executable | 1 = Code Seg, 0 = Data Seg
;   4       : Descriptor Type | 1 = Code/Data, 0 = System
;   5       : DPL0-DPL1 | Descriptor Privilege Level (ring)
;   6       : Present | 1 = Seg is valid
;
section .data

global gdt_descriptor

global CODE_SEG

global DATA_SEG

global CODE_SEG_64

align 16
gdt_start:
    dq 0x0                  ; Null descriptor

; 32 bit code segment
gdt_code:
    dw 0xffff               ; Limit 16 bits low
    dw 0x0                  ; Base 16 bits low
    db 0x0                  ; Base next 8 bits
    db 10011010b            ; Access code segment
    db 11001111b            ; Granularity byte (G=1, D/B=1, L=0)
    db 0x0                  ; Base 8 bits high

; 32 bit data segment
gdt_data:
    dw 0xffff               ; Limit 16 bits low
    dw 0x0                  ; Base 16 bits low
    db 0x0                  ; Base next 8 bits
    db 10010010b            ; Access data segment
    db 11001111b            ; Granularity byte (G=1, D/B=1, L=0)
    db 0x0                  ; Base 8 bits high

; 64 bit code segment
gdt_code_64:
    dw 0x0                  ; Limit ignored in 64-bit mode
    dw 0x0                  ; Base 16 bits low
    db 0x0                  ; Base middle
    db 10011010b            ; Access code segment (Code, Exe, Read)
    db 00100000b            ; Long mode, D/B cleared to avoid 32-bit default
    db 0x0                  ; Base 8 bits high

gdt_end:

gdt_descriptor:
    dw gdt_end - gdt_start - 1   ; Limit size of gdt
    dd gdt_start                 ; Base address of gdt

CODE_SEG equ (gdt_code - gdt_start)      ; Code segment selector
DATA_SEG equ (gdt_data - gdt_start)      ; Data segment selector
CODE_SEG_64 equ (gdt_code_64 - gdt_start) ; 64 bit code segment selector
