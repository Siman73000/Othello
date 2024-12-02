global print16, print16_nl

print16:
    pusha

print16_loop:
    mov al, [bx]
    cmp al, 0
    je print16_done

    mov ah, 0x0e ; tty
    int 0x10
    add bx, 1
    jmp print16_loop

print16_done:
    popa
    ret

print16_nl:
    pusha

    mov ah, 0x0e
    mov al, 0x0a ; newline char
    int 0x10
    mov al, 0x0d ; carriage return
    int 0x10

    popa
    ret

print16_cls:
    pusha

    mov ah, 0x00
    mov al, 0x03  ; text mode 80x25 16 colours for simplicity might change later
    int 0x10

    popa
    ret

print16_hex:
    pusha

    mov cx, 0

print16_hex_loop:
    cmp cx, 4
    je print16_hex_end

    ; If there's a god, it does not reside here.

    ; converting last char of 'dx' to ASCII
    mov ax, dx
    and ax, 0x000f ; 0x1234 -> 0x0004 by masking first three to zeros
    add al, 0x30 ; add 0x30 to N to convert it to ASCII "N"
    cmp al, 0x39
    jle print16_hex_step2
    add al, 7

print16_hex_step2:
    mov bx, PRINT16_HEX_OUT + 5
    sub bx, cx
    mov [bx], al ; copy the ASCII char on 'al' to the position pointed by 'bx'
    ror dx, 4 ; 0x1234 -> 0x4123 -> 0x3412 -> 0x2341 -> 0x1234
    add cx, 1
    jmp print16_hex_loop

print16_hex_end:
    mov bx, PRINT16_HEX_OUT
    call print16
    popa
    ret

PRINT16_HEX_OUT:
    db '0x0000', 0  ; ZEEERO