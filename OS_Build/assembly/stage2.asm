; stage2.asm - Second-stage loader at 0000:8000
;  * Still entered in 16-bit real mode, DL = boot drive
;  * Prints a message
;  * Loads kernel from disk into 0x0002_0000
;  * Builds 4-level paging, enables long mode
;  * Jumps to kernel at 0x0000000000020000

[bits 16]
[org 0x8000]

; ===================== CONSTANTS =====================

KERNEL_LOAD_SEG    equ 0x2000        ; ES = 0x2000 => phys 0x20000
KERNEL_LOAD_OFF    equ 0x0000        ; offset 0
KERNEL_LOAD_PHYS   equ 0x00020000    ; 128 KiB physical
KERNEL_SECTORS     equ 64            ; read 64 sectors (32 KiB) for kernel

; Disk layout:
;  LBA 0 : stage1 (MBR)
;  LBA 1-8 : stage2
;  LBA 9+ : kernel
KERNEL_LBA_START   equ 9
KERNEL_CHS_SECTOR  equ (KERNEL_LBA_START + 1) ; sector number in CHS (1-based)

; Page tables live under 2 MiB
PML4_PHYS          equ 0x1000
PDPTE_PHYS         equ 0x2000
PDE_PHYS           equ 0x3000

; GDT selectors
CODE32_SEL         equ 0x08
DATA32_SEL         equ 0x10
CODE64_SEL         equ 0x18
DATA64_SEL         equ 0x20

; ===================== ENTRY =====================

stage2_start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    sti

    ; Print "Stage 2 at 0x8000 OK"
    mov si, msg_stage2
    mov ah, 0x0E
.print_msg:
    lodsb
    test al, al
    jz .after_msg
    int 0x10
    jmp .print_msg
.after_msg:

    ; Load kernel to 0x0002_0000
    call load_kernel16

    ; Then switch to protected mode & long mode
    jmp enter_protected_mode

; ===================== REAL-MODE KERNEL LOAD =====================

; load_kernel16:
;   Read KERNEL_SECTORS starting at CHS (0,0,KERNEL_CHS_SECTOR)
;   into ES:BX = 0x2000:0000.
;   DL is still the boot drive set by BIOS / Stage 1.
load_kernel16:
    push ax
    push bx
    push cx
    push dx
    push es

    mov ax, KERNEL_LOAD_SEG
    mov es, ax
    mov bx, KERNEL_LOAD_OFF          ; ES:BX = 0x2000:0000

    mov ah, 0x02                     ; INT 13h read
    mov al, KERNEL_SECTORS           ; number of sectors (64)
    mov ch, 0                        ; cylinder 0
    mov dh, 0                        ; head 0
    mov cl, KERNEL_CHS_SECTOR        ; sector (10 => LBA 9)
    ; DL already holds boot drive
    int 0x13
    jc .read_fail

    jmp .done

.read_fail:
    mov si, msg_kfail
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

; ===================== PROTECTED MODE + LONG MODE =====================

enter_protected_mode:
    cli
    lgdt [gdt_descriptor]

    mov eax, cr0
    or  eax, 1                       ; enable PE bit
    mov cr0, eax

    jmp CODE32_SEL:pm_entry          ; far jump -> 32-bit

[bits 32]
pm_entry:
    ; flat segments
    mov ax, DATA32_SEL
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    mov esp, 0x90000                 ; 32-bit stack (under 2 MiB)

    ; ---- Build 4-level paging for first 2 MiB identity map ----

    ; Zero PML4
    mov edi, PML4_PHYS
    mov ecx, 4096 / 4
    xor eax, eax
    rep stosd

    ; Zero PDPT
    mov edi, PDPTE_PHYS
    mov ecx, 4096 / 4
    xor eax, eax
    rep stosd

    ; Zero PD
    mov edi, PDE_PHYS
    mov ecx, 4096 / 4
    xor eax, eax
    rep stosd

    ; PML4[0] -> PDPTE | present | writable
    mov dword [PML4_PHYS], (PDPTE_PHYS | 0x03)
    mov dword [PML4_PHYS + 4], 0

    ; PDPT[0] -> PDE | present | writable
    mov dword [PDPTE_PHYS], (PDE_PHYS | 0x03)
    mov dword [PDPTE_PHYS + 4], 0

    ; PDE[0] -> 2 MiB page at 0x00000000 | present | writable | PS
    mov dword [PDE_PHYS], 0x00000083
    mov dword [PDE_PHYS + 4], 0

    ; Enable PAE
    mov eax, cr4
    or  eax, 0x20                    ; PAE
    mov cr4, eax

    ; Enable long mode (LME) in IA32_EFER
    mov ecx, 0xC0000080              ; IA32_EFER
    rdmsr
    or  eax, 0x100                   ; LME
    wrmsr

    ; Load PML4
    mov eax, PML4_PHYS
    mov cr3, eax

    ; Enable paging
    mov eax, cr0
    or  eax, 0x80000000              ; PG
    mov cr0, eax

    ; Jump to 64-bit code
    jmp CODE64_SEL:long_mode_entry

[bits 64]
long_mode_entry:
    mov ax, DATA64_SEL
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    mov rsp, 0x90000
    and rsp, 0xFFFFFFFFFFFFFFF0      ; 16-byte aligned

    ; Optional tiny debug marker at top-left of screen
    mov rdi, 0xB8000
    mov rax, 0x0F2A0F2A0F2A0F2A      ; "***" style marker
    mov [rdi], rax

    ; Jump to Rust kernel entry at 0x0000000000020000
    mov rax, KERNEL_LOAD_PHYS
    jmp rax

hang64:
    hlt
    jmp hang64

; ===================== GDT =====================

; 0x00: null
; 0x08: 32-bit code
; 0x10: 32-bit data
; 0x18: 64-bit code
; 0x20: 64-bit data

gdt_start:
    dq 0x0000000000000000          ; null
    dq 0x00CF9A000000FFFF          ; 32-bit code
    dq 0x00CF92000000FFFF          ; 32-bit data
    dq 0x00AF9A000000FFFF          ; 64-bit code
    dq 0x00AF92000000FFFF          ; 64-bit data
gdt_end:

gdt_descriptor:
    dw gdt_end - gdt_start - 1
    dd gdt_start

; ===================== STRINGS =====================

msg_stage2 db "Stage 2 at 0x8000 OK", 0
msg_kfail  db "Kernel load failed, halting.", 0
