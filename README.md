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

### mbr_gdt_detection.asm

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
