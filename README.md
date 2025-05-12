# Othello OS

**Version:** 1.0

## Overview

Othello is a minimal, hand-crafted operating system written in x86_64 Assembly, Rust, and C. It demonstrates the complete CPU boot process—from real mode through protected mode and into long mode—while providing a GUI and CLI for development or general use.

### Goals
- **Education:** Expose each step of the mode transitions and hardware setup.  
- **Modularity:** Keep subsystems (GDT, disk loader, partition detector, mode switcher, print routines, kernel entry) cleanly separated.  
- **Simplicity:** Use a flat memory model and identity-mapped paging to minimize complexity.  
- **Extensibility:** Provide clear “hooks” (`BEGIN_32BIT`, `BEGIN_64BIT`, `kernel_main`, etc.) so you can drop in your own kernel logic.  
