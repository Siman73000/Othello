#![allow(dead_code)]
// src/fs_cmds.rs
// Shell command helpers for the RamFS + persistence sync.

extern crate alloc;

use alloc::string::{String, ToString};
use crate::fs::{self, FS, FsError, SpinLock};
use crate::persist;

static CWD: SpinLock<String> = SpinLock::new(String::new());

pub fn init_cwd() {
    let mut g = CWD.lock();
    if g.is_empty() { *g = "/".to_string(); }
}

pub fn cwd() -> String {
    CWD.lock().clone()
}

pub fn try_handle(cmd: &str, args: &[&str]) -> Option<String> {
    match cmd {
        "pwd" => Some(cmd_pwd()),
        "cd" => Some(cmd_cd(args)),
        "ls" => Some(cmd_ls(args)),
        "cat" => Some(cmd_cat(args)),
        "mkdir" => Some(cmd_mkdir(args)),
        "touch" => Some(cmd_touch(args)),
        "rm" => Some(cmd_rm(args)),
        "write" => Some(cmd_write(args, false)),
        "append" => Some(cmd_write(args, true)),
        "sync" => Some(cmd_sync()),
        "persist" => Some(cmd_persist(args)),
        _ => None,
    }
}

fn cmd_pwd() -> String { cwd() }

fn cmd_cd(args: &[&str]) -> String {
    let target = args.get(0).copied().unwrap_or("/");
    let cur = cwd();
    let abs = match fs::normalize_path(&cur, target) {
        Ok(p) => p,
        Err(e) => return alloc::format!("cd: {e:?}"),
    };
    let fsg = FS.lock();
    if !fsg.exists(&abs) { return "cd: not found".to_string(); }
    if !fsg.is_dir(&abs) { return "cd: not a directory".to_string(); }
    drop(fsg);
    *CWD.lock() = abs;
    String::new()
}

fn cmd_ls(args: &[&str]) -> String {
    let path = args.get(0).copied().unwrap_or(".");
    let cur = cwd();
    let abs = match fs::normalize_path(&cur, path) {
        Ok(p) => p,
        Err(e) => return alloc::format!("ls: {e:?}"),
    };
    let fsg = FS.lock();
    match fsg.ls(&abs) {
        Ok(items) => {
            if items.is_empty() { return String::new(); }
            let mut out = String::new();
            for it in items {
                out.push_str(&it);
                out.push('\n');
            }
            out
        }
        Err(FsError::NotDir) => "ls: not a directory".to_string(),
        Err(FsError::NotFound) => "ls: not found".to_string(),
        Err(e) => alloc::format!("ls: {e:?}"),
    }
}

fn cmd_cat(args: &[&str]) -> String {
    let path = match args.get(0) {
        Some(p) => *p,
        None => return "cat: missing path".to_string(),
    };
    let cur = cwd();
    let abs = match fs::normalize_path(&cur, path) {
        Ok(p) => p,
        Err(e) => return alloc::format!("cat: {e:?}"),
    };
    let fsg = FS.lock();
    match fsg.read_all(&abs) {
        Ok(bytes) => {
            match core::str::from_utf8(&bytes) {
                Ok(s) => s.to_string(),
                Err(_) => {
                    let mut out = String::new();
                    for b in bytes {
                        out.push_str(&alloc::format!("{:02X} ", b));
                    }
                    out
                }
            }
        }
        Err(FsError::NotFile) => "cat: not a file".to_string(),
        Err(FsError::NotFound) => "cat: not found".to_string(),
        Err(e) => alloc::format!("cat: {e:?}"),
    }
}

fn cmd_mkdir(args: &[&str]) -> String {
    let path = match args.get(0) { Some(p) => *p, None => return "mkdir: missing path".to_string() };
    let cur = cwd();
    let abs = match fs::normalize_path(&cur, path) {
        Ok(p) => p,
        Err(e) => return alloc::format!("mkdir: {e:?}"),
    };
    let mut fsg = FS.lock();
    match fsg.mkdir_p(&abs) {
        Ok(()) => String::new(),
        Err(FsError::NotDir) => "mkdir: parent not a directory".to_string(),
        Err(e) => alloc::format!("mkdir: {e:?}"),
    }
}

fn cmd_touch(args: &[&str]) -> String {
    let path = match args.get(0) { Some(p) => *p, None => return "touch: missing path".to_string() };
    let cur = cwd();
    let abs = match fs::normalize_path(&cur, path) {
        Ok(p) => p,
        Err(e) => return alloc::format!("touch: {e:?}"),
    };
    let mut fsg = FS.lock();
    match fsg.touch(&abs) {
        Ok(()) => String::new(),
        Err(e) => alloc::format!("touch: {e:?}"),
    }
}

fn cmd_rm(args: &[&str]) -> String {
    let path = match args.get(0) { Some(p) => *p, None => return "rm: missing path".to_string() };
    let cur = cwd();
    let abs = match fs::normalize_path(&cur, path) {
        Ok(p) => p,
        Err(e) => return alloc::format!("rm: {e:?}"),
    };
    let mut fsg = FS.lock();
    match fsg.rm(&abs) {
        Ok(()) => String::new(),
        Err(FsError::InvalidPath) => "rm: directory not empty or invalid".to_string(),
        Err(FsError::NotFound) => "rm: not found".to_string(),
        Err(e) => alloc::format!("rm: {e:?}"),
    }
}

fn cmd_write(args: &[&str], append: bool) -> String {
    if args.len() < 2 {
        return if append { "append: usage: append <path> <text...>" } else { "write: usage: write <path> <text...>" }.to_string();
    }
    let path = args[0];
    let text = join_tail(args, 1);

    let cur = cwd();
    let abs = match fs::normalize_path(&cur, path) {
        Ok(p) => p,
        Err(e) => return alloc::format!("write: {e:?}"),
    };

    let mut fsg = FS.lock();
    let bytes = text.as_bytes();
    let res = if append { fsg.append_all(&abs, bytes) } else { fsg.write_all(&abs, bytes) };
    match res {
        Ok(()) => String::new(),
        Err(e) => alloc::format!("write: {e:?}"),
    }
}

fn cmd_sync() -> String {
    if !persist::enabled() {
        return "sync: persistence disabled (no IDE drive?)".to_string();
    }
    match persist::sync_dirty() {
        Ok(n) => alloc::format!("sync: wrote {n} record(s)"),
        Err(e) => alloc::format!("sync: error {e:?}"),
    }
}

fn cmd_persist(args: &[&str]) -> String {
    let sub = args.get(0).copied().unwrap_or("status");
    match sub {
        "status" => {
            if persist::enabled() { "persist: enabled".to_string() } else { "persist: disabled".to_string() }
        }
        "format" => {
            match persist::format() {
                Ok(()) => "persist: formatted".to_string(),
                Err(e) => alloc::format!("persist: format failed {e:?}"),
            }
        }
        _ => "persist: usage: persist [status|format]".to_string()
    }
}

fn join_tail(args: &[&str], start: usize) -> String {
    let mut out = String::new();
    for (i, a) in args.iter().enumerate().skip(start) {
        if i > start { out.push(' '); }
        out.push_str(a);
    }
    out
}
