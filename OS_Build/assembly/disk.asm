extern print16
global disk_load
global load_kernel_mbr
global load_kernel_gpt
global EXPECTED_KERNEL_SECTORS
%define KERNEL_OFFSET 0x1000

section .data

EXPECTED_KERNEL_SECTORS db 0

section .text

; ---------------------------------------------------------------------------
; disk_load
; Inputs:
;   DL: BIOS drive number
;   CL: starting sector
;   DH: sector count
;   CH: cylinder (usually 0 for early boot)
;   ES:BX: destination offset (set by caller)
; Behavior:
;   Reads DH sectors starting at CL from DL into ES:BX using BIOS int 0x13.
;   Returns to the caller; on error, halts after printing a message.
; ---------------------------------------------------------------------------
disk_load:
    pusha
    cld                     ; Ensure predictable string direction for BIOS calls

    ; Defensive parameter validation before touching disk
    cmp dh, [EXPECTED_KERNEL_SECTORS]
    jne sectors_error       ; Reject unexpected sector counts

    cmp es, 0x0000
    jne load_context_error  ; Kernel loads must target the zero segment

    cmp bx, KERNEL_OFFSET
    jne load_context_error  ; Prevent overwriting boot structures

    test bx, 0x000F
    jnz load_context_error  ; Require 16-byte alignment for DMA safety

    test dh, dh             ; Validate sector count is non-zero
    jz sectors_error

    mov di, 3               ; Allow multiple retries for resilience

    mov ah, 0x02            ; BIOS read sectors function
    mov al, dh              ; Number of sectors to read (DH preserved in AL)
    mov bl, al              ; Keep expected count in BL for later comparison

    mov ch, 0x00            ; Cylinder 0
    mov dh, 0x00            ; Head 0

    ; es & bx reg point to 0x0000:0x1000 <- Phys Addr
    mov ax, 0x0000
    mov es, ax
    ; BX should already contain the offset from the caller

retry_read:
    int 0x13                ; Call BIOS to load sectors
    jc retry_or_fail_bios

    ; Check if sectors were read correctly
    cmp al, bl
    je read_ok

retry_or_fail_count:
    dec di                  ; Consume a retry budget
    jnz prepare_retry
    jmp sectors_error       ; Exhausted retries with count mismatch

retry_or_fail_bios:
    dec di                  ; Consume a retry budget
    jz disk_error           ; Give up after repeated BIOS failures

prepare_retry:
    mov ah, 0x00            ; Reset disk system before another attempt
    int 0x13
    jmp retry_read

read_ok:
    popa
    ret

load_kernel_mbr:
    mov bx, MSG_LOAD_KERNEL_MBR
    call print16
    mov byte [EXPECTED_KERNEL_SECTORS], 32
    mov bx, KERNEL_OFFSET
    mov dh, 32
    mov cl, 0x02
    call disk_load
    ret

load_kernel_gpt:
    mov bx, MSG_LOAD_KERNEL_GPT
    call print16
    mov byte [EXPECTED_KERNEL_SECTORS], 64
    mov bx, KERNEL_OFFSET
    mov dh, 64
    mov cl, 0x03
    call disk_load
    ret

disk_error:
    mov bx, MSG_DISK_ERROR
    call print16
    cli
    hlt
    jmp $

sectors_error:
    mov bx, MSG_SECTORS_ERROR
    call print16
    cli
    hlt
    jmp $

load_context_error:
    mov bx, MSG_LOAD_CONTEXT_ERROR
    call print16
    cli
    hlt
    jmp $

MSG_DISK_ERROR db "Disk read error!", 0
MSG_LOAD_KERNEL_MBR db "Loading MBR kernel into memory...", 0
MSG_LOAD_KERNEL_GPT db "Loading GPT kernel into memory...", 0
MSG_SECTORS_ERROR db "Sector mismatch error!", 0
MSG_LOAD_CONTEXT_ERROR db "Kernel load context rejected!", 0
