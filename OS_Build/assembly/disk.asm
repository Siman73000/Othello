; disk.asm – simple BIOS disk helpers for stage-2 bootloader
; Assembled as ELF32 but contains 16-bit real-mode code

[bits 16]

global disk_load
extern check_partition_table
global disk_error
global sectors_error
global load_kernel_mbr
global load_kernel_gpt
extern PARTITION_TYPE
global EXPECTED_KERNEL_SECTORS

extern print16
extern print16_nl
extern BOOT_DRIVE

KERNEL_OFFSET    equ 0x1000       ; must match mbr_or_gpt.asm

section .data

;PARTITION_TYPE          db 0       ; 1 = MBR, 2 = GPT (for now we just set 1)
EXPECTED_KERNEL_SECTORS db 0

MSG_DISK_ERROR          db "Disk read error", 0
MSG_SECTORS_ERROR       db "Sector count mismatch", 0

section .text

; ---------------------------------------------------------------------------
; check_partition_table
;   For now: just assume an MBR-style layout.
;   (You can later extend this to actually inspect a GPT protective MBR, etc.)
; ---------------------------------------------------------------------------
;check_partition_table:
;    mov byte [PARTITION_TYPE], 1   ; 1 = MBR
;    ret

; ---------------------------------------------------------------------------
; disk_error  – print error and halt
; ---------------------------------------------------------------------------
disk_error:
    mov bx, MSG_DISK_ERROR
    call print16
    cli
    hlt
    jmp $

; ---------------------------------------------------------------------------
; sectors_error – print error and halt
; ---------------------------------------------------------------------------
sectors_error:
    mov bx, MSG_SECTORS_ERROR
    call print16
    cli
    hlt
    jmp $

; ---------------------------------------------------------------------------
; disk_read_chs
;   Low-level CHS read using BIOS int 13h
;
;   In:
;     DL = drive
;     ES:BX = destination
;     CH = cylinder
;     DH = head
;     CL = starting sector (1-based)
;     AL = sector count (1..)
;
;   Out:
;     CF clear on success, set on error
;   Trashes:
;     AH
; ---------------------------------------------------------------------------
disk_read_chs:
    push dx
    push cx
    push bx
    push ax

    mov ah, 0x02            ; BIOS read sectors
    int 0x13
    jc .fail

    pop ax
    pop bx
    pop cx
    pop dx
    clc
    ret

.fail:
    pop ax
    pop bx
    pop cx
    pop dx
    stc
    ret

; ---------------------------------------------------------------------------
; load_kernel_mbr / load_kernel_gpt
;
; Calling convention (from mbr_or_gpt.asm):
;   - DL = BOOT_DRIVE
;   - EXPECTED_KERNEL_SECTORS already set
;   - Kernel should be loaded at KERNEL_OFFSET in segment 0x0000
;
; For now, GPT and MBR follow the same loading strategy.
; ---------------------------------------------------------------------------

load_kernel_mbr:
    jmp short load_kernel_common

load_kernel_gpt:
    jmp short load_kernel_common

load_kernel_common:
    push ax
    push bx
    push cx
    push dx
    push si

    ; ES = 0x0000, BX = KERNEL_OFFSET
    xor ax, ax
    mov es, ax
    mov bx, KERNEL_OFFSET

    movzx si, byte [EXPECTED_KERNEL_SECTORS] ; total sectors to read
    mov ch, 0               ; cylinder 0
    mov dh, 0               ; head 0
    mov cl, 2               ; start at sector 2 (sector 1 is boot sector)

.read_loop:
    cmp si, 0
    je .done

    mov al, 1               ; read 1 sector at a time
    call disk_read_chs
    jc disk_error

    add bx, 512             ; next destination address
    inc cl                  ; next sector
    dec si
    jmp .read_loop

.done:
    pop si
    pop dx
    pop cx
    pop bx
    pop ax
    ret

; ---------------------------------------------------------------------------
; disk_load
;
; Older path from mbr_or_gpt::load_kernel:
;   mov bx, KERNEL_OFFSET
;   mov edx, 32
;   mov dl, [BOOT_DRIVE]
;   call disk_load
;
; To keep things consistent, we ignore EDX and just use
;   EXPECTED_KERNEL_SECTORS + BOOT_DRIVE and reuse load_kernel_mbr.
; ---------------------------------------------------------------------------
disk_load:
    ; Ensure DL contains BOOT_DRIVE
    mov dl, [BOOT_DRIVE]
    jmp load_kernel_mbr
