extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;
use crate::security::{
    init_user_profile, lock_session, profile_summary, registry_get, registry_list, registry_set,
    security_status, unlock_with_biometric, unlock_with_password, unlock_with_pin, RegistryScope,
};

use super::{
    draw_rect, draw_text, mutate_window, prepare_window_surface, ACCENT_COLOR, FONT_HEIGHT,
    FONT_WIDTH, SUBDUED_TEXT_COLOR, TITLE_BAR_HEIGHT, TITLE_TEXT_COLOR, WindowHandle,
};

const SHELL_PADDING: usize = 12;
const SHELL_MAX_ENTRIES: usize = 96;
const SHELL_LINE_SPACING: usize = FONT_HEIGHT + 4;
const SHELL_MAX_INPUT_CHARS: usize = 160;
const SHELL_MAX_OUTPUT_CHARS: usize = 256;

#[derive(Clone)]
enum ShellEntry {
    Prompt(String),
    Output(String),
}

struct ShellState {
    window: Option<WindowHandle>,
    entries: Vec<ShellEntry>,
    input: String,
    prompt: &'static str,
}

impl ShellState {
    const fn new() -> Self {
        Self {
            window: None,
            entries: Vec::new(),
            input: String::new(),
            prompt: "othello> ",
        }
    }
}

static SHELL: Mutex<ShellState> = Mutex::new(ShellState::new());

fn shell_window_id() -> Option<WindowHandle> {
    SHELL.lock().window
}

fn shell_available_chars(handle: WindowHandle) -> usize {
    mutate_window(handle, |window| {
        let inner_width = window.width.saturating_sub(SHELL_PADDING * 2);
        if inner_width == 0 {
            0
        } else {
            inner_width / (FONT_WIDTH + 1)
        }
    })
    .unwrap_or(0)
}

fn shell_trim_entries(entries: &mut Vec<ShellEntry>) {
    while entries.len() > SHELL_MAX_ENTRIES {
        entries.remove(0);
    }
}

fn sanitize_ascii(input: &str, max_chars: usize) -> String {
    let mut sanitized = String::new();
    for ch in input.chars() {
        if sanitized.chars().count() >= max_chars {
            break;
        }

        if ch.is_ascii_graphic() || ch == ' ' {
            sanitized.push(ch);
        } else if !ch.is_control() {
            sanitized.push('?');
        }
    }

    sanitized
}

fn sanitize_output(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|line| sanitize_ascii(&line, SHELL_MAX_OUTPUT_CHARS))
        .collect()
}

fn shell_execute(command: &str) -> Vec<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return Vec::new();
    }

    let response = match parts[0] {
        "help" => vec![
            "commands: help, status, netscan, about, clear".into(),
            "security: user init|lock|unlock|status|profile, registry set|get|list".into(),
            "unlock before registry user updates; keep keys ascii".into(),
        ],
        "status" => {
            let mut lines = vec![
                "kernel: ready • scheduler idle".into(),
                "net: rtl8139 driver loaded".into(),
                "display: hdmi+dp configured".into(),
            ];
            lines.extend(security_status());
            lines
        }
        "netscan" => vec![
            "scanning interfaces for dhcp leases…".into(),
            "assigned 10.0.2.15/24 • gw 10.0.2.2".into(),
        ],
        "about" => vec![
            "othello shell • realtime micro ui".into(),
            "type commands to poke drivers or show status".into(),
        ],
        "clear" => vec!["__clear__".into()],
        "registry" => {
            if parts.len() < 2 {
                vec!["usage: registry [set|get|list] ...".into()]
            } else {
                match parts[1] {
                    "set" => {
                        if parts.len() < 5 {
                            vec!["usage: registry set <system|user> <key> <value>".into()]
                        } else {
                            let scope = if parts[2] == "system" {
                                RegistryScope::System
                            } else {
                                RegistryScope::User
                            };
                            let value = parts[4..].join(" ");
                            match registry_set(scope, parts[3], &value) {
                                Ok(msg) => vec![msg.into()],
                                Err(err) => vec![format!("registry error: {err}")],
                            }
                        }
                    }
                    "get" => {
                        if parts.len() < 4 {
                            vec!["usage: registry get <system|user> <key>".into()]
                        } else {
                            let scope = if parts[2] == "system" {
                                RegistryScope::System
                            } else {
                                RegistryScope::User
                            };
                            match registry_get(scope, parts[3]) {
                                Ok(value) => vec![format!("{value}")],
                                Err(err) => vec![format!("registry error: {err}")],
                            }
                        }
                    }
                    "list" => {
                        if parts.len() < 3 {
                            vec!["usage: registry list <system|user>".into()]
                        } else {
                            let scope = if parts[2] == "system" {
                                RegistryScope::System
                            } else {
                                RegistryScope::User
                            };
                            match registry_list(scope) {
                                Ok(entries) => entries,
                                Err(err) => vec![format!("registry error: {err}")],
                            }
                        }
                    }
                    _ => vec!["usage: registry [set|get|list] ...".into()],
                }
            }
        }
        "user" => {
            if parts.len() < 2 {
                vec!["usage: user [init|lock|unlock|status|profile] ...".into()]
            } else {
                match parts[1] {
                    "init" => {
                        if parts.len() < 4 {
                            vec!["usage: user init <username> <password> [pin] [biometric]".into()]
                        } else {
                            let pin = parts.get(4).copied();
                            let biometric = parts.get(5).copied();
                            match init_user_profile(parts[2], parts[3], pin, biometric) {
                                Ok(msg) => vec![msg.into()],
                                Err(err) => vec![format!("user init error: {err}")],
                            }
                        }
                    }
                    "lock" => vec![lock_session().into()],
                    "unlock" => {
                        if parts.len() < 5 {
                            vec!["usage: user unlock <password|pin|bio> <username> <secret>".into()]
                        } else {
                            let method = parts[2];
                            let user = parts[3];
                            let secret = parts[4];
                            match method {
                                "password" => unlock_with_password(user, secret)
                                    .map(String::from)
                                    .map_err(|e| e.into()),
                                "pin" => unlock_with_pin(user, secret)
                                    .map(String::from)
                                    .map_err(|e| e.into()),
                                "bio" | "biometric" => unlock_with_biometric(user, secret)
                                    .map(String::from)
                                    .map_err(|e| e.into()),
                                _ => Err("unknown unlock method"),
                            }
                            .map(|msg| vec![msg])
                            .unwrap_or_else(|err| vec![format!("unlock error: {err}")])
                        }
                    }
                    "status" => security_status(),
                    "profile" => {
                        if parts.len() < 3 {
                            vec!["usage: user profile <username>".into()]
                        } else {
                            match profile_summary(parts[2]) {
                                Ok(lines) => lines,
                                Err(err) => vec![format!("user error: {err}")],
                            }
                        }
                    }
                    _ => vec!["usage: user [init|lock|unlock|status|profile] ...".into()],
                }
            }
        }
        other => vec![format!("unknown command '{other}' — type 'help'")],
    };

    sanitize_output(response)
}

fn text_width(text: &str) -> usize {
    text.chars().count().saturating_mul(FONT_WIDTH + 1)
}

fn wrap_line(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if current.chars().count() >= max_chars {
            lines.push(current);
            current = String::new();
        }
        current.push(ch);
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn draw_prompt_line(window: &mut super::Window, y: usize, prompt: &str, text: &str) {
    let start_x = SHELL_PADDING;
    draw_text(window, start_x, y, prompt, ACCENT_COLOR);
    let prompt_width = text_width(prompt);
    draw_text(window, start_x + prompt_width + 2, y, text, TITLE_TEXT_COLOR);
}

fn render_shell() {
    let snapshot = {
        let shell = SHELL.lock();
        (
            shell.window,
            shell.prompt,
            shell.input.clone(),
            shell.entries.clone(),
        )
    };

    let (Some(handle), prompt, input, entries) = snapshot else {
        return;
    };

    let _ = mutate_window(handle, |window| {
        prepare_window_surface(window, super::ACTIVE_TITLE_COLOR, super::DEFAULT_BORDER_COLOR);
        let mut cursor_y = TITLE_BAR_HEIGHT + SHELL_PADDING;
        let available_chars = window
            .width
            .saturating_sub(SHELL_PADDING * 2)
            .checked_div(FONT_WIDTH + 1)
            .unwrap_or(0);

        for entry in entries.iter() {
            if cursor_y + FONT_HEIGHT >= window.height {
                break;
            }

            match entry {
                ShellEntry::Prompt(cmd) => {
                    if available_chars == 0 {
                        continue;
                    }

                    let prompt_chars = prompt.chars().count();
                    let line_capacity = available_chars
                        .saturating_sub(prompt_chars.saturating_add(1))
                        .max(1);
                    let glyphs: Vec<char> = cmd.chars().collect();
                    let first_slice: String = glyphs.iter().take(line_capacity).collect();
                    draw_prompt_line(window, cursor_y, prompt, &first_slice);
                    cursor_y = cursor_y.saturating_add(SHELL_LINE_SPACING);
                    let mut idx = line_capacity;

                    while idx < glyphs.len() && cursor_y + FONT_HEIGHT < window.height {
                        let take = (glyphs.len() - idx).min(line_capacity);
                        let segment: String = glyphs[idx..idx + take].iter().collect();
                        let indent_x = SHELL_PADDING + text_width(prompt) + 2;
                        draw_text(window, indent_x, cursor_y, &segment, TITLE_TEXT_COLOR);
                        cursor_y = cursor_y.saturating_add(SHELL_LINE_SPACING);
                        idx += take;
                    }
                }
                ShellEntry::Output(line) => {
                    for chunk in wrap_line(line, available_chars) {
                        draw_text(window, SHELL_PADDING, cursor_y, &chunk, SUBDUED_TEXT_COLOR);
                        cursor_y = cursor_y.saturating_add(SHELL_LINE_SPACING);
                        if cursor_y + FONT_HEIGHT >= window.height {
                            break;
                        }
                    }
                }
            }
        }

        if cursor_y + FONT_HEIGHT < window.height {
            draw_prompt_line(window, cursor_y, prompt, &input);
            let caret_x = SHELL_PADDING + text_width(prompt) + text_width(&input) + 1;
            draw_rect(window, caret_x, cursor_y, 2, FONT_HEIGHT, ACCENT_COLOR);
        }
    });
}

pub fn handle_keypress(active: WindowHandle, key: char) -> bool {
    let shell_id = shell_window_id();
    if shell_id != Some(active) {
        return false;
    }

    let max_chars = shell_available_chars(active);
    {
        let mut shell = SHELL.lock();
        match key {
            '\n' => {
                let command = sanitize_ascii(&shell.input, SHELL_MAX_INPUT_CHARS);
                shell.entries.push(ShellEntry::Prompt(command.clone()));
                shell_trim_entries(&mut shell.entries);

                for line in shell_execute(&command) {
                    if line == "__clear__" {
                        shell.entries.clear();
                        continue;
                    }
                    shell.entries.push(ShellEntry::Output(line));
                    shell_trim_entries(&mut shell.entries);
                }
                shell.input.clear();
            }
            '\x08' => {
                shell.input.pop();
            }
            ch if ch.is_ascii_graphic() || ch == ' ' => {
                let prompt_chars = shell.prompt.chars().count();
                let limit = max_chars
                    .saturating_sub(prompt_chars.saturating_add(1))
                    .min(SHELL_MAX_INPUT_CHARS)
                    .max(1);
                if shell.input.chars().count() < limit {
                    shell.input.push(ch);
                }
            }
            _ => {}
        }
    }

    render_shell();
    true
}

pub fn paint_shell(handle: WindowHandle) {
    {
        let mut shell = SHELL.lock();
        shell.window = Some(handle);
        shell.input.clear();
        shell.entries.clear();
        shell.entries.push(ShellEntry::Output("othello shell ready.".into()));
        shell
            .entries
            .push(ShellEntry::Output("type 'help' for available commands.".into()));
    }

    render_shell();
}
