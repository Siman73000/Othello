;____________________________________________________________________________________
;
;   Detects if the Hardware is using MBR/GPT PT and or BIOS/UEFI
;
;   This code scans the hardware for a valid partition tabe.
;   If a valid MBR table is found it loads the MBR.
;   If a valid GPT table is found it loads the GPT.
;   If no valid table is found it defaults to MBR.
;
;____________________________________________________________________________________

section .data
global PARTITION_TYPE
PARTITION_TYPE db 0
MBR_SIGNATURE dw 0xaa55
GPT_SIGNATURE db "EFI PART" ; GPT Signature in
MBR_MSG db "MBR Detected", 0
GPT_MSG db "GPT Detected", 0
NO_PARTITION_MSG db "No Valid Partition Table Found", 0

section .bss
buffer resb 512     ; The buffer used to store sector data

section .text
global check_partition_table

;   Function used to check the hardware for a valid PT
check_partition_table:
    ; Load the first sector of the MBR disk
    mov bx, buffer
    xor ah, ah      ; Function 02h: Read sectors
    mov al, 1       ; Read 1 sector
    mov ch, 0       ; Cylinder 0
    mov cl, 1       ; Sector 1
    mov dh, 0       ; Head 0
    int 0x13        ; BIOS interrupt
    jc no_partition ; Jump if read error

    ; Check MBR Signature
    mov si, buffer
    add si, 510     ; Offset to MBR Signature
    cmp word [si], MBR_SIGNATURE
    je is_mbr       ; If matches it's MBR

    ; Load the 2nd sector (GPT header)
    mov bx, buffer
    xor ah, ah
    mov al, 1
    mov ch, 0
    mov cl, 2
    mov dh, 0
    int 0x13
    jc no_partition

    ; Check GPT Signature
    mov si, buffer
    cmp dword [si], 'EFIP'      ; First 4 bytes of GPT header "EFI PART"
    jne no_partition
    cmp dword [si + 4], ' TRA'
    je is_gpt


    ; Basic print functions
no_partition:
    mov byte [PARTITION_TYPE], 0
    mov bx, NO_PARTITION_MSG
    call print
    ret

is_mbr:
    mov byte [PARTITION_TYPE], 1
    mov bx, MBR_MSG
    call print
    ret

is_gpt:
    mov byte [PARTITION_TYPE], 2
    mov bx, GPT_MSG
    call print
    ret

; Assumes BIOS teletype output
print:
    mov ah, 0x0E
.next_char:
    lodsb
    test al, al
    jz .done
    int 0x10
    jmp .next_char
.done:
    ret