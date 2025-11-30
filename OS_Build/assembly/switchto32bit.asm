extern gdt_descriptor
extern CODE_SEG
extern DATA_SEG
extern CODE_SEG_64
extern BEGIN_32BIT
extern BEGIN_64BIT
extern print32
global switchto64bit

[bits 16]
switchto64bit:
    cli                     ; Disable interrupts
    lgdt [gdt_descriptor]   ; Load GDT descriptor
    mov eax, cr0
    or al, 0x1              ; Enable protected mode
    mov cr0, eax
    jmp CODE_SEG:init_32bit ; Far jump to 32-bit mode

[bits 32]
init_32bit:
    cld                     ; Ensure deterministic string operation direction
    mov ax, DATA_SEG
    mov ds, ax
    mov ss, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    mov ebp, 0x90000
    and ebp, 0xFFFFFFF0     ; Maintain 16-byte stack alignment before use
    mov esp, ebp

    ; Scrub page table structures to prevent reuse of stale or injected mappings
    mov edi, pml4_table
    mov ecx, 4096 / 4
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

    ; Rebuild a minimal, identity-mapped paging hierarchy
    mov dword [pml4_table], (pdpt_table | 0x03)
    mov dword [pml4_table + 4], 0

    mov dword [pdpt_table], (pd_table | 0x03)
    mov dword [pdpt_table + 4], 0

    mov dword [pd_table], 0x00000083   ; 2MB identity mapping entry
    mov dword [pd_table + 4], 0

    ; Verify CPUID is supported before relying on feature flags
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
    jz cpu_feature_failure   ; CPUID not supported
    push ecx                 ; Restore original EFLAGS
    popfd

    ; Capture the maximum supported standard CPUID leaf for optional hardening
    mov eax, 0x0
    cpuid
    mov esi, eax

    ; Confirm extended CPUID functions reach at least 0x80000001
    mov eax, 0x80000000
    cpuid
    cmp eax, 0x80000001
    jb cpu_feature_failure

    ; Require long-mode and NX support for execution hardening
    mov eax, 0x80000001
    cpuid
    test edx, 1 << 29        ; Long mode available
    jz cpu_feature_failure
    test edx, 1 << 20        ; NX bit available
    jz nx_unsupported

    ; Harden CR0 early to catch faults and enforce write protection
    mov eax, cr0
    or eax, (1 << 1) | (1 << 5) | (1 << 16) ; MP, NE, WP
    and eax, ~(1 << 18)                       ; Clear AM to avoid mask bypass
    mov cr0, eax

    call BEGIN_32BIT        ; Optional: Call 32-bit entry

    ; Verify paging structures remained untouched before enabling paging
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

    ; Enable long mode in IA32_EFER
    mov ecx, 0xC0000080     ; IA32_EFER MSR
    rdmsr
    or eax, 0x900           ; Set LME (bit 8) and NXE (bit 11)
    wrmsr

    ; Enable PAE (Physical Address Extension) and supervisor protections when available
    mov edx, cr4
    or edx, 0x20

    cmp esi, 0x7
    jb skip_supervisor_exec_protections

    mov eax, 0x7
    xor ecx, ecx
    cpuid
    test ebx, 1 << 7         ; SMEP support
    jz maybe_smap
    or edx, 1 << 20

maybe_smap:
    test ebx, 1 << 20        ; SMAP support
    jz skip_supervisor_exec_protections
    or edx, 1 << 21

skip_supervisor_exec_protections:
    mov cr4, edx

    ; Load PML4 table address into CR3
    mov eax, pml4_table
    mov cr3, eax

    ; Enable paging (PG bit) and long mode (LMA in CR0)
    mov eax, cr0
    or eax, 0x80000001 | (1 << 1) | (1 << 5) | (1 << 16)
    mov cr0, eax

    jmp CODE_SEG_64:init_64bit ; Far jump to 64-bit mode

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

[bits 64]
init_64bit:
    mov ax, DATA_SEG
    mov ds, ax
    mov ss, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    mov rsp, 0x90000        ; Setup stack for 64-bit mode
    call BEGIN_64BIT        ; Call 64-bit entry point

    hlt                     ; Hang
    jmp $

section .bss
align 4096
pml4_table:
    dq pdpt_table | 0x03

align 4096
pdpt_table:
    dq pd_table | 0x03
    times 511 dq 0


align 4096
pd_table:
    dq 0x0000000000000083   ; 2MB identity mapping for lowest region
    times 511 dq 0

section .rodata
MSG_PAGING_TAMPER db "Paging structures were modified; halting", 0
