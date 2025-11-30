extern gdt_descriptor
extern CODE_SEG_64
extern DATA_SEG
extern BEGIN_64BIT

[bits 32]
global switchto64bit_stage2
switchto64bit_stage2:
    cli                     ; Disable interrupts
    lgdt [gdt_descriptor]   ; Load GDT descriptor
    mov eax, cr4
    or eax, 0x20            ; Enable PAE
    mov cr4, eax

    mov ecx, 0xC0000080     ; MSR address for EFER
    rdmsr
    or eax, 0x100           ; Set LME (bit 8)
    wrmsr

    mov eax, cr0
    or eax, 0x80000000      ; Enable paging (PG bit)
    mov cr0, eax

    jmp 0x08:long_mode_entry  ; Far jump to 64-bit code

[bits 64]
long_mode_entry:
    mov ax, DATA_SEG         ; Load data segment selectors

    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    mov rsp, 0x90000         ; Set stack pointer

    call BEGIN_64BIT         ; Jump to 64-bit kernel
