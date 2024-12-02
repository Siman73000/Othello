AS = nasm
CC = i686-elf-gcc
LD = i686-elf-ld
OBJCOPY = i686-elf-objcopy


ASM_SOURCES = mbr.asm disk.asm gdt.asm switchto32bit.asm print16bit.asm print32.asm kernelentry.asm
C_SOURCES = kernel.c display.c util.c ports.c
ASM_OBJECTS = $(ASM_SOURCES:.asm=.o)
C_OBJECTS = $(C_SOURCES:.c=.o)
OBJECTS = $(ASM_OBJECTS) $(C_OBJECTS)


all: kernel.img

%.o: %.asm
	$(AS) -f elf32 $< -o $@

%.o: %.c
	$(CC) -m32 -ffreestanding -c $< -o $@

kernel.bin: $(OBJECTS)
	$(LD) -o $@ -T linker.ld $(OBJECTS)

kernel.img: kernel.bin
	$(OBJCOPY) -O binary kernel.bin $@

clean:
	rm -f $(OBJECTS) Kernel/kernel.bin/ Kernel/kernel.img/ Assembly/disk.img/
