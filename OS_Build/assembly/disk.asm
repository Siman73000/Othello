extern print16
global disk_load
global load_kernel_mbr
global load_kernel_gpt
%define KERNEL_OFFSET 0x1000

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

    mov ah, 0x02            ; BIOS read sectors function
    mov al, dh              ; Number of sectors to read (DH preserved in AL)
    mov bl, al              ; Keep expected count in BL for later comparison

    mov ch, 0x00            ; Cylinder 0
    mov dh, 0x00            ; Head 0

    ; es & bx reg point to 0x0000:0x1000 <- Phys Addr
    mov ax, 0x0000
    mov es, ax
    ; BX should already contain the offset from the caller

    int 0x13                ; Call BIOS to load sectors
    jc disk_error

    ; Check if sectors were read correctly
    cmp al, bl
    jne sectors_error

    popa
    ret

load_kernel_mbr:
    mov bx, MSG_LOAD_KERNEL_MBR
    call print16
    mov bx, KERNEL_OFFSET
    mov dh, 32
    mov cl, 0x02
    call disk_load
    ret

load_kernel_gpt:
    mov bx, MSG_LOAD_KERNEL_GPT
    call print16
    mov bx, KERNEL_OFFSET
    mov dh, 64
    mov cl, 0x03
    call disk_load
    ret

disk_error:
    mov bx, MSG_DISK_ERROR
    call print16
    jmp $

sectors_error:
    mov bx, MSG_SECTORS_ERROR
    call print16
    jmp $

MSG_DISK_ERROR db "Disk read error!", 0
MSG_LOAD_KERNEL_MBR db "Loading MBR kernel into memory...", 0
MSG_LOAD_KERNEL_GPT db "Loading GPT kernel into memory...", 0
MSG_SECTORS_ERROR db "Sector mismatch error!", 0
