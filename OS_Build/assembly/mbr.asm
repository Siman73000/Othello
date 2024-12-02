extern print16
extern print16_nl
extern switchto32bit
extern print32
extern disk_load
global BEGIN_32BIT
global start
global load_kernel

KERNEL_OFFSET equ 0x1000

; Start the boot loader process

[bits 16]   ;   defining as 16 bit Real Mode

start:
    mov [BOOT_DRIVE], dl
    mov bp, 0x9000
    mov sp, bp

    mov bx, MSG_16BIT_MODE
    call print16
    call print16_nl

    call load_kernel
    call switchto32bit
    jmp $


load_kernel:
    mov bx, MSG_LOAD_KERNEL
    call print16
    call print16_nl

    mov bx, KERNEL_OFFSET
    mov edx, 32
    mov edx, [BOOT_DRIVE]
    call disk_load
    ret


[bits 32]   ; defining as 32 bit Protected Mode

BEGIN_32BIT:
    mov ebx, MSG_32BIT_MODE
    call print32
    call KERNEL_OFFSET
    jmp $

; Self explanitory stuff below

BOOT_DRIVE db 0
MSG_16BIT_MODE db "Started in 16-bit Real Mode", 0
MSG_32BIT_MODE db "Landed in 32-bit Protected Mode", 0
MSG_LOAD_KERNEL db "Loading kernel into memory", 0


; hex codes for the boot sector

times 510 - ($-$$) db 0
dw 0xaa55
