extern gdt_descriptor
extern CODE_SEG
extern DATA_SEG
extern CODE_SEG_64
extern BEGIN_32BIT
extern BEGIN_64BIT
extern print32

global switchto64bit

; --------------------------------------
; 16-bit: jump into 32-bit protected mode
; --------------------------------------
[bits 16]
switchto64bit:
    cli                     ; Disable interrupts
    lgdt [gdt_descriptor]   ; Load GDT descriptor

    mov eax, cr0
    or al, 0x1              ; Enable protected mode (PE bit)
    mov cr0, eax

    jmp CODE_SEG:init_32bit ; Far jump to 32-bit mode via 32-bit code segment

; --------------------------------------
; 32-bit: set up paging, check CPU features, enter long mode
; --------------------------------------
[bits 32]
init_32bit:
    cld                     ; Ensure deterministic string op direction

    ; Set up flat segments
    mov ax, DATA_SEG
    mov ds, ax
    mov ss, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    ; Set up a 32-bit stack (16-byte aligned)
    mov ebp, 0x90000
    and ebp, 0xFFFFFFF0
    mov esp, ebp

    ; ---------------------------------------------------
    ; Scrub page table structures to prevent stale mappings
    ; ---------------------------------------------------
    mov edi, pml4_table
    mov ecx, 4096 / 4       ; 4 KiB / 4 bytes per dword
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

    ; ---------------------------------------------------
    ; Build a minimal identity-mapped paging hierarchy:
    ;   PML4[0] -> PDPT | 0x3
    ;   PDPT[0] -> PD   | 0x3
    ;   PD[0]   -> 2 MiB identity map | 0x83
    ; ---------------------------------------------------

    ; PML4[0] = pdpt_table | 0x03
    mov eax, pdpt_table
    or  eax, 0x03
    mov [pml4_table], eax
    mov dword [pml4_table + 4], 0

    ; PDPT[0] = pd_table | 0x03
    mov eax, pd_table
    or  eax, 0x03
    mov [pdpt_table], eax
    mov dword [pdpt_table + 4], 0

    ; PD[0] = 2 MiB identity mapping, present + write + PS
    mov dword [pd_table], 0x00000083
    mov dword [pd_table + 4], 0

    ; ---------------------------------------------------
    ; Verify CPUID support
    ; ---------------------------------------------------
    pushfd
    pop eax
    mov ecx, eax
    xor eax, 0x00200000     ; Attempt to toggle ID bit
    push eax
    popfd
    pushfd
    pop eax
    xor eax, ecx
    test eax, 0x00200000
    jz cpu_feature_failure  ; CPUID not supported
    push ecx                ; Restore original EFLAGS
    popfd

    ; Capture maximum supported standard CPUID leaf
    mov eax, 0x0
    cpuid
    mov esi, eax            ; EAX = max std leaf

    ; Confirm extended CPUID functions reach at least 0x80000001
    mov eax, 0x80000000
    cpuid
    cmp eax, 0x80000001
    jb cpu_feature_failure

    ; Require long-mode and NX support
    mov eax, 0x80000001
    cpuid
    test edx, 1 << 29       ; Long mode available
    jz cpu_feature_failure
    test edx, 1 << 20       ; NX bit available
    jz nx_unsupported

    ; Harden CR0 early: MP, NE, WP; clear AM
    mov eax, cr0
    or  eax, (1 << 1) | (1 << 5) | (1 << 16) ; MP, NE, WP
    and eax, ~(1 << 18)                      ; Clear AM
    mov cr0, eax

    ; Optional 32-bit hook
    call BEGIN_32BIT

    ; ---------------------------------------------------
    ; Verify paging structures remained untouched
    ; ---------------------------------------------------
    mov edi, pml4_table + 8
    mov ecx, (4096 - 8) / 4
.check_pml4:
    mov edx, [edi]
    test edx, edx
    jnz paging_tamper_detected
    add edi, 4
    loop .check_pml4

    mov edi, pdpt_table + 8
    mov ecx, (4096 - 8) / 4
.check_pdpt:
    mov edx, [edi]
    test edx, edx
    jnz paging_tamper_detected
    add edi, 4
    loop .check_pdpt

    mov edi, pd_table + 8
    mov ecx, (4096 - 8) / 4
.check_pd:
    mov edx, [edi]
    test edx, edx
    jnz paging_tamper_detected
    add edi, 4
    loop .check_pd

    ; ---------------------------------------------------
    ; Enable long mode in IA32_EFER (LME + NXE)
    ; ---------------------------------------------------
    mov ecx, 0xC0000080     ; IA32_EFER
    rdmsr
    or  eax, 0x900          ; LME (bit 8) | NXE (bit 11)
    wrmsr

    ; Enable PAE and (optionally) SMEP/SMAP
    mov edx, cr4
    or  edx, 0x20           ; PAE

    cmp esi, 0x7
    jb  skip_supervisor_exec_protections

    mov eax, 0x7
    xor ecx, ecx
    cpuid
    test ebx, 1 << 7        ; SMEP
    jz   maybe_smap
    or   edx, 1 << 20

maybe_smap:
    test ebx, 1 << 20       ; SMAP
    jz   skip_supervisor_exec_protections
    or   edx, 1 << 21

skip_supervisor_exec_protections:
    mov cr4, edx

    ; Load PML4 into CR3
    mov eax, pml4_table
    mov cr3, eax

    ; Enable paging + keep protections
    mov eax, cr0
    or  eax, 0x80000001 | (1 << 1) | (1 << 5) | (1 << 16)
    mov cr0, eax

    ; Far jump to 64-bit mode
    jmp CODE_SEG_64:init_64bit

paging_tamper_detected:
    mov ebx, MSG_PAGING_TAMPER
    call print32
    cli
    hlt
    jmp $

cpu_feature_failure:
    cli
    hlt
    jmp $

nx_unsupported:
    cli
    hlt
    jmp $

; --------------------------------------
; 64-bit long mode entry
; --------------------------------------
[bits 64]
init_64bit:
    mov ax, DATA_SEG
    mov ds, ax
    mov ss, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    mov rsp, 0x90000        ; 64-bit stack
    call BEGIN_64BIT        ; Hand off to 64-bit entry

    hlt
    jmp $

; --------------------------------------
; Paging structures (zeroed at runtime)
; --------------------------------------
section .bss
align 4096
pml4_table:
    resq 512                ; 4 KiB

align 4096
pdpt_table:
    resq 512                ; 4 KiB

align 4096
pd_table:
    resq 512                ; 4 KiB

; --------------------------------------
; Read-only messages
; --------------------------------------
section .rodata
MSG_PAGING_TAMPER db "Paging structures were modified; halting", 0
