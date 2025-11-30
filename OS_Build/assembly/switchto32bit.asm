extern gdt_descriptor
extern CODE_SEG
extern DATA_SEG
extern CODE_SEG_64
extern BEGIN_32BIT
extern BEGIN_64BIT
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
    mov ax, DATA_SEG
    mov ds, ax
    mov ss, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    mov ebp, 0x90000
    mov esp, ebp

    call BEGIN_32BIT        ; Optional: Call 32-bit entry

    ; Enable long mode in IA32_EFER
    mov ecx, 0xC0000080     ; IA32_EFER MSR
    rdmsr
    or eax, 0x100           ; Set LME (Long Mode Enable)
    wrmsr

    ; Enable PAE (Physical Address Extension)
    mov eax, cr4
    or eax, 0x20
    mov cr4, eax

    ; Load PML4 table address into CR3
    mov eax, pml4_table
    mov cr3, eax

    ; Enable paging (PG bit) and long mode (LMA in CR0)
    mov eax, cr0
    or eax, 0x80000001
    mov cr0, eax

    jmp CODE_SEG_64:init_64bit ; Far jump to 64-bit mode

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