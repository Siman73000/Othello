; stage2.asm - second-stage bootloader for Othello with VBE/LFB
; ----------------------------------------------------------------------
; Loaded by MBR (stage1) at physical address 0x0000:0x8000.
; Sets up video (VBE if possible, else VGA 13h), 32-bit protected mode,
; 64-bit long mode, loads the Rust kernel via INT 13h extensions,
; and jumps to it.
;
; BootVideoInfo is written at 0x0000:0x9000:
;   u16 width
;   u16 height
;   u16 bpp
;   u64 framebuffer_addr   (low 32 bits valid, high 32 bits = 0)

[BITS 16]
[ORG 0x8000]

%include "kernel_sectors.inc"
%define STAGE2_SECTORS    8
%define KERNEL_LBA_START  (1 + STAGE2_SECTORS)
%ifndef KERNEL_SECTORS
%define KERNEL_SECTORS    256
%endif
%define KERNEL_LOAD_SEG   0x2000
%define KERNEL_LOAD_OFF   0x0000
%define KERNEL_LOAD_PHYS  0x0000000000020000

%define KERNEL_READ_CHUNK  16    ; sectors per INT13h AH=42h call (keeps buffers <64KiB)

%define CODE_SEG      0x08
%define DATA_SEG      0x10
%define CODE_SEG_64   0x18
%define DATA_SEG_64   0x20

; VBE / BootVideoInfo constants
%define VBE_INFO_ADDR       0x0600
%define VBE_MODE_INFO_ADDR  0x0800
%define BOOTVIDEO_ADDR      0x9000

; Desired VBE mode (we scan mode list)
%define DESIRED_WIDTH       1920
%define DESIRED_HEIGHT      1080
%define DESIRED_BPP_MIN     24

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

    ; ------------------------------------------------------------
    ; Set up video mode + BootVideoInfo @ 0x0000:0x9000
    ;   - Try VBE LFB 1920x1080 (or equivalent)
    ;   - fall back to VGA Mode 13h (320x200x8)
    ; ------------------------------------------------------------
    call set_video_mode_and_bootinfo

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

    ; Fill in the Disk Address Packet (DAP)
    mov byte [dap.size], 16
    mov byte [dap.reserved], 0
    mov word [dap.buf_off],      KERNEL_LOAD_OFF

    ; Destination starts at KERNEL_LOAD_SEG:0000
    mov ax, KERNEL_LOAD_SEG
    mov es, ax

    mov dword [dap.lba_low],  KERNEL_LBA_START
    mov dword [dap.lba_high], 0

    ; Progress message (may not show in VBE graphics, but harmless)
    mov si, msg_load_kernel
    call bios_print_string

    mov cx, KERNEL_SECTORS          ; remaining sectors

.load_loop:
    cmp cx, 0
    je  .success

    ; ax = min(cx, KERNEL_READ_CHUNK)
    mov ax, cx
    cmp ax, KERNEL_READ_CHUNK
    jbe .count_ok
    mov ax, KERNEL_READ_CHUNK
.count_ok:
    mov word [dap.sector_count], ax
    mov word [dap.buf_seg],      es

    ; DS:SI must point to DAP
    mov si, dap

    ; DL = BIOS drive number
    mov dl, [boot_drive]

    mov ah, 0x42                    ; extended read
    int 0x13
    jc  .error                      ; CF set on error

    ; Advance LBA by ax sectors (32-bit low)
    add word [dap.lba_low], ax
    adc word [dap.lba_low+2], 0

    ; Advance destination segment by ax*512 bytes = ax*32 paragraphs
    mov bx, ax
    shl bx, 5
    mov dx, es
    add dx, bx
    mov es, dx

    ; remaining -= ax
    sub cx, ax
    jmp .load_loop

.success:
    mov si, msg_kernel_ok
    call bios_print_string

    pop es
    pop ds
    popa
    ret

.error:
    ; Save BIOS status code in AH for debugging
    mov [disk_status], ah

    ; Switch to text mode so the error is visible (VBE graphics won't show teletype)
    mov ax, 0x0003
    int 0x10

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

; ------------------------------------------------
; set_video_mode_and_bootinfo
; ------------------------------------------------

set_video_mode_and_bootinfo:
    push ax
    push bx
    push cx
    push dx
    push si
    push di
    push es

    xor ax, ax
    mov ds, ax
    mov es, ax

    ; -------------------------------
    ; Get VBE controller info into 0x0600
    ; -------------------------------
    mov ax, 0x4F00
    mov di, VBE_INFO_ADDR
    int 0x10
    cmp ax, 0x004F
    jne .fallback_vga

    ; Mode list pointer is at offset 0x0E (offset) and 0x10 (segment)
    mov bx, [VBE_INFO_ADDR + 0x0E]     ; offset
    mov cx, [VBE_INFO_ADDR + 0x10]     ; segment
    mov [mode_list_off], bx
    mov [mode_list_seg], cx

    mov ax, 0FFFFh
    mov [best_vbe_mode], ax

.scan_modes:
    ; ES:DI -> next mode entry (word)
    mov bx, [mode_list_off]
    mov cx, [mode_list_seg]
    mov es, cx
    mov di, bx

    mov dx, [es:di]
    cmp dx, 0FFFFh
    je  .choose_mode        ; end of list

    ; advance list pointer
    add bx, 2
    mov [mode_list_off], bx

    ; Get mode info for DX into 0x0800
    push es
    push di
    mov ax, 0x4F01
    mov cx, dx              ; mode number
    mov di, VBE_MODE_INFO_ADDR
    push ds                 ; DS is 0
    pop es                  ; ES = 0
    int 0x10
    pop di
    pop es
    cmp ax, 0x004F
    jne .next_mode

    ; Check ModeAttributes LFB bit (bit 7)
    mov ax, [VBE_MODE_INFO_ADDR + 0x00]
    test ax, 080h
    jz .next_mode

    ; Check resolution
    mov ax, [VBE_MODE_INFO_ADDR + 0x12] ; XRes
    mov bx, [VBE_MODE_INFO_ADDR + 0x14] ; YRes
    cmp ax, DESIRED_WIDTH
    jne .next_mode
    cmp bx, DESIRED_HEIGHT
    jne .next_mode

    ; Check bpp >= DESIRED_BPP_MIN
    xor cx, cx
    mov cl, [VBE_MODE_INFO_ADDR + 0x19] ; BitsPerPixel
    cmp cl, DESIRED_BPP_MIN
    jb  .next_mode

    ; Found a suitable mode
    mov [best_vbe_mode], dx
    jmp .choose_mode

.next_mode:
    jmp .scan_modes

.choose_mode:
    mov ax, [best_vbe_mode]
    cmp ax, 0FFFFh
    je  .fallback_vga

    ; Set chosen VBE mode with LFB
    mov bx, ax
    or  bx, 4000h                   ; bit 14 = LFB
    mov ax, 0x4F02
    int 0x10
    cmp ax, 0x004F
    jne .fallback_vga

    ; Re-fetch mode info into 0x0800
    mov ax, 0x4F01
    mov cx, [best_vbe_mode]
    mov di, VBE_MODE_INFO_ADDR
    push ds
    pop es
    int 0x10
    cmp ax, 0x004F
    jne .fallback_vga

    ; width (u16)
    mov ax, [VBE_MODE_INFO_ADDR + 0x12]  ; XRes
    mov [BOOTVIDEO_ADDR + 0], ax

    ; height (u16)
    mov ax, [VBE_MODE_INFO_ADDR + 0x14]  ; YRes
    mov [BOOTVIDEO_ADDR + 2], ax

    ; bpp (u16) from u8 BitsPerPixel
    xor ax, ax
    mov al, [VBE_MODE_INFO_ADDR + 0x19]
    mov [BOOTVIDEO_ADDR + 4], ax

    ; framebuffer_addr (u64) from PhysBasePtr (32-bit)
    mov ax, [VBE_MODE_INFO_ADDR + 0x28]  ; low word
    mov [BOOTVIDEO_ADDR + 6], ax
    mov ax, [VBE_MODE_INFO_ADDR + 0x2A]  ; high word of low dword
    mov [BOOTVIDEO_ADDR + 8], ax
    xor ax, ax
    mov [BOOTVIDEO_ADDR + 10], ax        ; high dword = 0
    mov [BOOTVIDEO_ADDR + 12], ax

    ; Store pitch (BytesPerScanLine)
    mov ax, [VBE_MODE_INFO_ADDR + 0x10]      ; BytesPerScanLine (u16)
    mov [BOOTVIDEO_ADDR + 14], ax

    jmp .done

.fallback_vga:
    ; VGA Mode 13h 320x200x8 @ 0xA0000
    mov ax, 0x0013
    int 0x10

    ; width = 320
    mov ax, 320
    mov [BOOTVIDEO_ADDR + 0], ax

    ; height = 200
    mov ax, 200
    mov [BOOTVIDEO_ADDR + 2], ax

    ; bpp = 8
    mov ax, 8
    mov [BOOTVIDEO_ADDR + 4], ax

    ; framebuffer_addr = 0x000A0000 (u64)
    mov ax, 0x0000
    mov [BOOTVIDEO_ADDR + 6], ax   ; low word
    mov ax, 0x000A
    mov [BOOTVIDEO_ADDR + 8], ax   ; high word of low dword
    xor ax, ax
    mov [BOOTVIDEO_ADDR + 10], ax  ; high dword = 0
    mov [BOOTVIDEO_ADDR + 12], ax

    ; pitch = 320 bytes/scanline in Mode 13h
    mov ax, 320
    mov [BOOTVIDEO_ADDR + 14], ax
.done:
    pop es
    pop di
    pop si
    pop dx
    pop cx
    pop bx
    pop ax
    ret

; ------------------------------------------------
; 32-bit protected-mode entry
; ------------------------------------------------

[BITS 32]
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
; 32-bit long-mode enable + paging (identity map 0..4 GiB)
; ------------------------------------------------

switchto64bit_stage2:
    ; Clear 6 pages at 0x1000..0x6000:
    ;   0x1000: PML4
    ;   0x2000: PDPT
    ;   0x3000: PD0 (0..1 GiB)
    ;   0x4000: PD1 (1..2 GiB)
    ;   0x5000: PD2 (2..3 GiB)
    ;   0x6000: PD3 (3..4 GiB)
    mov     edi, 0x1000
    mov     ecx, (6 * 4096) / 4
    xor     eax, eax
    rep     stosd

    ; PML4 @ 0x1000
    mov     dword [0x1000], 0x2003   ; entry 0 -> PDPT @ 0x2000 (P=1,RW=1)

    ; PDPT @ 0x2000: 4 entries -> 4 PDs (0..4 GiB)
    mov     dword [0x2000], 0x3003   ; PDPTE[0] -> PD0 @ 0x3000
    mov     dword [0x2008], 0x4003   ; PDPTE[1] -> PD1 @ 0x4000
    mov     dword [0x2010], 0x5003   ; PDPTE[2] -> PD2 @ 0x5000
    mov     dword [0x2018], 0x6003   ; PDPTE[3] -> PD3 @ 0x6000

    ; PD0 @ 0x3000: map 0..1 GiB as 2MiB pages
    mov     eax, 0x00000083          ; physical 0x0000_0000 + flags
    mov     edi, 0x3000              ; PD0 base
    mov     ecx, 512                 ; 512 entries * 2MiB = 1GiB
.map_pd0:
    mov     [edi], eax               ; write low 32 bits of PDE
    add     eax, 0x00200000          ; next 2MiB chunk
    add     edi, 8                   ; next PDE slot (64-bit entries)
    loop    .map_pd0

    ; PD1 @ 0x4000: map 1..2 GiB
    mov     eax, 0x40000083          ; physical 0x4000_0000 + flags
    mov     edi, 0x4000
    mov     ecx, 512
.map_pd1:
    mov     [edi], eax
    add     eax, 0x00200000
    add     edi, 8
    loop    .map_pd1

    ; PD2 @ 0x5000: map 2..3 GiB
    mov     eax, 0x80000083          ; physical 0x8000_0000 + flags
    mov     edi, 0x5000
    mov     ecx, 512
.map_pd2:
    mov     [edi], eax
    add     eax, 0x00200000
    add     edi, 8
    loop    .map_pd2

    ; PD3 @ 0x6000: map 3..4 GiB (covers typical VBE LFB like 0xFD000000)
    mov     eax, 0xC0000083          ; physical 0xC000_0000 + flags
    mov     edi, 0x6000
    mov     ecx, 512
.map_pd3:
    mov     [edi], eax
    add     eax, 0x00200000
    add     edi, 8
    loop    .map_pd3

    ; Load CR3 with PML4 physical address
    mov     eax, 0x1000              ; PML4 base
    mov     cr3, eax

    ; Enable PAE (CR4.PAE = 1)
    mov     eax, cr4
    or      eax, 0x20                ; bit 5 = PAE
    mov     cr4, eax

    ; Enable Long Mode in EFER (IA32_EFER MSR, 0xC000_0080)
    mov     ecx, 0xC0000080          ; IA32_EFER
    rdmsr
    or      eax, 0x00000100          ; LME = 1
    wrmsr

    ; Enable paging + protected mode in CR0
    mov     eax, cr0
    or      eax, 0x80000001          ; PG=1, PE=1
    mov     cr0, eax

    ; Far jump into 64-bit code segment
    jmp     CODE_SEG_64:long_mode_entry

; ------------------------------------------------
; 64-bit long-mode entry
; ------------------------------------------------

[BITS 64]
BEGIN_64BIT:
long_mode_entry:
    mov ax, DATA_SEG_64
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    ; 64-bit stack
    mov rsp, 0x0009FF00

    ; Debug: write "LM64" to text memory (won't be visible in VBE, but harmless)
    mov rdi, 0x00000000000B8000
    mov ax, 0x0F4C         ; 'L'
    mov [rdi], ax
    mov ax, 0x0F4D         ; 'M'
    mov [rdi+2], ax
    mov ax, 0x0F36         ; '6'
    mov [rdi+4], ax
    mov ax, 0x0F34         ; '4'
    mov [rdi+6], ax

    ; Jump to kernel entry at 0x0002_0000
    mov rax, 0x0000000000020000
    jmp rax

.hang:
    hlt
    jmp .hang

; ------------------------------------------------
; Data (16-bit)
; ------------------------------------------------

[BITS 16]

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

; VBE helper variables (must be AFTER code so stage2_entry is at ORG 0x8000)
best_vbe_mode   dw 0FFFFh
mode_list_off   dw 0
mode_list_seg   dw 0

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
