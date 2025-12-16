#![allow(dead_code)]
// src/persist.rs
// Persistent storage: append-only key/value log stored at the END of the IDE disk.
// Replays into RamFs at boot and supports `sync` to flush dirty changes.
//
// Layout:
//   [base+0] superblock (magic + head pointer)
//   [base+1 .. base+head-1] log records
//
// Record types:
//   PUT: path -> bytes
//   DEL: path deleted
//
// This is intentionally simple + robust; we can add compaction later.

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{ata, crc32};
use crate::fs::{FS, FsError};

const SUPER_MAGIC: u32 = 0x4F46_5342; // 'OFSB'
const REC_MAGIC:   u32 = 0x4F46_5331; // 'OFS1'
const VERSION: u16 = 1;

// reserve last 32 MiB for persistent store (adjustable)
const RESERVED_SECTORS: u32 = 65536; // 65536 * 512 = 32 MiB

const KIND_PUT: u8 = 1;
const KIND_DEL: u8 = 2;

#[derive(Debug, Clone, Copy)]
pub enum PersistError {
    Ata(ata::AtaError),
    Corrupt,
    NoSpace,
    Disabled,
}

static mut ENABLED: bool = false;
static mut BASE_LBA: u32 = 0;
static mut HEAD_REL: u32 = 0; // next free sector offset from base

pub fn enabled() -> bool { unsafe { ENABLED } }

pub fn init() -> Result<(), PersistError> {
    let drive = ata::identify().map_err(PersistError::Ata)?;
    let total = drive.total_sectors;
    if total < 16_384 {
        // too small; disable
        unsafe { ENABLED = false; }
        return Err(PersistError::Disabled);
    }

    // reserve tail region
    let reserve = if total > RESERVED_SECTORS { RESERVED_SECTORS } else { total / 4 };
    let base = total - reserve;

    unsafe {
        BASE_LBA = base;
        ENABLED = true;
    }

    // load or create superblock
    let mut sec = [0u8; 512];
    ata::read_sectors_lba28(base, 1, &mut sec).map_err(PersistError::Ata)?;

    let magic = u32::from_le_bytes([sec[0], sec[1], sec[2], sec[3]]);
    if magic != SUPER_MAGIC {
        // format new superblock
        unsafe { HEAD_REL = 1; }
        write_superblock().map_err(|_| PersistError::Ata(ata::AtaError::Timeout))?;
        return Ok(());
    }

    let ver = u16::from_le_bytes([sec[4], sec[5]]);
    if ver != VERSION {
        // unknown version -> treat as disabled for now
        unsafe { ENABLED = false; }
        return Err(PersistError::Disabled);
    }

    let head = u32::from_le_bytes([sec[8], sec[9], sec[10], sec[11]]);
    let crc = u32::from_le_bytes([sec[12], sec[13], sec[14], sec[15]]);
    let calc = crc32::crc32(&sec[0..12]);
    if crc != calc {
        return Err(PersistError::Corrupt);
    }

    unsafe { HEAD_REL = head.max(1); }
    Ok(())
}

pub fn mount_into_ramfs() -> Result<(), PersistError> {
    if !enabled() { return Err(PersistError::Disabled); }

    let (base, head) = unsafe { (BASE_LBA, HEAD_REL) };
    if head <= 1 { return Ok(()); }

    // Iterate records
    let mut rel = 1u32;
    let mut sector = [0u8; 512];

    while rel < head {
        ata::read_sectors_lba28(base + rel, 1, &mut sector).map_err(PersistError::Ata)?;

        let magic = u32::from_le_bytes([sector[0], sector[1], sector[2], sector[3]]);
        if magic == 0 {
            break; // end
        }
        if magic != REC_MAGIC {
            return Err(PersistError::Corrupt);
        }

        let kind = sector[4];
        let path_len = u16::from_le_bytes([sector[6], sector[7]]) as usize;
        let data_len = u32::from_le_bytes([sector[8], sector[9], sector[10], sector[11]]) as usize;
        let crc = u32::from_le_bytes([sector[12], sector[13], sector[14], sector[15]]);

        let total_len = 16 + path_len + data_len;
        let sectors_needed = ((total_len + 511) / 512).max(1);
        let mut buf = Vec::with_capacity(sectors_needed * 512);
        buf.extend_from_slice(&sector);

        if sectors_needed > 1 {
            let mut tmp = alloc::vec![0u8; (sectors_needed - 1) * 512];
            ata::read_sectors_lba28(base + rel + 1, (sectors_needed - 1) as u8, &mut tmp).map_err(PersistError::Ata)?;
            buf.extend_from_slice(&tmp);
        }

        let payload = &buf[4..total_len]; // kind..data
        let calc = crc32::crc32(payload);
        if calc != crc {
            return Err(PersistError::Corrupt);
        }

        let path_bytes = &buf[16..16 + path_len];
        let path = core::str::from_utf8(path_bytes).map_err(|_| PersistError::Corrupt)?.to_string();
        let data = &buf[16 + path_len .. 16 + path_len + data_len];

        apply_record(kind, &path, data);

        rel += sectors_needed as u32;
    }

    Ok(())
}

fn apply_record(kind: u8, path: &str, data: &[u8]) {
    // Apply into RAM fs without marking dirty
    let mut fs = FS.lock();
    match kind {
        KIND_PUT => {
            // ensure parent dirs
            if let Ok((parent, _leaf)) = split_parent(path) {
                let _ = fs.mkdir_p(&parent);
            }
            let _ = fs.write_all_nodirty(path, data);
        }
        KIND_DEL => {
            let _ = fs.rm_nodirty(path);
        }
        _ => {}
    }
}

fn split_parent(abs_path: &str) -> Result<(String, String), FsError> {
    if !abs_path.starts_with('/') || abs_path == "/" { return Err(FsError::InvalidPath); }
    let mut comps: Vec<&str> = abs_path.split('/').filter(|s| !s.is_empty()).collect();
    let leaf = comps.pop().ok_or(FsError::InvalidPath)?;
    let parent = if comps.is_empty() { "/".to_string() } else { alloc::format!("/{}", comps.join("/")) };
    Ok((parent, leaf.to_string()))
}

fn write_superblock() -> Result<(), PersistError> {
    if !enabled() { return Err(PersistError::Disabled); }
    let base = unsafe { BASE_LBA };
    let head = unsafe { HEAD_REL };

    let mut sec = [0u8; 512];
    sec[0..4].copy_from_slice(&SUPER_MAGIC.to_le_bytes());
    sec[4..6].copy_from_slice(&VERSION.to_le_bytes());
    // [6..8] reserved
    sec[8..12].copy_from_slice(&head.to_le_bytes());
    let crc = crc32::crc32(&sec[0..12]);
    sec[12..16].copy_from_slice(&crc.to_le_bytes());

    ata::write_sectors_lba28(base, 1, &sec).map_err(PersistError::Ata)?;
    Ok(())
}

fn append_record(kind: u8, path: &str, data: &[u8]) -> Result<(), PersistError> {
    if !enabled() { return Err(PersistError::Disabled); }

    let base = unsafe { BASE_LBA };
    let head = unsafe { HEAD_REL };

    let path_b = path.as_bytes();
    let path_len = path_b.len();
    let data_len = data.len();

    // header(16) + payload
    let total_len = 16 + path_len + data_len;
    let sectors_needed = ((total_len + 511) / 512).max(1);

    // Build contiguous buffer
    let mut buf = alloc::vec![0u8; sectors_needed * 512];
    buf[0..4].copy_from_slice(&REC_MAGIC.to_le_bytes());
    buf[4] = kind;
    buf[5] = 0;
    buf[6..8].copy_from_slice(&(path_len as u16).to_le_bytes());
    buf[8..12].copy_from_slice(&(data_len as u32).to_le_bytes());

    // payload for CRC: kind..data_len + path + data
    buf[16..16+path_len].copy_from_slice(path_b);
    buf[16+path_len .. 16+path_len+data_len].copy_from_slice(data);
    let crc = crc32::crc32(&buf[4..(16+path_len+data_len)]);
    buf[12..16].copy_from_slice(&crc.to_le_bytes());

    // capacity check: keep one sector for superblock
    // (reserve size isn't explicitly tracked; we rely on tail partition big enough)
    if head + (sectors_needed as u32) >= RESERVED_SECTORS {
        return Err(PersistError::NoSpace);
    }

    ata::write_sectors_lba28(base + head, sectors_needed as u8, &buf).map_err(PersistError::Ata)?;

    unsafe { HEAD_REL = head + sectors_needed as u32; }
    write_superblock()?;
    Ok(())
}

/// Flush dirty changes from RamFs to disk log.
/// - PUT: for dirty files
/// - DEL: for deleted paths (tracked by RamFs)
pub fn sync_dirty() -> Result<usize, PersistError> {
    if !enabled() { return Err(PersistError::Disabled); }

    // Collect dirty files + deletes
    let (puts, dels) = {
        let mut fs = FS.lock();
        fs.take_dirty_sets()
    };

    let mut wrote = 0usize;

    for p in dels {
        append_record(KIND_DEL, &p, &[])?;
        wrote += 1;
    }

    for p in puts {
        let bytes = {
            let fs = FS.lock();
            match fs.read_all(&p) {
                Ok(v) => v,
                Err(_) => continue,
            }
        };
        append_record(KIND_PUT, &p, &bytes)?;
        wrote += 1;
    }

    Ok(wrote)
}

/// (Optional) wipe persistent region (dangerous; mainly for dev)
pub fn format() -> Result<(), PersistError> {
    if !enabled() { return Err(PersistError::Disabled); }
    let base = unsafe { BASE_LBA };

    // zero first 128 sectors of region (super + some log) for quick reset
    let zero = [0u8; 512];
    for i in 0..128u32 {
        ata::write_sectors_lba28(base + i, 1, &zero).map_err(PersistError::Ata)?;
    }

    unsafe { HEAD_REL = 1; }
    write_superblock()?;
    Ok(())
}
