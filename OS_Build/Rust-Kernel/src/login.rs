#![allow(dead_code)]

use crate::gui::{draw_text, shell_left, shell_top, SHELL_FG_COLOR};
use crate::serial_write_str;

pub fn show_login_screen() {
    serial_write_str("Login screen stub.\n");

    let x = shell_left() + 8;
    let y = shell_top() + 8;

    draw_text(x, y, "Login screen not implemented yet.", SHELL_FG_COLOR);
}
