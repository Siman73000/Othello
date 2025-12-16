#![allow(dead_code)]
// src/ata.rs
// ATA PIO driver for QEMU IDE (primary channel, master).
//
// QEMU must attach disk.img as IDE, e.g.:
//   -drive file=disk.img,format=raw,if=ide,index=0,media=disk

use crate::portio::{inb, inw, outb, outw, io_wait};

const ATA_DATA:   u16 = 0x1F0;
const ATA_ERROR:  u16 = 0x1F1;
const ATA_SECCNT: u16 = 0x1F2;
const ATA_LBA0:   u16 = 0x1F3;
const ATA_LBA1:   u16 = 0x1F4;
const ATA_LBA2:   u16 = 0x1F5;
const ATA_HDDEV:  u16 = 0x1F6;
const ATA_STATUS: u16 = 0x1F7;
const ATA_CMD:    u16 = 0x1F7;

const CMD_IDENTIFY: u8 = 0xEC;
const CMD_READ_SECTORS: u8 = 0x20;   // LBA28
const CMD_WRITE_SECTORS: u8 = 0x30;  // LBA28

// status bits
const ST_BSY: u8 = 0x80;
const ST_DRQ: u8 = 0x08;
const ST_ERR: u8 = 0x01;

#[derive(Debug, Clone, Copy)]
pub enum AtaError {
    NoDevice,
    Error(u8),
    Timeout,
}

pub struct AtaDrive {
    pub total_sectors: u32,
}

fn poll_ready() -> Result<(), AtaError> {
    for _ in 0..1_000_000 {
        let st = unsafe { inb(ATA_STATUS) };
        if (st & ST_BSY) == 0 && (st & ST_DRQ) != 0 {
            return Ok(());
        }
        if (st & ST_ERR) != 0 {
            let err = unsafe { inb(ATA_ERROR) };
            return Err(AtaError::Error(err));
        }
    }
    Err(AtaError::Timeout)
}

fn poll_not_busy() -> Result<(), AtaError> {
    for _ in 0..1_000_000 {
        let st = unsafe { inb(ATA_STATUS) };
        if (st & ST_BSY) == 0 {
            if (st & ST_ERR) != 0 {
                let err = unsafe { inb(ATA_ERROR) };
                return Err(AtaError::Error(err));
            }
            return Ok(());
        }
    }
    Err(AtaError::Timeout)
}

pub fn identify() -> Result<AtaDrive, AtaError> {
    unsafe {
        outb(ATA_HDDEV, 0xE0); // master
        io_wait();

        outb(ATA_SECCNT, 0);
        outb(ATA_LBA0, 0);
        outb(ATA_LBA1, 0);
        outb(ATA_LBA2, 0);

        outb(ATA_CMD, CMD_IDENTIFY);
        io_wait();
    }

    let st = unsafe { inb(ATA_STATUS) };
    if st == 0 {
        return Err(AtaError::NoDevice);
    }

    poll_ready()?;

    let mut id = [0u16; 256];
    for i in 0..256 {
        id[i] = unsafe { inw(ATA_DATA) };
    }

    let total = (id[61] as u32) << 16 | (id[60] as u32);
    if total == 0 {
        return Err(AtaError::NoDevice);
    }

    Ok(AtaDrive { total_sectors: total })
}

pub fn read_sectors_lba28(lba: u32, count: u8, out: &mut [u8]) -> Result<(), AtaError> {
    if out.len() < (count as usize) * 512 {
        return Err(AtaError::Error(0xFF));
    }

    unsafe {
        outb(ATA_HDDEV, 0xE0 | (((lba >> 24) & 0x0F) as u8));
        outb(ATA_SECCNT, count);
        outb(ATA_LBA0, (lba & 0xFF) as u8);
        outb(ATA_LBA1, ((lba >> 8) & 0xFF) as u8);
        outb(ATA_LBA2, ((lba >> 16) & 0xFF) as u8);
        outb(ATA_CMD, CMD_READ_SECTORS);
    }

    let mut off = 0usize;
    for _ in 0..count {
        poll_ready()?;
        for _ in 0..256 {
            let w = unsafe { inw(ATA_DATA) };
            out[off] = (w & 0xFF) as u8;
            out[off + 1] = (w >> 8) as u8;
            off += 2;
        }
    }
    Ok(())
}

pub fn write_sectors_lba28(lba: u32, count: u8, data: &[u8]) -> Result<(), AtaError> {
    if data.len() < (count as usize) * 512 {
        return Err(AtaError::Error(0xFE));
    }

    unsafe {
        outb(ATA_HDDEV, 0xE0 | (((lba >> 24) & 0x0F) as u8));
        outb(ATA_SECCNT, count);
        outb(ATA_LBA0, (lba & 0xFF) as u8);
        outb(ATA_LBA1, ((lba >> 8) & 0xFF) as u8);
        outb(ATA_LBA2, ((lba >> 16) & 0xFF) as u8);
        outb(ATA_CMD, CMD_WRITE_SECTORS);
    }

    let mut off = 0usize;
    for _ in 0..count {
        poll_ready()?;
        for _ in 0..256 {
            let lo = data[off] as u16;
            let hi = data[off + 1] as u16;
            let w = lo | (hi << 8);
            unsafe { outw(ATA_DATA, w) };
            off += 2;
        }
        poll_not_busy()?;
    }
    Ok(())
}
