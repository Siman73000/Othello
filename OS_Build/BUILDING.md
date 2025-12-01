# Building Othello OS into an ISO

The repository is split into three major pieces under `OS_Build/`:

- `assembly/`: the real-mode/protected-mode bootloader and mode-switching stubs.
- `kernel/`: C helpers that can be linked into the high-level kernel.
- `Rust-Kernel/`: the Rust kernel that is entered via `kernel_entry`.

The outline below shows one way to assemble, link, and wrap everything into a bootable ISO on a Linux host. The commands assume you start at the repository root.

## Prerequisites

Install the toolchain components that match the sources:

- Rust (stable) with `cargo`.
- `nasm` (assembler for the bootloader pieces).
- `clang` or `gcc` that can emit freestanding 64-bit objects (`-ffreestanding -m64`).
- `ld.lld` (or `ld`) and `objcopy` for producing flat binaries.
- `xorriso` (or `mkisofs`/`genisoimage`) to wrap the disk image as an El Torito ISO.

## 1) Build the Rust kernel

```bash
cd OS_Build/Rust-Kernel
# Install the bare-metal target once
rustup target add $(pwd)/bare_metal.json

# Build a release kernel using the repository linker script
cargo build --release

# Strip the ELF into a flat binary the bootloader can read
objcopy -O binary target/bare_metal/release/rust-kernel ../build/kernel.bin
cd -
```

The linker script at `OS_Build/Rust-Kernel/src/linker.ld` fixes the load address at `0x0010_0000` (1 MiB) and keeps the ELF sections in `.text`, `.rodata`, `.data`, and `.bss` contiguous.

## 2) Assemble the bootloader and mode switcher

All of the 16/32/64-bit boot code lives in `OS_Build/assembly/`. You can assemble each module to an object file, then link them into a single flat boot sector + loader image. A minimal invocation that pulls in every helper looks like this:

```bash
cd OS_Build/assembly
mkdir -p ../build/boot

# Assemble each stage as 64-bit ELF objects (the sources mix [bits 16], [bits 32], and [bits 64])
for src in print16bit.asm print32.asm print64.asm disk.asm mbr_gdt_detection.asm mbr_or_gpt.asm switchto32bit.asm switchto64bit.asm kernelentry.asm; do
  nasm -f elf64 "$src" -o "../build/boot/${src%.asm}.o"
done

# Link the objects into a single bootable image at 0x7c00, keep the 0xAA55 signature,
# and emit a flat binary the BIOS can execute.
ld.lld -nostdlib -Ttext 0x7c00 -o ../build/boot/bootloader.elf ../build/boot/*.o
objcopy -O binary ../build/boot/bootloader.elf ../build/boot/bootloader.bin
cd -
```

> **Note:** `mbr_or_gpt.asm` currently reserves space for the `0xAA55` boot signature and expects to fit in the first 512 bytes. If you add code and the object grows beyond a single sector, trim the new logic or move it into a second-stage loader that you read after the initial sector.

## 3) Build the C helpers (optional)

If you want the C helpers in `OS_Build/kernel/` and `OS_Build/drivers/` linked into your kernel image, compile them to objects that can be passed to the final linker:

```bash
cd OS_Build
mkdir -p build/c
clang -ffreestanding -m64 -c kernel/kernel.c kernel/util.c drivers/display.c drivers/ports.c -Ikernel -Idrivers -o build/c/kernel.o
cd -
```

You can then add `build/c/kernel.o` to the `ld.lld` link line in step 2 so the bootable binary contains those routines before `kernel_entry` hands off to the Rust side.

## 4) Combine into a bootable disk image

Concatenate the boot sector/loader and the kernel payload into a raw disk image that the BIOS will treat like an MBR disk. The bootloader in `mbr_or_gpt.asm` reads 32 (MBR) or 64 (GPT) sectors from LBA 2 onward, so pad the kernel binary accordingly:

```bash
cd OS_Build
mkdir -p build

# Start the disk image with the 512-byte bootloader
cp build/boot/bootloader.bin build/othello.img

# Pad the kernel to the sector count your loader expects (e.g., 64 sectors)
python - <<'PY'
from pathlib import Path
kernel = Path('build/kernel.bin').read_bytes()
sector = 512
padded_len = ((len(kernel) + sector - 1) // sector) * sector
kernel += b'\x00' * (padded_len - len(kernel))
Path('build/kernel.pad').write_bytes(kernel)
PY

# Append the padded kernel starting at LBA 2
cat build/kernel.pad >> build/othello.img
cd -
```

## 5) Wrap as an ISO image

Use `xorriso` (or `mkisofs`) to expose the raw disk image as an El Torito boot image. BIOSes will execute the first sector (`0xAA55` signature) and the loader will pull in the kernel sectors you appended.

```bash
cd OS_Build
mkdir -p build/isofiles/boot
touch build/isofiles/boot/placeholder   # keep the directory in the ISO
xorriso -as mkisofs \
  -b boot/othello.img \
  -no-emul-boot \
  -boot-load-size 4 \
  -boot-info-table \
  -o build/othello.iso \
  build
cd -
```

You can then boot-test with QEMU:

```bash
qemu-system-x86_64 -cdrom OS_Build/build/othello.iso -boot d -m 512M
```

That end-to-end flow assembles the bootloader, links in the Rust kernel (plus optional C helpers), stitches them together into a raw disk image, and wraps the result into an ISO that a PC firmware can boot.
