; switchto64bit.asm
; ------------------
; Entered from 32-bit protected mode.
; - Assumes a valid GDT is already loaded.
; - Sets up minimal paging (identity map first 2 MiB).
; - Enables PAE + Long Mode + Paging.
; - Far-jumps to 64-bit code and calls BEGIN_64BIT.

extern gdt_descriptor
extern CODE_SEG_64      ; 64-bit code segment selector from your GDT
extern DATA_SEG         ; data segment selector
extern BEGIN_64BIT      ; 64-bit kernel entry (Rust side)

[bits 32]
global switchto64bit_stage2
switchto64bit_stage2:
    cli

    ; -------------------------
    ; Zero-out paging structures
    ; -------------------------
    mov edi, pml4_table
    mov ecx, 4096 / 4         ; 4 KiB / 4 bytes per dword = 1024 dwords
    xor eax, eax
    rep stosd

    mov edi, pdpt_table
    mov ecx, 4096 / 4
    xor eax, eax
    rep stosd

    mov edi, pd_table
    mov ecx, 4096 / 4
    xor eax, eax
    rep stosd

    ; ---------------------------------
    ; Link the 4-level paging hierarchy
    ; ---------------------------------
    ; PML4[0] -> PDPT | present | writable
    mov eax, pdpt_table
    or  eax, 0x03
    mov [pml4_table], eax
    mov dword [pml4_table + 4], 0

    ; PDPT[0] -> PD | present | writable
    mov eax, pd_table
    or  eax, 0x03
    mov [pdpt_table], eax
    mov dword [pdpt_table + 4], 0

    ; PD[0] -> 2 MiB page at 0x00000000
    ; bits: present (1) | writable (2) | PS (0x80) = 0x83
    mov dword [pd_table], 0x00000083
    mov dword [pd_table + 4], 0

    ; -------------------------
    ; Load PML4 into CR3
    ; -------------------------
    mov eax, pml4_table
    mov cr3, eax

    ; -------------------------
    ; Enable PAE in CR4
    ; -------------------------
    mov eax, cr4
    or  eax, 0x20              ; PAE
    mov cr4, eax

    ; -------------------------
    ; Enable Long Mode in EFER
    ; -------------------------
    mov ecx, 0xC0000080        ; IA32_EFER MSR
    rdmsr
    or  eax, 0x100             ; LME bit
    wrmsr

    ; -------------------------
    ; Enable paging (and ensure PE is on)
    ; -------------------------
    mov eax, cr0
    or  eax, 0x80000001        ; PG | PE
    mov cr0, eax

    ; -------------------------
    ; Now we are in Long Mode (once the jump completes)
    ; Far jump to 64-bit code segment
    ; -------------------------
    jmp CODE_SEG_64:long_mode_entry

[bits 64]
long_mode_entry:
    ; Set up flat segments
    mov ax, DATA_SEG
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; 64-bit stack (adjust to taste)
    mov rsp, 0x0000000000090000
    and rsp, ~0xF               ; keep 16-byte alignment

    ; Jump into your 64-bit kernel entry point
    mov rax, 0x0000000000100000   ; 1 MiB physical address
    jmp rax


.hang:
    hlt
    jmp .hang

; -------------------------
; Paging structures
; -------------------------
section .bss
align 4096
pml4_table:
    resb 4096

align 4096
pdpt_table:
    resb 4096

align 4096
pd_table:
    resb 4096
