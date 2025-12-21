# Othello UEFI Loader

Builds a `BOOTX64.EFI` that loads `/kernel.elf` (the Rust kernel ELF linked to 0x0020_0000) and jumps to it.

This lets the kernel grow arbitrarily large (no sector-count / INT13h limits).
