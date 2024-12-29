extern print16
extern print16_nl
extern switchto32bit
extern switchto64bit
extern print32
extern print64
extern disk_load
global BEGIN_32BIT
global BEGIN_64BIT
global start
global load_kernel

KERNEL_OFFSET equ 0x1000

; Start the bootloader process

[bits 16]   ; 16-bit Real Mode

start:
    mov [BOOT_DRIVE], dl         ; Store boot drive number
    mov bp, 0x9000              ; Set up stack
    mov sp, bp

    mov bx, MSG_16BIT_MODE
    call print16
    call print16_nl

    call load_kernel            ; Load kernel from disk
    call switchto32bit          ; Switch to 32-bit Protected Mode
    jmp $

load_kernel:
    mov bx, MSG_LOAD_KERNEL
    call print16
    call print16_nl

    mov bx, KERNEL_OFFSET       ; Load kernel at defined offset
    mov edx, 32
    mov dl, [BOOT_DRIVE]
    call disk_load
    ret

[bits 32]   ; 32-bit Protected Mode

BEGIN_32BIT:
    mov ebx, MSG_32BIT_MODE
    call print32

    call switchto64bit          ; Transition to 64-bit Long Mode
    jmp $

[bits 64]   ; 64-bit Long Mode

BEGIN_64BIT:
    mov rbx, MSG_64BIT_MODE
    call print64                ; Print 64-bit mode message

    mov rsi, KERNEL_OFFSET      ; Load kernel entry point
    call rsi                    ; Call the 64-bit kernel entry point
    hlt                         ; Halt if kernel returns
    jmp $

; Messages

BOOT_DRIVE db 0
MSG_16BIT_MODE db "Started in 16-bit Real Mode", 0
MSG_32BIT_MODE db "Landed in 32-bit Protected Mode", 0
MSG_64BIT_MODE db "Entered 64-bit Long Mode", 0
MSG_LOAD_KERNEL db "Loading kernel into memory", 0

; Boot sector padding and signature

times 510 - ($-$$) db 0
dw 0xaa55
