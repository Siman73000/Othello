# Othello

Operating System Build Version 1.0

## Bare-Metal Integration

All of the following are manually programmed in x86_64 Assembly.

- GPT
The GPT (Global Descriptor Table) defines memory segments for the CPU to utilize as well as enforces memory Access Control RWE (Read, Write, and Execute permissions). This program also has x86 memory protection and transitions between 16-bit real mode, 32-bit protected mode, and 64-bit long mode.
  - Descriptor Layout for 32-bit Protected Mode
  | Register Numbers | Use Case |
  |:-----------------|---------:|
  | 0-15             | Seg Limit|
