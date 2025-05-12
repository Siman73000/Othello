# Othello

Operating System Build Version 1.0

## Bare-Metal Integration

All of the following are manually programmed in x86_64 Assembly.

- GPT
The GPT (Global Descriptor Table) defines memory segments for the CPU to utilize as well as enforces memory Access Control RWE (Read, Write, and Execute permissions). This program also has x86 memory protection and transitions between 16-bit real mode, 32-bit protected mode, and 64-bit long mode.
  - Descriptor Layout for 32-bit Protected Mode

  | Bits   | Field                              |
  |:-------|:-----------------------------------|
  | 0–15   | Seg Limit (low 16 bits)            |
  | 16–31  | Base Address (low 16 bits)         |
  | 32–39  | Base Address (middle 8 bits)       |
  | 40–43  | Access Byte                        |
  | 44–47  | Flags and Seg Limit (high 4 bits)  |
  | 48–55  | Base Address (high 8 bits)         |
  | 56–63  | Reserved for Future Uses           |
