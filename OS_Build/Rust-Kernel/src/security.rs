#![allow(dead_code)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

const MAX_KEY_LEN: usize = 72;
const MAX_VALUE_LEN: usize = 256;
const MAX_USERNAME_LEN: usize = 32;
const MAX_PROFILES: usize = 8;
const MIN_PASSWORD_LEN: usize = 6;
const MAX_SECRET_LEN: usize = 64;
const INITIAL_SALT: u64 = 0x9e37_79b9_7f4a_7c15;

#[derive(Clone, Copy)]
pub enum RegistryScope {
    System,
    User,
}

#[derive(Clone)]
struct UserProfile {
    username: String,
    password_hash: u64,
    pin_hash: Option<u64>,
    biometric_hash: Option<u64>,
    salt: u64,
}

struct Registry {
    system: BTreeMap<String, String>,
    user: BTreeMap<String, BTreeMap<String, String>>,
}

impl Registry {
    fn new() -> Self {
        Self {
            system: BTreeMap::new(),
            user: BTreeMap::new(),
        }
    }

    fn set(&mut self, scope: RegistryScope, owner: Option<&str>, key: &str, value: &str) {
        match scope {
            RegistryScope::System => {
                self.system.insert(key.into(), value.into());
            }
            RegistryScope::User => {
                if let Some(user) = owner {
                    let entry = self.user.entry(user.into()).or_insert_with(BTreeMap::new);
                    entry.insert(key.into(), value.into());
                }
            }
        }
    }

    fn get(&self, scope: RegistryScope, owner: Option<&str>, key: &str) -> Option<String> {
        match scope {
            RegistryScope::System => self.system.get(key).cloned(),
            RegistryScope::User => owner.and_then(|user| self.user.get(user)).and_then(|m| m.get(key)).cloned(),
        }
    }

    fn list(&self, scope: RegistryScope, owner: Option<&str>) -> Vec<String> {
        match scope {
            RegistryScope::System => self
                .system
                .iter()
                .map(|(k, v)| format!("{k} = {v}"))
                .collect(),
            RegistryScope::User => owner
                .and_then(|user| self.user.get(user))
                .map(|map| map.iter().map(|(k, v)| format!("{k} = {v}")))
                .into_iter()
                .flatten()
                .collect(),
        }
    }
}

struct SecurityState {
    registry: Registry,
    profiles: Vec<UserProfile>,
    session_locked: bool,
    active_user: Option<String>,
}

impl SecurityState {
    const fn new() -> Self {
        Self {
            registry: Registry {
                system: BTreeMap::new(),
                user: BTreeMap::new(),
            },
            profiles: Vec::new(),
            session_locked: true,
            active_user: None,
        }
    }
}

static SECURITY: Mutex<SecurityState> = Mutex::new(SecurityState::new());
static NEXT_SALT: AtomicU64 = AtomicU64::new(INITIAL_SALT);

fn reset_salt() {
    NEXT_SALT.store(INITIAL_SALT, Ordering::SeqCst);
}

fn next_salt() -> u64 {
    NEXT_SALT.fetch_add(0x1000_0001, Ordering::SeqCst)
}

fn constant_time_eq(a: u64, b: u64) -> bool {
    let mut diff = 0u64;
    diff |= a ^ b;
    diff == 0
}

fn simple_hash(secret: &str, salt: u64) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325 ^ salt;
    for byte in secret.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_1b3);
        hash ^= hash >> 33;
    }
    hash
}

fn sanitize_ascii(input: &str, max: usize) -> Result<String, &'static str> {
    let mut out = String::new();
    for ch in input.chars() {
        if out.chars().count() >= max {
            break;
        }

        if ch.is_ascii_alphanumeric() || ['_', '-', '.', '/', ':', '#'].contains(&ch) {
            out.push(ch);
        } else {
            return Err("invalid characters detected");
        }
    }

    if out.is_empty() {
        return Err("value cannot be empty");
    }

    Ok(out)
}

fn sanitize_value(input: &str, max: usize) -> Result<String, &'static str> {
    let mut out = String::new();
    for ch in input.chars() {
        if out.chars().count() >= max {
            break;
        }

        if ch.is_ascii_graphic() || ch == ' ' {
            out.push(ch);
        } else if !ch.is_control() {
            out.push('?');
        }
    }

    if out.is_empty() {
        return Err("value cannot be empty");
    }

    Ok(out)
}

fn update_session_registry(registry: &mut Registry, locked: bool) {
    let state = if locked { "locked" } else { "unlocked" };
    registry.set(RegistryScope::System, None, "security.session", state);
}

fn find_profile<'a>(profiles: &'a mut [UserProfile], username: &str) -> Option<&'a mut UserProfile> {
    profiles.iter_mut().find(|p| p.username == username)
}

pub fn security_status() -> Vec<String> {
    let state = SECURITY.lock();
    let mut lines = Vec::new();
    lines.push(format!("session: {}", if state.session_locked { "locked" } else { "unlocked" }));
    match state.active_user.as_ref() {
        Some(user) => lines.push(format!("active user: {user}")),
        None => lines.push("active user: none".into()),
    }
    lines.push(format!("registered users: {}", state.profiles.len()));
    lines
}

pub fn initialize_security() -> &'static str {
    let mut guard = SECURITY.lock();
    guard.registry = Registry::new();
    guard.profiles.clear();
    guard.session_locked = true;
    guard.active_user = None;
    reset_salt();

    // Seed a few system registry entries so the shell has non-empty, validated data to read.
    guard
        .registry
        .set(RegistryScope::System, None, "os.vendor", "othello");
    guard
        .registry
        .set(RegistryScope::System, None, "os.boot", "cold-start");
    update_session_registry(&mut guard.registry, true);

    "security state initialized"
}

pub fn init_user_profile(
    username: &str,
    password: &str,
    pin: Option<&str>,
    biometric: Option<&str>,
) -> Result<&'static str, &'static str> {
    let sanitized_user = sanitize_ascii(username, MAX_USERNAME_LEN)?;
    if password.chars().count() < MIN_PASSWORD_LEN || password.chars().count() > MAX_SECRET_LEN {
        return Err("password length out of bounds");
    }

    let pin_value = if let Some(pin) = pin {
        if pin.chars().count() < 4 || pin.chars().count() > MAX_SECRET_LEN {
            return Err("pin must be 4-64 characters");
        }
        Some(pin)
    } else {
        None
    };

    let biometric_value = if let Some(bio) = biometric {
        if bio.chars().count() < 4 || bio.chars().count() > MAX_SECRET_LEN {
            return Err("biometric token must be 4-64 characters");
        }
        Some(bio)
    } else {
        None
    };

    let mut guard = SECURITY.lock();
    if guard.profiles.len() >= MAX_PROFILES {
        return Err("profile limit reached");
    }
    if guard.profiles.iter().any(|p| p.username == sanitized_user) {
        return Err("user already exists");
    }

    let salt = next_salt();
    let profile = UserProfile {
        username: sanitized_user.clone(),
        password_hash: simple_hash(password, salt),
        pin_hash: pin_value.map(|p| simple_hash(p, salt)),
        biometric_hash: biometric_value.map(|b| simple_hash(b, salt)),
        salt,
    };

    guard.profiles.push(profile);
    guard.session_locked = false;
    guard.active_user = Some(sanitized_user);
    Ok("user initialized and session unlocked")
}

pub fn lock_session() -> &'static str {
    let mut guard = SECURITY.lock();
    guard.session_locked = true;
    guard.active_user = None;
    update_session_registry(&mut guard.registry, true);
    "session locked"
}

pub fn unlock_with_password(username: &str, password: &str) -> Result<&'static str, &'static str> {
    let sanitized_user = sanitize_ascii(username, MAX_USERNAME_LEN)?;
    let mut guard = SECURITY.lock();
    let Some(profile) = find_profile(&mut guard.profiles, &sanitized_user) else {
        return Err("user not found");
    };

    let candidate = simple_hash(password, profile.salt);
    if constant_time_eq(candidate, profile.password_hash) {
        guard.session_locked = false;
        guard.active_user = Some(sanitized_user);
        update_session_registry(&mut guard.registry, false);
        return Ok("unlocked with password");
    }

    Err("invalid password")
}

pub fn unlock_with_pin(username: &str, pin: &str) -> Result<&'static str, &'static str> {
    let sanitized_user = sanitize_ascii(username, MAX_USERNAME_LEN)?;
    let mut guard = SECURITY.lock();
    let Some(profile) = find_profile(&mut guard.profiles, &sanitized_user) else {
        return Err("user not found");
    };

    let Some(pin_hash) = profile.pin_hash else {
        return Err("pin not set");
    };

    let candidate = simple_hash(pin, profile.salt);
    if constant_time_eq(candidate, pin_hash) {
        guard.session_locked = false;
        guard.active_user = Some(sanitized_user);
        update_session_registry(&mut guard.registry, false);
        return Ok("unlocked with pin");
    }

    Err("invalid pin")
}

pub fn unlock_with_biometric(username: &str, token: &str) -> Result<&'static str, &'static str> {
    let sanitized_user = sanitize_ascii(username, MAX_USERNAME_LEN)?;
    let mut guard = SECURITY.lock();
    let Some(profile) = find_profile(&mut guard.profiles, &sanitized_user) else {
        return Err("user not found");
    };

    let Some(stored) = profile.biometric_hash else {
        return Err("biometric token not set");
    };

    let candidate = simple_hash(token, profile.salt);
    if constant_time_eq(candidate, stored) {
        guard.session_locked = false;
        guard.active_user = Some(sanitized_user);
        update_session_registry(&mut guard.registry, false);
        return Ok("unlocked with biometric token");
    }

    Err("invalid biometric token")
}

fn require_unlocked_user(state: &SecurityState) -> Result<String, &'static str> {
    if state.session_locked {
        Err("session locked; unlock first")
    } else {
        match state.active_user.as_ref() {
            Some(user) => Ok(user.clone()),
            None => Err("session unlocked without active user"),
        }
    }
}

pub fn registry_set(scope: RegistryScope, key: &str, value: &str) -> Result<&'static str, &'static str> {
    let sanitized_key = sanitize_ascii(key, MAX_KEY_LEN)?;
    let sanitized_value = sanitize_value(value, MAX_VALUE_LEN)?;

    let mut guard = SECURITY.lock();
    let owner = match scope {
        RegistryScope::System => None,
        RegistryScope::User => Some(require_unlocked_user(&guard)?),
    };

    guard
        .registry
        .set(scope, owner.as_deref(), &sanitized_key, &sanitized_value);
    Ok("registry updated")
}

pub fn registry_get(scope: RegistryScope, key: &str) -> Result<String, &'static str> {
    let sanitized_key = sanitize_ascii(key, MAX_KEY_LEN)?;
    let guard = SECURITY.lock();
    let owner = match scope {
        RegistryScope::System => None,
        RegistryScope::User => Some(require_unlocked_user(&guard)?),
    };

    guard
        .registry
        .get(scope, owner.as_deref(), &sanitized_key)
        .ok_or("entry not found")
}

pub fn registry_list(scope: RegistryScope) -> Result<Vec<String>, &'static str> {
    let guard = SECURITY.lock();
    let owner = match scope {
        RegistryScope::System => None,
        RegistryScope::User => Some(require_unlocked_user(&guard)?),
    };

    let entries = guard.registry.list(scope, owner.as_deref());
    if entries.is_empty() {
        Ok(vec!["registry empty".into()])
    } else {
        Ok(entries)
    }
}

pub fn profile_summary(username: &str) -> Result<Vec<String>, &'static str> {
    let sanitized_user = sanitize_ascii(username, MAX_USERNAME_LEN)?;
    let guard = SECURITY.lock();
    let Some(profile) = guard.profiles.iter().find(|p| p.username == sanitized_user) else {
        return Err("user not found");
    };

    let mut lines = Vec::new();
    lines.push(format!("user: {sanitized_user}"));
    lines.push("password: set".into());
    lines.push(format!("pin: {}", if profile.pin_hash.is_some() { "set" } else { "not set" }));
    lines.push(format!(
        "biometric: {}",
        if profile.biometric_hash.is_some() {
            "set"
        } else {
            "not set"
        }
    ));
    Ok(lines)
}
