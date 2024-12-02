extern gdt_descriptor
extern CODE_SEG
extern DATA_SEG
extern BEGIN_32BIT
global switchto32bit

[bits 16]
switchto32bit:
    cli                     ; 1. disable interrupts
    lgdt [gdt_descriptor]   ; 2. load GDT descriptor
    mov eax, cr0
    or al, 0x1             ; 3. enable protected mode
    mov cr0, eax
    jmp CODE_SEG:init_32bit ; 4. far jump

[bits 32]
init_32bit:
    ; Print message or halt to check if transition is successful
    mov eax, 0x1
    int 0x80 ; Call BIOS or OS interrupt if available

    ; Setup segment registers and stack
    mov ax, DATA_SEG
    mov ds, ax
    mov ss, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    mov ebp, 0x90000
    mov esp, ebp

    call BEGIN_32BIT