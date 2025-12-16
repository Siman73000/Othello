#![allow(dead_code)]
pub const FONT_W: usize = 8;
pub const FONT_H: usize = 16;

#[rustfmt::skip]
// NOTE:
// Some bare-metal link/build pipelines accidentally omit or fail to load `.rodata`.
// If that happens, all immutable `static` data (fonts, cursor bitmaps, string literals)
// can appear as zeroes at runtime, which makes *all text/cursors invisible*.
//
// To be resilient, place the font table in `.data` (which most kernels already load).
#[link_section = ".data"]
pub static FONT8X8: [[u8; 8]; 128] = include!("font8x8_basic.incl");

/// Return the 8-bit row mask for a glyph at a 16px font height.
/// We scale 8px->16px by duplicating each row.
#[inline]
pub fn glyph_row(ch: u8, row16: usize) -> u8 {
    let row8 = (row16 / 2).min(7);
    FONT8X8[ch as usize][row8]
}
