global print64
[bits 64]

VIDEO_MEMORY equ 0xb8000
WHITE_ON_BLACK equ 0x0f

print64:
    push rax
    push rbx
    push rcx
    push rdx

    mov rdx, VIDEO_MEMORY

.print64_loop:
    mov al, [rbx]
    mov ah, WHITE_ON_BLACK

    cmp al, 0
    je .print64_done

    mov [rdx], ax
    inc rbx
    add rdx, 2
    jmp .print64_loop

.print64_done:
    pop rdx
    pop rcx
    pop rbx
    pop rax
    ret
