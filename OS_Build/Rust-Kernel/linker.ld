OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

SECTIONS
{
    /* Physical / virtual address where the kernel is loaded. */
    . = 0x0000000000020000;

    .text : ALIGN(4K)
    {
        KEEP(*(.text._start))   /* our Rust entrypoint */
        KEEP(*(.text.boot))
        *(.text*)
    }

    .rodata : ALIGN(4K)
    {
        *(.rodata*)
    }

    .data : ALIGN(4K)
    {
        *(.data*)
    }

    .bss : ALIGN(4K)
    {
        *(.bss*)
        *(COMMON)
    }
}
