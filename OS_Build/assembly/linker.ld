ENTRY(start)

SECTIONS {

    . = 0x7c00;    /* MBR address */

    .text : {
        *(.text)
    }

    . = ALIGN(4K);

    .gdt : {
        KEEP(*(.gdt))
    }

    .data : {
        *(.data)
    }
    
    .bss : {
        *(.bss)
    }
}
