; stage2.asm - second-stage bootloader for Othello
; ------------------------------------------------
; Loaded by MBR (stage1) at physical address 0x0000:0x8000.
; Sets up 32-bit protected mode, then 64-bit long mode,
; loads the Rust kernel via INT 13h extensions, and jumps to it.

[bits 16]
[org 0x8000]

; -------------------------------
; Disk / layout parameters
; -------------------------------

STAGE2_SECTORS    equ 8                      ; sectors reserved for stage2 (must match stage1)
KERNEL_LBA_START  equ 1 + STAGE2_SECTORS

; How many sectors of kernel to read (can safely overshoot a bit)
KERNEL_SECTORS    equ 128                    ; 128 * 512 = 64 KiB

; Kernel load location (physical)
KERNEL_LOAD_SEG   equ 0x2000                ; 0x2000:0x0000 = 0x0002_0000
KERNEL_LOAD_OFF   equ 0x0000
KERNEL_LOAD_PHYS  equ 0x0000000000020000    ; 64-bit jump target

; GDT selectors (indices into our GDT defined later)
CODE_SEG      equ 0x08      ; 32-bit code
DATA_SEG      equ 0x10      ; 32-bit data
CODE_SEG_64   equ 0x18      ; 64-bit code
DATA_SEG_64   equ 0x20      ; 64-bit data

global stage2_entry
global BEGIN_64BIT

; ------------------------------------------------
; Stage 2 entry (real mode, 16-bit)
; ------------------------------------------------

stage2_entry:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00              ; simple temporary 16-bit stack below us

    mov [boot_drive], dl        ; preserve BIOS boot drive for INT 13h

    ; switch to 320x200x256 (Mode 13h) for the framebuffer at 0xA0000
    call set_vga_mode13

    sti

    ; Banner so we know stage2 actually ran
    mov si, msg_stage2
    call bios_print_string

    ; DL must be boot drive for INT 13h extensions
    mov dl, [boot_drive]
    call load_kernel_lba

    ; If we get here, kernel was loaded successfully.
    ; Now build GDT and enter protected mode.
    cli
    lgdt [gdt_descriptor]

    mov eax, cr0
    or  eax, 0x00000001         ; set PE bit
    mov cr0, eax

    ; Far jump flushes prefetch queue and loads CS with CODE_SEG
    jmp CODE_SEG:pm_entry

; ------------------------------------------------
; Real-mode kernel loader (INT 13h extensions, AH=42h)
; ------------------------------------------------
; In:  DL = BIOS drive number
; Out: returns on success, prints error and halts on failure

load_kernel_lba:
    pusha
    push ds
    push es

    xor ax, ax
    mov ds, ax

    ; Fill in the Disk Address Packet
    mov byte [dap.size], 16
    mov byte [dap.reserved], 0

    mov word [dap.sector_count], KERNEL_SECTORS
    mov word [dap.buf_off],      KERNEL_LOAD_OFF
    mov word [dap.buf_seg],      KERNEL_LOAD_SEG

    mov dword [dap.lba_low],  KERNEL_LBA_START
    mov dword [dap.lba_high], 0

    ; Progress message
    mov si, msg_load_kernel
    call bios_print_string

    ; DS:SI must point to DAP
    mov si, dap

    ; DL = BIOS drive number
    mov dl, [boot_drive]

    mov ah, 0x42                ; extended read
    int 0x13
    jc  .error                  ; CF set on error

    ; Success
    mov si, msg_kernel_ok
    call bios_print_string

    pop es
    pop ds
    popa
    ret

.error:
    ; Save BIOS status code in AH for debugging
    mov [disk_status], ah

    mov si, msg_kernel_fail
    call bios_print_string

    ; Print status in hex (two digits)
    mov al, [disk_status]
    call print_hex8

    mov si, msg_load_fail
    call bios_print_string

.hang:
    cli
.hang_loop:
    hlt
    jmp .hang_loop

; ------------------------------------------------
; Small 16-bit helpers
; ------------------------------------------------

; Print zero-terminated string at DS:SI using INT 10h / teletype
bios_print_string:
    pusha
    mov ah, 0x0E
.bs_loop:
    lodsb
    test al, al
    jz   .bs_done
    int  0x10
    jmp  .bs_loop
.bs_done:
    popa
    ret

; Print AL as two hex digits
print_hex8:
    pusha
    mov ah, al          ; save original in AH

    ; High nibble
    shr al, 4
    call print_hex_nibble

    ; Low nibble
    mov al, ah
    and al, 0x0F
    call print_hex_nibble

    popa
    ret

print_hex_nibble:
    cmp al, 10
    jb  .digit
    add al, 'A' - 10
    jmp .out
.digit:
    add al, '0'
.out:
    mov ah, 0x0E
    int 0x10
    ret

; Set VGA Mode 13h (320x200x256)
set_vga_mode13:
    mov ax, 0x0013
    int 0x10
    ret

; ------------------------------------------------
; 32-bit protected-mode entry
; ------------------------------------------------

[bits 32]
pm_entry:
    ; Set up data segments
    mov ax, DATA_SEG
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    ; 32-bit temporary stack near top of first MiB
    mov esp, 0x0009FC00

    call switchto64bit_stage2

.halt32:
    hlt
    jmp .halt32

; ------------------------------------------------
; 32-bit long-mode enable + paging
; ------------------------------------------------
; Uses PAE and 2 MiB pages to identity-map the first 2 GiB.
; Page tables at physical 0x1000 (PML4), 0x2000 (PDPT), 0x3000 (PD0),
; 0x4000 (PD1), 0x5000 (unused scratch).

switchto64bit_stage2:
    mov     edi, 0x1000          ; start clearing at 0x1000
    mov     ecx, (6 * 4096) / 4  ; 6 pages * 4096 bytes / 4 bytes per dword
    xor     eax, eax
    rep     stosd

    ; ---------------------------------------------------------
    ; PML4 @ 0x1000
    ;   entry 0 -> PDPT @ 0x2000 (present + RW)
    ; ---------------------------------------------------------
    mov     dword [0x1000], 0x2003   ; base=0x2000, P=1, RW=1

    ; ---------------------------------------------------------
    ; PDPT @ 0x2000
    ;   PDPTE[0] -> PD0 @ 0x3000 (maps 0..1 GiB)
    ;   PDPTE[1] -> PD1 @ 0x4000 (maps 1..2 GiB)
    ; ---------------------------------------------------------
    mov     dword [0x2000], 0x3003   ; entry 0 → PD0
    mov     dword [0x2008], 0x4003   ; entry 1 → PD1

    ; ---------------------------------------------------------
    ; PD0 @ 0x3000: map 0..1 GiB as 2MiB pages
    ; ---------------------------------------------------------
    mov     eax, 0x00000083          ; start at physical 0x00000000
    mov     edi, 0x3000              ; PD0 base
    mov     ecx, 512                 ; 512 entries * 2MiB = 1GiB
.map_pd0:
    mov     [edi], eax               ; write low 32 bits of PDE
    add     eax, 0x00200000          ; next 2MiB chunk
    add     edi, 8                   ; next PDE slot (64-bit entries)
    loop    .map_pd0

    ; ---------------------------------------------------------
    ; PD1 @ 0x4000: map 1..2 GiB as 2MiB pages
    ; first large page at physical 0x40000000 (1 GiB)
    ; ---------------------------------------------------------
    mov     eax, 0x40000083          ; physical 0x4000_0000 + flags
    mov     edi, 0x4000              ; PD1 base
    mov     ecx, 512                 ; another 1GiB
.map_pd1:
    mov     [edi], eax
    add     eax, 0x00200000
    add     edi, 8
    loop    .map_pd1

    ; ---------------------------------------------------------
    ; Load CR3 with PML4 physical address
    ; ---------------------------------------------------------
    mov     eax, 0x1000              ; PML4 base
    mov     cr3, eax

    ; ---------------------------------------------------------
    ; Enable PAE (CR4.PAE = 1)
    ; ---------------------------------------------------------
    mov     eax, cr4
    or      eax, 0x20                ; bit 5 = PAE
    mov     cr4, eax

    ; ---------------------------------------------------------
    ; Enable Long Mode in EFER (IA32_EFER MSR, 0xC000_0080)
    ; Set LME (bit 8)
    ; ---------------------------------------------------------
    mov     ecx, 0xC0000080          ; IA32_EFER
    rdmsr
    or      eax, 0x00000100          ; LME = 1
    wrmsr

    ; ---------------------------------------------------------
    ; Enable paging + (already enabled) protected mode in CR0
    ; Set PG (bit 31)
    ; ---------------------------------------------------------
    mov     eax, cr0
    or      eax, 0x80000001          ; PG=1, PE=1
    mov     cr0, eax

    ; Far jump into 64-bit code segment
    jmp     CODE_SEG_64:long_mode_entry

; ------------------------------------------------
; 64-bit long-mode entry
; ------------------------------------------------

[BITS 64]
long_mode_entry:
    mov ax, DATA_SEG_64
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    ; 64-bit stack
    mov rsp, 0x0009FF00

    ; Debug: print "LM64" so we know long mode works (text mode still active)
    mov rdi, 0x00000000000B8000
    mov ax, 0x0F4C         ; 'L'
    mov [rdi], ax
    mov ax, 0x0F4D         ; 'M'
    mov [rdi+2], ax
    mov ax, 0x0F36         ; '6'
    mov [rdi+4], ax
    mov ax, 0x0F34         ; '4'
    mov [rdi+6], ax

    xor rdi, rdi

    ; Jump to kernel entry at 0x0002_0000 (must match linker + loader)
    mov rax, 0x0000000000020000
    jmp rax

.hang:
    hlt
    jmp .hang

; ------------------------------------------------
; Data (back to 16-bit for convenience)
; ------------------------------------------------

[bits 16]

dap:
.size          db 16
.reserved      db 0
.sector_count  dw 0
.buf_off       dw 0
.buf_seg       dw 0
.lba_low       dd 0
.lba_high      dd 0

disk_status db 0
boot_drive  db 0

msg_stage2      db "Stage 2 loaded at 0x8000", 13,10,0
msg_load_kernel db "Loading kernel...", 13,10,0
msg_kernel_ok   db "Kernel load OK.", 13,10,0
msg_kernel_fail db "Kernel read error (AH=0x",0
msg_load_fail   db ")",13,10,"Kernel load failed - halting.",13,10,0

; ------------------------------------------------
; Global Descriptor Table
; ------------------------------------------------

align 8
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
