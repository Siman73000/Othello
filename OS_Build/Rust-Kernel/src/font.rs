#![allow(dead_code)]
pub const FONT_W: usize = 8;
pub const FONT_H: usize = 16;

#[rustfmt::skip]
pub static FONT8X8: [[u8; 8]; 128] = include!("font8x8_basic.incl");

/// Return the 8-bit row mask for a glyph at a 16px font height.
/// We scale 8px->16px by duplicating each row.
#[inline]
pub fn glyph_row(ch: u8, row16: usize) -> u8 {
    let row8 = (row16 / 2).min(7);
    FONT8X8[ch as usize][row8]
}
