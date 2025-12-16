#![allow(dead_code)]
// src/fs.rs
// RAM filesystem with dirty tracking for persistence.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    NotDir,
    NotFile,
    Exists,
    InvalidPath,
    ReadOnly,
}

pub type FsResult<T> = core::result::Result<T, FsError>;

/// Minimal spinlock for no_std
pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}
unsafe impl<T: Send> Sync for SpinLock<T> {}

pub struct SpinGuard<'a, T> {
    lock: &'a SpinLock<T>,
}
impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self {
        Self { locked: AtomicBool::new(false), data: UnsafeCell::new(value) }
    }
    pub fn lock(&self) -> SpinGuard<'_, T> {
        while self.locked.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
        SpinGuard { lock: self }
    }
}
impl<'a, T> core::ops::Deref for SpinGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T { unsafe { &*self.lock.data.get() } }
}
impl<'a, T> core::ops::DerefMut for SpinGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T { unsafe { &mut *self.lock.data.get() } }
}
impl<'a, T> Drop for SpinGuard<'a, T> {
    fn drop(&mut self) { self.lock.locked.store(false, Ordering::Release); }
}

#[derive(Debug, Clone)]
enum NodeKind {
    Dir { children: BTreeMap<String, usize> },
    File { data: Vec<u8> },
}

#[derive(Debug, Clone)]
struct Node {
    name: String,
    parent: Option<usize>,
    kind: NodeKind,
}

#[derive(Debug)]
pub struct RamFs {
    nodes: Vec<Node>,
    root: usize,

    // persistence tracking
    dirty_puts: BTreeMap<String, bool>,
    dirty_dels: BTreeMap<String, bool>,
}

impl RamFs {
    pub fn new() -> Self {
        let mut nodes = Vec::new();
        nodes.push(Node {
            name: "/".to_string(),
            parent: None,
            kind: NodeKind::Dir { children: BTreeMap::new() },
        });
        Self {
            nodes,
            root: 0,
            dirty_puts: BTreeMap::new(),
            dirty_dels: BTreeMap::new(),
        }
    }

    pub fn take_dirty_sets(&mut self) -> (Vec<String>, Vec<String>) {
        let puts = self.dirty_puts.keys().cloned().collect::<Vec<_>>();
        let dels = self.dirty_dels.keys().cloned().collect::<Vec<_>>();
        self.dirty_puts.clear();
        self.dirty_dels.clear();
        (puts, dels)
    }

    pub fn exists(&self, abs_path: &str) -> bool {
        self.resolve_abs(abs_path).is_ok()
    }

    pub fn is_dir(&self, abs_path: &str) -> bool {
        self.resolve_abs(abs_path).map(|idx| matches!(self.nodes[idx].kind, NodeKind::Dir{..})).unwrap_or(false)
    }

    pub fn mkdir_p(&mut self, abs_path: &str) -> FsResult<()> {
        self.mkdir_p_inner(abs_path, true)
    }

    pub fn touch(&mut self, abs_path: &str) -> FsResult<()> {
        self.touch_inner(abs_path, true)
    }

    pub fn write_all(&mut self, abs_path: &str, bytes: &[u8]) -> FsResult<()> {
        self.write_all_inner(abs_path, bytes, true)
    }

    pub fn append_all(&mut self, abs_path: &str, bytes: &[u8]) -> FsResult<()> {
        self.append_all_inner(abs_path, bytes, true)
    }

    pub fn read_all(&self, abs_path: &str) -> FsResult<Vec<u8>> {
        let idx = self.resolve_abs(abs_path)?;
        match &self.nodes[idx].kind {
            NodeKind::File { data } => Ok(data.clone()),
            _ => Err(FsError::NotFile),
        }
    }

    pub fn ls(&self, abs_path: &str) -> FsResult<Vec<String>> {
        let idx = self.resolve_abs(abs_path)?;
        match &self.nodes[idx].kind {
            NodeKind::Dir { children } => Ok(children.keys().cloned().collect()),
            _ => Err(FsError::NotDir),
        }
    }

    pub fn rm(&mut self, abs_path: &str) -> FsResult<()> {
        self.rm_inner(abs_path, true)
    }

    // ---- no-dirty variants for persistence replay ----
    pub fn mkdir_p_nodirty(&mut self, abs_path: &str) -> FsResult<()> {
        self.mkdir_p_inner(abs_path, false)
    }
    pub fn touch_nodirty(&mut self, abs_path: &str) -> FsResult<()> {
        self.touch_inner(abs_path, false)
    }
    pub fn write_all_nodirty(&mut self, abs_path: &str, bytes: &[u8]) -> FsResult<()> {
        self.write_all_inner(abs_path, bytes, false)
    }
    pub fn rm_nodirty(&mut self, abs_path: &str) -> FsResult<()> {
        self.rm_inner(abs_path, false)
    }

    // ---- internals ----
    fn mark_put(&mut self, abs_path: &str) {
        if abs_path.starts_with('/') && abs_path != "/" {
            self.dirty_puts.insert(abs_path.to_string(), true);
            self.dirty_dels.remove(abs_path);
        }
    }
    fn mark_del(&mut self, abs_path: &str) {
        if abs_path.starts_with('/') && abs_path != "/" {
            self.dirty_dels.insert(abs_path.to_string(), true);
            self.dirty_puts.remove(abs_path);
        }
    }

    fn mkdir_p_inner(&mut self, abs_path: &str, _dirty: bool) -> FsResult<()> {
        let comps = split_abs(abs_path)?;
        let mut cur = self.root;

        for name in comps {
            // First, check existence without holding a mutable borrow.
            let existing = match &self.nodes[cur].kind {
                NodeKind::Dir { children } => children.get(&name).copied(),
                _ => return Err(FsError::NotDir),
            };

            if let Some(idx) = existing {
                cur = idx;
                continue;
            }

            // Create node (no borrows held).
            let idx = self.nodes.len();
            self.nodes.push(Node {
                name: name.clone(),
                parent: Some(cur),
                kind: NodeKind::Dir { children: BTreeMap::new() },
            });

            // Insert into parent's children (new mutable borrow).
            match &mut self.nodes[cur].kind {
                NodeKind::Dir { children } => {
                    children.insert(name, idx);
                }
                _ => return Err(FsError::NotDir),
            }

            cur = idx;
        }

        Ok(())
    }

    fn touch_inner(&mut self, abs_path: &str, dirty: bool) -> FsResult<()> {
        let (parent, leaf) = parent_leaf(abs_path)?;
        let pidx = self.resolve_abs(&parent)?;

        // Check if exists without holding a mutable borrow across push()
        let exists = match &self.nodes[pidx].kind {
            NodeKind::Dir { children } => children.contains_key(&leaf),
            _ => return Err(FsError::NotDir),
        };
        if exists {
            return Ok(());
        }

        let idx = self.nodes.len();
        self.nodes.push(Node {
            name: leaf.clone(),
            parent: Some(pidx),
            kind: NodeKind::File { data: Vec::new() },
        });

        match &mut self.nodes[pidx].kind {
            NodeKind::Dir { children } => {
                children.insert(leaf, idx);
            }
            _ => return Err(FsError::NotDir),
        }

        if dirty { self.mark_put(abs_path); }
        Ok(())
    }

    fn write_all_inner(&mut self, abs_path: &str, bytes: &[u8], dirty: bool) -> FsResult<()> {
        if self.resolve_abs(abs_path).is_err() {
            self.touch_inner(abs_path, dirty)?;
        }
        let idx = self.resolve_abs(abs_path)?;
        match &mut self.nodes[idx].kind {
            NodeKind::File { data } => {
                data.clear();
                data.extend_from_slice(bytes);
                if dirty { self.mark_put(abs_path); }
                Ok(())
            }
            _ => Err(FsError::NotFile),
        }
    }

    fn append_all_inner(&mut self, abs_path: &str, bytes: &[u8], dirty: bool) -> FsResult<()> {
        if self.resolve_abs(abs_path).is_err() {
            self.touch_inner(abs_path, dirty)?;
        }
        let idx = self.resolve_abs(abs_path)?;
        match &mut self.nodes[idx].kind {
            NodeKind::File { data } => {
                data.extend_from_slice(bytes);
                if dirty { self.mark_put(abs_path); }
                Ok(())
            }
            _ => Err(FsError::NotFile),
        }
    }

    fn rm_inner(&mut self, abs_path: &str, dirty: bool) -> FsResult<()> {
        if abs_path == "/" { return Err(FsError::InvalidPath); }
        let idx = self.resolve_abs(abs_path)?;
        // Can't remove non-empty dirs
        if let NodeKind::Dir { children } = &self.nodes[idx].kind {
            if !children.is_empty() { return Err(FsError::InvalidPath); }
        }
        let parent = self.nodes[idx].parent.ok_or(FsError::InvalidPath)?;
        let name = self.nodes[idx].name.clone();
        match &mut self.nodes[parent].kind {
            NodeKind::Dir { children } => { children.remove(&name); }
            _ => return Err(FsError::NotDir),
        }
        if dirty { self.mark_del(abs_path); }
        Ok(())
    }

    fn resolve_abs(&self, abs_path: &str) -> FsResult<usize> {
        let comps = split_abs(abs_path)?;
        let mut cur = self.root;
        for name in comps {
            cur = match &self.nodes[cur].kind {
                NodeKind::Dir { children } => *children.get(&name).ok_or(FsError::NotFound)?,
                _ => return Err(FsError::NotDir),
            };
        }
        Ok(cur)
    }
}

/// Global FS instance (RAM-backed)
use core::mem::MaybeUninit;

static FS_INNER: SpinLock<MaybeUninit<RamFs>> = SpinLock::new(MaybeUninit::uninit());
static FS_READY: AtomicBool = AtomicBool::new(false);

pub struct GlobalFs;

pub struct FsGuard<'a> {
    g: SpinGuard<'a, MaybeUninit<RamFs>>,
}

impl<'a> core::ops::Deref for FsGuard<'a> {
    type Target = RamFs;
    fn deref(&self) -> &RamFs {
        unsafe { self.g.assume_init_ref() }
    }
}
impl<'a> core::ops::DerefMut for FsGuard<'a> {
    fn deref_mut(&mut self) -> &mut RamFs {
        unsafe { self.g.assume_init_mut() }
    }
}

impl GlobalFs {
    pub fn init(&self) {
        if !FS_READY.load(Ordering::Acquire) {
            let mut g = FS_INNER.lock();
            if !FS_READY.load(Ordering::Relaxed) {
                *g = MaybeUninit::new(RamFs::new());
                FS_READY.store(true, Ordering::Release);
            }
        }
    }

    pub fn lock(&self) -> FsGuard<'_> {
        self.init();
        FsGuard { g: FS_INNER.lock() }
    }
}

pub static FS: GlobalFs = GlobalFs;
/// Initialize default filesystem layout (call once at boot if persist is empty)
pub fn init_default_layout() {
    let mut fs = FS.lock();
    let _ = fs.mkdir_p_nodirty("/etc");
    let _ = fs.mkdir_p_nodirty("/home");
    let _ = fs.mkdir_p_nodirty("/home/user");
    let _ = fs.mkdir_p_nodirty("/bin");
    let _ = fs.write_all_nodirty("/etc/motd", b"Welcome to Othello OS!\nType: help, ls, cat, write, mkdir, touch, cd, pwd, sync\n");
    let _ = fs.write_all_nodirty("/home/user/readme.txt", b"This is your home directory.\n");
}

/// Convert (cwd, path) -> normalized absolute path
pub fn normalize_path(cwd: &str, path: &str) -> FsResult<String> {
    if path.is_empty() { return Err(FsError::InvalidPath); }

    let mut parts: Vec<&str> = Vec::new();

    if !path.starts_with('/') {
        for p in cwd.split('/') {
            if !p.is_empty() { parts.push(p); }
        }
    }

    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => { parts.pop(); }
            _ => parts.push(p),
        }
    }

    let mut out = String::from("/");
    out.push_str(&parts.join("/"));
    if out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    Ok(out)
}

// ---- internal helpers ----

fn split_abs(abs_path: &str) -> FsResult<Vec<String>> {
    if !abs_path.starts_with('/') { return Err(FsError::InvalidPath); }
    if abs_path == "/" { return Ok(Vec::new()); }
    let mut out = Vec::new();
    for p in abs_path.split('/') {
        if p.is_empty() { continue; }
        if p == "." || p == ".." { return Err(FsError::InvalidPath); }
        out.push(p.to_string());
    }
    Ok(out)
}

fn parent_leaf(abs_path: &str) -> FsResult<(String, String)> {
    if !abs_path.starts_with('/') { return Err(FsError::InvalidPath); }
    if abs_path == "/" { return Err(FsError::InvalidPath); }

    let mut comps: Vec<&str> = abs_path.split('/').filter(|s| !s.is_empty()).collect();
    let leaf = comps.pop().ok_or(FsError::InvalidPath)?;
    let parent = if comps.is_empty() { "/".to_string() } else { alloc::format!("/{}", comps.join("/")) };
    Ok((parent, leaf.to_string()))
}
