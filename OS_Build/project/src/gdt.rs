#![no_std]
#![no_main]

use core::mem::size_of;

#[repr(C, packed)]
struct GdtEntry {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
}

impl GdtEntry {
    const fn null() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0,
            granularity: 0,
            base_high: 0,
        }
    }

    const fn new(base: u32, limit: u32, access: u8, granularity: u8) -> Self {
        Self {
            limit_low: (limit & 0xFFFF) as u16,
            base_low: (base & 0xFFFF) as u16,
            base_middle: ((base >> 16) & 0xFF) as u8,
            access,
            granularity: ((limit >> 16) & 0x0F) as u8 | (granularity & 0xF0),
            base_high: ((base >> 24) & 0xFF) as u8,
        }
    }
}

#[repr(C, packed)]
struct GdtDescriptor {
    limit: u16,
    base: u32,
}

static GDT: [GdtEntry; 3] = [
    GdtEntry::null(), // Null descriptor
    GdtEntry::new(    // Code segment
        0x00000000,   // Base
        0xFFFFF,      // Limit
        0b10011010,   // Access
        0b11001111,   // Granularity
    ),
    GdtEntry::new(    // Data segment
        0x00000000,   // Base
        0xFFFFF,      // Limit
        0b10010010,   // Access
        0b11001111,   // Granularity
    ),
];

static GDT_DESCRIPTOR: GdtDescriptor = GdtDescriptor {
    limit: (size_of::<[GdtEntry; 3]>() - 1) as u16,
    base: GDT.as_ptr() as u32,
};

// Offsets for selectors
pub const CODE_SEG: u16 = (1 << 3) as u16; // Code segment offset
pub const DATA_SEG: u16 = (2 << 3) as u16; // Data segment offset
