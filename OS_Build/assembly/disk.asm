extern print16
global disk_load
%define KERNEL_OFFSET 0x1000

section .text

disk_load:
    pusha
    push dx
    mov ah, 0x02            ; BIOS read sectors function

    mov al, dh              ; Number of sectors to read (in DH)
    mov cl, 0x02            ; Start reading from sector 2

    mov ch, 0x00            ; Cylinder 0
    mov dh, 0x00            ; Head 0

    ; es & bx reg point to 0x0000:0x1000 <- Phys Add
    mov ax, 0x0000
    mov es, ax
    mov bx, KERNEL_OFFSET   ; Load offset (0x1000) into BX

    int 0x13                ; Call BIOS to load sectors
    jc disk_error

    ; Check if sectors were read correctly
    cmp al, dh
    jne sectors_error

    ; Print a success message
    mov ax, 0x4C00
    int 0x21

disk_error:
    mov ebx, MSG_DISK_ERROR
    call print16
    jmp $

sectors_error:
    mov ebx, MSG_SECTORS_ERROR
    call print16
    jmp $

MSG_DISK_ERROR db "Disk read error!", 0
MSG_SECTORS_ERROR db "Sector mismatch error!", 0
