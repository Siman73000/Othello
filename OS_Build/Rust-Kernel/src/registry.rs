#![allow(dead_code)]

//! Minimal in-memory “Registry” for Othello OS.
//!
//! Allocation-free (no heap). Stores users under a Windows-like path:
//!   HKLM\SOFTWARE\Othello\Users\<username>
//!
//! Values stored per user:
//!   - Salt (u32)
//!   - Hash (u64)   // salted FNV-1a
//!   - CreatedTsc (u64)
//!
//! NOTE: Without a filesystem, this registry is not persistent across reboot.

use crate::time;

pub const MAX_USERS: usize = 16;
pub const MAX_USERNAME: usize = 24;
pub const MAX_PASSWORD: usize = 32;

#[derive(Clone, Copy)]
pub struct UserEntry {
    pub used: bool,
    pub name_len: u8,
    pub name: [u8; MAX_USERNAME],
    pub salt: u32,
    pub hash: u64,
    pub created_tsc: u64,
}

impl Default for UserEntry {
    fn default() -> Self {
        Self { used: false, name_len: 0, name: [0; MAX_USERNAME], salt: 0, hash: 0, created_tsc: 0 }
    }
}

static mut USERS: [UserEntry; MAX_USERS] = [UserEntry { used: false, name_len: 0, name: [0; MAX_USERNAME], salt: 0, hash: 0, created_tsc: 0 }; MAX_USERS];

pub fn init() {
    unsafe {
        for u in USERS.iter_mut() {
            *u = UserEntry::default();
        }
    }
}

pub fn user_count() -> usize {
    unsafe { USERS.iter().filter(|u| u.used).count() }
}

pub fn has_users() -> bool { user_count() != 0 }

pub fn iter_users<F: FnMut(&UserEntry)>(mut f: F) {
    unsafe {
        for u in USERS.iter() {
            if u.used { f(u); }
        }
    }
}

pub fn find_user(username: &str) -> Option<UserEntry> {
    let name = username.as_bytes();
    unsafe {
        for u in USERS.iter() {
            if !u.used { continue; }
            let n = u.name_len as usize;
            if n == name.len() && u.name[..n] == name[..] {
                return Some(*u);
            }
        }
    }
    None
}

fn find_user_index(username: &[u8]) -> Option<usize> {
    unsafe {
        for (i, u) in USERS.iter().enumerate() {
            if !u.used { continue; }
            let n = u.name_len as usize;
            if n == username.len() && u.name[..n] == username[..] { return Some(i); }
        }
    }
    None
}

fn find_free_slot() -> Option<usize> {
    unsafe {
        for (i, u) in USERS.iter().enumerate() {
            if !u.used { return Some(i); }
        }
    }
    None
}

pub fn validate_username(username: &str) -> bool {
    let b = username.as_bytes();
    if b.is_empty() || b.len() > MAX_USERNAME { return false; }
    for &ch in b {
        // allow a-z A-Z 0-9 _ - .
        let ok = (ch >= b'a' && ch <= b'z') || (ch >= b'A' && ch <= b'Z') || (ch >= b'0' && ch <= b'9') || ch == b'_' || ch == b'-' || ch == b'.';
        if !ok { return false; }
    }
    true
}

pub fn create_user(username: &str, password: &str) -> Result<(), &'static str> {
    if !validate_username(username) { return Err("Invalid username (use a-z A-Z 0-9 _ - .)"); }
    if password.is_empty() || password.len() > MAX_PASSWORD { return Err("Invalid password length"); }

    let uname = username.as_bytes();
    if find_user_index(uname).is_some() { return Err("User already exists"); }
    let slot = find_free_slot().ok_or("User database full")?;

    let t = time::rdtsc();
    let salt = (t as u32) ^ ((t >> 32) as u32).wrapping_mul(0x9E3779B9);
    let hash = salted_hash(salt, password.as_bytes());

    unsafe {
        let u = &mut USERS[slot];
        *u = UserEntry::default();
        u.used = true;
        u.name_len = uname.len() as u8;
        u.name[..uname.len()].copy_from_slice(uname);
        u.salt = salt;
        u.hash = hash;
        u.created_tsc = t;
    }
    Ok(())
}

pub fn validate_login(username: &str, password: &str) -> bool {
    let uname = username.as_bytes();
    let Some(idx) = find_user_index(uname) else { return false; };
    unsafe {
        let u = &USERS[idx];
        salted_hash(u.salt, password.as_bytes()) == u.hash
    }
}

pub fn user_entry_by_index(nth: usize) -> Option<UserEntry> {
    let mut i = 0usize;
    unsafe {
        for u in USERS.iter() {
            if !u.used { continue; }
            if i == nth { return Some(*u); }
            i += 1;
        }
    }
    None
}

// -----------------------------------------------------------------------------
// Hashing (toy, but avoids plaintext)
// -----------------------------------------------------------------------------

fn salted_hash(salt: u32, pass: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325; // FNV offset
    // Mix salt
    for &b in salt.to_le_bytes().iter() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    // Mix password
    for &b in pass {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    // Final avalanche
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
    h ^= h >> 33;
    h
}
