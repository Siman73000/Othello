; mbr_stage1.asm - 16-bit MBR (Stage 1)
; BIOS loads this to 0000:7C00 and jumps here.
; It loads Stage 2 from disk into 0000:8000, then jumps there.

[bits 16]
[org 0x7C00]

STAGE2_LOAD_SEG equ 0x0000       ; ES = 0x0000
STAGE2_LOAD_OFF equ 0x8000       ; 0000:8000 physical
STAGE2_SECTORS  equ 8            ; load 8 sectors (LBA 1..8 => CHS S=2..9)

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    sti

    mov [boot_drive], dl         ; save BIOS boot drive

    ; Print "MBR alive, loading stage 2..."
    mov si, msg1
    mov ah, 0x0E
.print_msg1:
    lodsb
    test al, al
    jz .after_msg1
    int 0x10
    jmp .print_msg1

.after_msg1:
    call load_stage2

    ; Far jump to 0000:8000 (Stage 2)
    jmp 0x0000:STAGE2_LOAD_OFF

; ---------------------------------------------------------------------------
; load_stage2: read STAGE2_SECTORS sectors starting at CHS (0,0,2)
; into ES:BX = 0000:8000
; ---------------------------------------------------------------------------
load_stage2:
    push ax
    push bx
    push cx
    push dx
    push es

    mov ax, STAGE2_LOAD_SEG
    mov es, ax
    mov bx, STAGE2_LOAD_OFF

    mov ah, 0x02                 ; BIOS read
    mov al, STAGE2_SECTORS       ; sectors to read (8)
    mov ch, 0                    ; cylinder 0
    mov dh, 0                    ; head 0
    mov cl, 2                    ; sector 2 (LBA 1)
    mov dl, [boot_drive]
    int 0x13
    jc .read_fail

    jmp .done

.read_fail:
    mov si, msg_fail
    mov ah, 0x0E
.rf_loop:
    lodsb
    test al, al
    jz .hang
    int 0x10
    jmp .rf_loop

.hang:
    cli
.hang_loop:
    hlt
    jmp .hang_loop

.done:
    pop es
    pop dx
    pop cx
    pop bx
    pop ax
    ret

; ---------------------------------------------------------------------------
; Data
; ---------------------------------------------------------------------------

boot_drive db 0

msg1     db "MBR alive, loading stage 2...", 0
msg_fail db "Disk read failed, halting.", 0

; ---------------------------------------------------------------------------
; Boot signature
; ---------------------------------------------------------------------------

times 510 - ($ - $$) db 0
dw 0xAA55
