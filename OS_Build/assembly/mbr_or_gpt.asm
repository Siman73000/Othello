extern print16
extern print16_nl
extern switchto64bit
extern print32
extern print64
extern disk_load
extern check_partition_table
extern disk_error
extern sectors_error
extern load_kernel_mbr
extern load_kernel_gpt
extern PARTITION_TYPE
extern EXPECTED_KERNEL_SECTORS

global BEGIN_32BIT
global BOOT_DRIVE
global BEGIN_64BIT
global start
global load_kernel

KERNEL_OFFSET   equ 0x1000
KERNEL_BYTES    dd 0
KERNEL_CHECKSUM dd 0

; ---------------------------------------------------------------------------
; Stage-2 bootloader (not a 512-byte MBR)
; ---------------------------------------------------------------------------

[bits 16]   ; 16-bit Real Mode

start:
    cli                          ; Prevent interrupts while setting up segments
    xor ax, ax                   ; Ensure a known, flat real-mode base
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov bp, 0x9000               ; Set up stack
    mov sp, bp
    and sp, 0xFFF0               ; Keep stack 16-byte aligned for safer interrupts
    and bp, 0xFFF0               ; Mirror alignment in BP for predictable frame setup
    cld                          ; Clear direction flag for deterministic string ops
    mov [BOOT_DRIVE], dl         ; Store boot drive number
    sti                          ; Re-enable interrupts for BIOS services

    mov bx, MSG_16BIT_MODE
    call print16
    call print16_nl

    call check_partition_table   ; Detect PT scheme

    cmp byte [PARTITION_TYPE], 0x01
    je load_mbr_kernel
    cmp byte [PARTITION_TYPE], 0x02
    je load_gpt_kernel
    jmp partition_error

; ---------------------------------------------------------------------------
; Load kernel depending on partition scheme
; ---------------------------------------------------------------------------

load_mbr_kernel:
    mov bl, 0x01                    ; Expected partition type: MBR
    mov cx, 32                      ; Expected sector count for MBR kernel
    call validate_kernel_load
    mov dl, [BOOT_DRIVE]
    call load_kernel_mbr
    mov word [KERNEL_BYTES], 32 * 512 ; Track bytes read for integrity checking
    mov dword [KERNEL_CHECKSUM], 0    ; Clear prior checksum before recomputation
    jmp continue_boot

load_gpt_kernel:
    mov bl, 0x02                    ; Expected partition type: GPT
    mov cx, 64                      ; Expected sector count for GPT kernel
    call validate_kernel_load
    mov dl, [BOOT_DRIVE]
    call load_kernel_gpt
    mov word [KERNEL_BYTES], 64 * 512 ; Track bytes read for integrity checking
    mov dword [KERNEL_CHECKSUM], 0    ; Clear prior checksum before recomputation
    jmp continue_boot

; Generic load path (e.g., legacy usage)

load_kernel:
    mov bx, MSG_LOAD_KERNEL
    call print16
    call print16_nl

    mov bl, [PARTITION_TYPE]
    mov cx, 32
    call validate_kernel_load

    mov byte [EXPECTED_KERNEL_SECTORS], 32
    mov bx, KERNEL_OFFSET       ; Load kernel at defined offset
    mov edx, 32
    mov dl, [BOOT_DRIVE]
    call disk_load
    mov word [KERNEL_BYTES], 32 * 512
    mov dword [KERNEL_CHECKSUM], 0
    ret

continue_boot:
    cli                          ; Prevent interrupts during mode transition
    call switchto64bit
    hlt                          ; Halt if we ever return here unexpectedly
    jmp $

; ---------------------------------------------------------------------------
; Error path: unsupported / bad partition
; ---------------------------------------------------------------------------

partition_error:
    mov bx, MSG_PARTITION_ERROR
    call print16
    cli
    hlt
    jmp $

; ---------------------------------------------------------------------------
; Sanity checks before loading kernel
; ---------------------------------------------------------------------------

validate_kernel_load:
    push ax
    push bx
    push cx
    push dx

    ; Validate boot drive
    mov al, [BOOT_DRIVE]
    test al, al
    jnz .drive_ok
    mov bx, MSG_BOOT_DRIVE_INVALID
    call print16
    cli
    hlt
    jmp $

.drive_ok:
    ; Ensure detected partition type matches expectation
    cmp byte [PARTITION_TYPE], bl
    je .partition_ok
    mov bx, MSG_PARTITION_MISMATCH
    call print16
    cli
    hlt
    jmp $

.partition_ok:
    ; Ensure kernel is loaded on a paragraph boundary
    mov ax, KERNEL_OFFSET
    test ax, 0x000F
    jz .aligned
    mov bx, MSG_KERNEL_ALIGNMENT_ERROR
    call print16
    cli
    hlt
    jmp $

.aligned:
    ; Check that kernel payload does not overlap stack region
    mov ax, cx
    shl ax, 9                         ; Convert sectors to bytes
    add ax, KERNEL_OFFSET
    dec ax                            ; Last byte address of kernel payload
    cmp ax, 0x8FFF                    ; Keep clear of stack at 0x9000
    jbe .capacity_ok
    mov bx, MSG_KERNEL_RANGE_ERROR
    call print16
    cli
    hlt
    jmp $

.capacity_ok:
    mov [EXPECTED_KERNEL_SECTORS], cl ; Share expected count with disk loader

    pop dx
    pop cx
    pop bx
    pop ax
    ret

; ---------------------------------------------------------------------------
; 32-bit Protected Mode entry (called after GDT / CR0 setup elsewhere)
; ---------------------------------------------------------------------------

[bits 32]

BEGIN_32BIT:
    pushad

    mov esi, KERNEL_OFFSET
    mov ecx, [KERNEL_BYTES]
    test ecx, ecx
    jz .skip_checksum

    xor eax, eax
.checksum_loop32:
    movzx edx, byte [esi]
    add eax, edx
    inc esi
    dec ecx
    jnz .checksum_loop32

    mov [KERNEL_CHECKSUM], eax

.skip_checksum:
    popad

    mov ebx, MSG_32BIT_MODE
    call print32
    ret

; ---------------------------------------------------------------------------
; 64-bit Long Mode entry
; ---------------------------------------------------------------------------

[bits 64]

BEGIN_64BIT:
    mov rbx, MSG_64BIT_MODE
    call print64                ; Print 64-bit mode message

    call verify_kernel_integrity64

    mov rsi, KERNEL_OFFSET      ; Load kernel entry point
    call rsi                    ; Call the 64-bit kernel entry point
    hlt                         ; Halt if kernel returns
    jmp $

; ---------------------------------------------------------------------------
; 64-bit kernel integrity verification
; ---------------------------------------------------------------------------

verify_kernel_integrity64:
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi

    mov ecx, [KERNEL_BYTES]
    test ecx, ecx
    jz .integrity_ok

    mov rsi, KERNEL_OFFSET
    xor eax, eax
.checksum_loop64:
    movzx edx, byte [rsi]
    add eax, edx
    inc rsi
    dec ecx
    jnz .checksum_loop64

    cmp eax, [KERNEL_CHECKSUM]
    je .integrity_ok

    mov rbx, MSG_KERNEL_TAMPER
    call print64
    cli
    hlt
    jmp $

.integrity_ok:
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    ret

; ---------------------------------------------------------------------------
; Messages / globals (stage-2 data)
; ---------------------------------------------------------------------------

BOOT_DRIVE                    db 0
MSG_16BIT_MODE                db "Started in 16-bit Real Mode", 0
MSG_32BIT_MODE                db "Landed in 32-bit Protected Mode", 0
MSG_64BIT_MODE                db "Entered 64-bit Long Mode", 0
MSG_LOAD_KERNEL               db "Loading kernel into memory", 0
MSG_KERNEL_TAMPER             db "Kernel integrity violation detected", 0
MSG_BOOT_DRIVE_INVALID        db "Boot drive missing or invalid", 0
MSG_PARTITION_MISMATCH        db "Partition scheme changed unexpectedly", 0
MSG_KERNEL_ALIGNMENT_ERROR    db "Kernel offset not aligned", 0
MSG_KERNEL_RANGE_ERROR        db "Kernel payload overlaps stack", 0
MSG_PARTITION_ERROR           db "Unsupported or invalid partition layout", 0
