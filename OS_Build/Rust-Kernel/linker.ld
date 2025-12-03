OUTPUT_FORMAT(elf64-x86-64)
ENTRY(kernel_entry)
SECTIONS {
    /* Link address must match KERNEL_LOAD_PHYS in stage2.asm */
    . = 0x00020000;

    .text :
    {
        *(.text._start)      /* make sure _start is at the beginning */
        *(.text*)
    }

    .rodata : ALIGN(4096)
    {
        *(.rodata*)
    }

    .data : ALIGN(4096)
    {
        *(.data*)
    }

    .bss : ALIGN(4096)
    {
        *(.bss*)
        *(COMMON)
    }

    /DISCARD/ :
    {
        *(.eh_frame*)
        *(.comment*)
    }
}
