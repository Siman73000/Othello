; stage2.asm – real-mode second stage loader + long-mode trampoline

[bits 16]
[org 0x8000]                  ; we execute at 0000:8000

extern gdt_descriptor
extern CODE_SEG
extern DATA_SEG
extern CODE_SEG_64
extern switchto64bit_stage2

STAGE2_SECTORS    equ 4               ; must match mbr_stage1.asm + disk builder
KERNEL_LBA_START  equ 1 + STAGE2_SECTORS  ; kernel starts after stage2
KERNEL_SECTORS    equ 64              ; sectors to read for kernel

KERNEL_LOAD_SEG   equ 0x2000          ; ES = 0x2000
KERNEL_LOAD_OFF   equ 0x0000          ; BX = 0x0000
; physical address = 0x2000 << 4 = 0x00020000
KERNEL_LOAD_PHYS  equ 0x0000000000020000  ; 64-bit immediate for long mode jump

global stage2_entry
stage2_entry:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    sti

    ; print "Stage 2 at 0x8000 OK"
    mov si, msg_stage2
    mov ah, 0x0E
.print_msg:
    lodsb
    test al, al
    jz .after_msg
    int 0x10
    jmp .print_msg
.after_msg:

    ; --- load kernel using BIOS Int 13h extensions (LBA read) ---
    call load_kernel_lba

    ; --- switch to 32-bit protected mode ---
    cli
    lgdt [gdt_descriptor]
    mov eax, cr0
    or eax, 0x1                  ; PE = 1
    mov cr0, eax
    jmp CODE_SEG:pm_entry        ; far jump to 32-bit code

; ----------------- BIOS LBA loader (real mode) -----------------
; Uses Int 13h, AH=42h (Extended Read)
; Requires DL still holding boot drive (BIOS preserves DL across our jump
; from stage1 -> stage2, so we rely on that).
; ----------------------------------------------------------------
load_kernel_lba:
    pushad

    mov word [dap.sector_count], KERNEL_SECTORS
    mov word [dap.buf_off],      KERNEL_LOAD_OFF
    mov word [dap.buf_seg],      KERNEL_LOAD_SEG
    mov dword [dap.lba_low],     KERNEL_LBA_START
    mov dword [dap.lba_high],    0

    mov si, dap
    mov ah, 0x42                 ; Extended read
    ; DL already holds boot drive
    int 0x13
    jc .read_fail

    popad
    ret

.read_fail:
    mov si, msg_load_fail
    mov ah, 0x0E
.err_print:
    lodsb
    test al, al
    jz .hang
    int 0x10
    jmp .err_print

.hang:
    cli
.hang_loop:
    hlt
    jmp .hang_loop

; ===================== 32-bit section ==========================
[bits 32]
pm_entry:
    mov ax, DATA_SEG
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    mov esp, 0x9FC00             ; temporary 32-bit stack

    ; let dedicated file do PAE + long-mode + paging, then call BEGIN_64BIT
    call switchto64bit_stage2

.halt32:
    hlt
    jmp .halt32

; ===================== 64-bit section ==========================
[bits 64]
global BEGIN_64BIT
BEGIN_64BIT:
    ; We are now in 64-bit long mode with paging enabled.
    ; The kernel has been loaded at KERNEL_LOAD_PHYS (identity mapped).
    mov rax, KERNEL_LOAD_PHYS
    jmp rax                      ; enter your Rust kernel

; ===================== Data / DAP / strings ====================
[bits 16]
section .data

dap:                        ; Disk Address Packet (for AH=42h)
    db  16                  ; size of packet
    db  0                   ; reserved
sector_count:
    dw  0                   ; filled in at runtime
buf_off:
    dw  0                   ; offset of buffer
buf_seg:
    dw  0                   ; segment of buffer
lba_low:
    dd  0                   ; low dword of LBA
lba_high:
    dd  0                   ; high dword of LBA

msg_stage2    db "Stage 2 at 0x8000 OK", 0
msg_load_fail db "Kernel load failed, halting.", 0

; ======================================================
; Load kernel.bin into 0x0010:0000 (physical 0x100000)
; ======================================================
load_kernel:
    mov ah, 0x02              ; BIOS read sectors
    mov al, 32                ; read 32 sectors (~16 KiB kernel)
    mov ch, 0                 ; cylinder 0
    mov dh, 0                 ; head 0
    mov cl, 6                 ; sector 6 (LBA 5 = sector 6)
    mov dl, [boot_drive]
    mov bx, 0x0000
    mov es, 0x0010            ; ES:BX = 0010:0000 → 0x100000
    int 0x13
    jc kernel_load_failed

    mov si, msg_kernel_ok
    mov ah, 0x0E
.print_msg2:
    lodsb
    test al, al
    jz kernel_loaded
    int 0x10
    jmp .print_msg2

kernel_loaded:
    jmp continue_stage2

kernel_load_failed:
    mov si, msg_kernel_fail
    mov ah, 0x0E
.print_fail2:
    lodsb
    test al, al
    jz .halt2
    int 0x10
    jmp .print_fail2
.halt2:
    cli
    hlt
    jmp $

msg_kernel_ok   db " Kernel loaded OK.",0
msg_kernel_fail db " Kernel load failed!",0

