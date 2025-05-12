# Othello OS

**Version:** 1.0

## Bare-Metal Integration

> Everything in this OS is hand-crafted in pure x86_64 Assembly, running directly on the hardware.

### Global Descriptor Table (GDT)

The GDT defines your CPU’s memory segments and their permissions (Read/Write/Execute), and it drives the transitions between:

- **Real Mode** (16-bit)  
- **Protected Mode** (32-bit)  
- **Long Mode** (64-bit)

#### 32-bit Protected-Mode Descriptor Layout


  | Bits   | Field                              |
  |:-------|:----------------------------------:|
  | 0–15   | Seg Limit (low 16 bits)            |
  | 16–31  | Base Address (low 16 bits)         |
  | 32–39  | Base Address (middle 8 bits)       |
  | 40–43  | Access Byte                        |
  | 44–47  | Flags and Seg Limit (high 4 bits)  |
  | 48–55  | Base Address (high 8 bits)         |
  | 56–63  | Reserved for Future Uses           |


#### Access Byte Bit Breakdown

  | Bits   | Field                                                     |
  |:-------|:---------------------------------------------------------:|
  | 0      | (Accessed) Set by CPU when seg is accessed                |
  | 1      | (Write/Read) Data write, code read                        |
  | 2      | (Direction/Conforming) Expands down data or conforms code |
  | 3      | (Executable) 1 = Code Seg, 0 = Data Seg                   |
  | 4      | (Descriptor Type) 1 = Code/Data, 0 = System               |
  | 5      | (DPL0-DPL1) Descriptor Privilege Level / ring             |
  | 6      | (Present) 1 = Seg is valid                                |


### Disk

`disk.asm` provides the low-level routines that your bootloader uses to read the kernel image off disk via BIOS interrupt 0x13 and place it into memory at a fixed offset.

#### Externals & Globals
- **extern** `print16`  
  A 16-bit print routine used for status and error messages.
- **global** `disk_load`  
  The core sector‐read function.

#### Constants
- `KERNEL_OFFSET` (0x1000)  
  Physical memory offset (in paragraphs) where the kernel will be loaded.

#### Entry Points / API
1. **`disk_load`**  
   - **Inputs:**  
     - `DH` = number of sectors to read  
     - `CL` = starting sector (e.g. 0x02)  
     - `CH` = cylinder (here, 0)  
     - `DH` = head (here, 0)  
   - **Behavior:**  
     1. Sets up ES:BX = `0x0000:KERNEL_OFFSET`  
     2. Calls `int 0x13` with AH=0x02 to read sectors  
     3. On carry-set, jumps to `disk_error`  
     4. Verifies sector count matches, else jumps to `sectors_error`  
     5. On success, returns to caller  
2. **`load_kernel_mbr`**  
   - Loads the first 32 sectors (MBR-style) at `KERNEL_OFFSET`  
   - Prints `MSG_LOAD_KERNEL_MBR` before calling `disk_load`  
3. **`load_kernel_gpt`**  
   - Loads the first 64 sectors (GPT-style) at `KERNEL_OFFSET`  
   - Prints `MSG_LOAD_KERNEL_GPT` before calling `disk_load`

#### Error Handlers
- **`disk_error`**  
  Prints `MSG_DISK_ERROR` and halts.
- **`sectors_error`**  
  Prints `MSG_SECTORS_ERROR` and halts.

#### Message Strings
```asm
MSG_DISK_ERROR       db "Disk read error!", 0
MSG_LOAD_KERNEL_MBR  db "Loading MBR kernel into memory...", 0
MSG_LOAD_KERNEL_GPT  db "Loading GPT kernel into memory...", 0
MSG_SECTORS_ERROR    db "Sector mismatch error!", 0
```
### MBR/GDT Detection

**Purpose:**  
Detect whether the disk uses an MBR or GPT partition table (or neither), then print a status message via BIOS teletype.

---

#### Externals & Globals
- **extern** `print`  
  BIOS‐teletype routine (AH=0x0E) for outputting characters.
- **global** `check_partition_table`  
  Entry point for probing the disk.

#### Constants & Messages
```asm
MBR_SIGNATURE       dw 0xAA55            ; MBR magic at offset 510
GPT_SIGNATURE       db "EFI PART"        ; GPT magic in header
MBR_MSG             db "MBR Detected",0
GPT_MSG             db "GPT Detected",0
NO_PARTITION_MSG    db "No Valid Partition Table Found",0
```


### Kernel Entry Point

**Purpose:**  
Initialize the CPU for 64-bit operation, set up segment registers and stack, then jump into your C/Rust kernel entry point.

---

#### Externals & Globals
- **extern** `kernel_main`  
  Your high-level kernel’s entry function (written in C, Rust, etc.).
- **global** `_start`, `kernel_entry`  
  Entry symbols the linker/bootloader will use.

---

#### Layout

```asm
[bits 64]

global kernel_entry
extern kernel_main

section .text
global _start

kernel_entry:
    ; 1) Set up data segment registers for long mode
    mov ax, 0x10             ; DATA_SEG selector (64-bit GDT entry)
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; 2) Initialize the stack pointer
    mov rsp, 0x90000         ; Point RSP at your kernel stack

    ; 3) Call into the high-level kernel
    call kernel_main

    ; 4) If kernel_main ever returns, halt here forever
    hlt
    jmp $
```


### 16-bit Print Operations

**Purpose:**  
A collection of BIOS‐teletype (AH=0x0E) routines for outputting text and basic primitives in 16-bit real mode.

---

#### Exports
```asm
global print16
global print16_nl
```


### 32-bit Print Operations

**Purpose:**  
A simple 32-bit protected-mode routine that writes a null-terminated ASCII string directly into VGA text-mode memory (0xB8000) with a fixed white-on-black attribute.

---

#### Exports
```asm
global print32
```


### Switch from 16-bit to 32-bit

**Purpose:**  
Perform a three-stage CPU mode switch:  
1. Real mode → 32-bit protected mode  
2. Protected mode setup (segments, stack)  
3. 32-bit → 64-bit long mode  

---

#### Externals & Globals
```asm
extern gdt_descriptor    ; GDT descriptor (limit + base) for LGDT
extern CODE_SEG         ; 32-bit code segment selector
extern DATA_SEG         ; 32-bit data segment selector
extern CODE_SEG_64      ; 64-bit code segment selector
extern BEGIN_32BIT      ; Optional 32-bit entry routine
extern BEGIN_64BIT      ; 64-bit entry routine
extern pml4_table       ; Physical address of your PML4 page table
global  switchto64bit   ; Entry point for the transition
```


### Switching from 32-bit to 64-bit

**Purpose:**  
Enable Physical Address Extension (PAE), turn on long mode, and jump into 64-bit kernel code.

---

#### Externals & Globals
```asm
extern gdt_descriptor    ; GDT limit & base for LGDT
extern CODE_SEG_64      ; 64-bit code segment selector
extern DATA_SEG         ; 64-bit data segment selector
extern BEGIN_64BIT      ; 64-bit kernel entry point
global  switchto64bit   ; Entry to transition from protected to long mode
```
