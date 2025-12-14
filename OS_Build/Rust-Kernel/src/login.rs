#![allow(dead_code)]
use crate::gui;

pub fn show_login_screen() {
    let x = gui::shell_left() + 12;
    let y = gui::shell_top() + 36;
    gui::draw_text(x, y, "Login screen not implemented yet.", gui::SHELL_FG_COLOR, gui::SHELL_BG_COLOR);
}
