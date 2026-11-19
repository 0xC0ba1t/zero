//! A minimal ATA PIO driver: reads and writes 512-byte sectors of the first
//! IDE drive (primary channel, master) with 28-bit addressing.
//!
//! That is the disk QEMU gives an image passed as `-drive format=raw,file=...`
//! (and what most emulators and older machines present). It polls instead of
//! using interrupts, which keeps it very small.

use spin::Mutex;
use x86_64::instructions::port::Port;

pub const SECTOR: usize = 512;

const DATA: u16 = 0x1F0;
const SECTOR_COUNT: u16 = 0x1F2;
const LBA_LOW: u16 = 0x1F3;
const LBA_MID: u16 = 0x1F4;
const LBA_HIGH: u16 = 0x1F5;
const DRIVE: u16 = 0x1F6;
const COMMAND: u16 = 0x1F7;
const CONTROL: u16 = 0x3F6;

const STATUS_ERR: u8 = 0x01;
const STATUS_DRQ: u8 = 0x08;
const STATUS_DF: u8 = 0x20;
const STATUS_BSY: u8 = 0x80;

#[derive(Debug, PartialEq, Eq)]
pub enum DiskError {
    NoDisk,
    Timeout,
    Failed,
    OutOfRange,
}

pub struct Disk {
    /// Number of 512-byte sectors.
    pub sectors: u64,
}

/// The disk, if there is one. Taking the lock also serializes access.
pub static DISK: Mutex<Option<Disk>> = Mutex::new(None);

fn inb(port: u16) -> u8 {
    unsafe { Port::<u8>::new(port).read() }
}

fn outb(port: u16, value: u8) {
    unsafe { Port::<u8>::new(port).write(value) }
}

/// Waits until the drive is no longer busy and returns its status.
fn wait_idle() -> Result<u8, DiskError> {
    for _ in 0..4_000_000 {
        let status = inb(COMMAND);
        if status & STATUS_BSY == 0 {
            return Ok(status);
        }
    }
    Err(DiskError::Timeout)
}

/// Waits for the drive to be ready to transfer data.
fn wait_data() -> Result<(), DiskError> {
    for _ in 0..4_000_000 {
        let status = inb(COMMAND);
        if status & STATUS_BSY == 0 {
            if status & (STATUS_ERR | STATUS_DF) != 0 {
                return Err(DiskError::Failed);
            }
            if status & STATUS_DRQ != 0 {
                return Ok(());
            }
        }
    }
    Err(DiskError::Timeout)
}

/// Looks for the drive. Returns whether there is one.
pub fn init() -> bool {
    // no interrupts from the drive, we poll
    outb(CONTROL, 0x02);
    outb(DRIVE, 0xA0);
    // give the drive its 400 ns to answer the select
    for _ in 0..4 {
        inb(CONTROL);
    }
    outb(SECTOR_COUNT, 0);
    outb(LBA_LOW, 0);
    outb(LBA_MID, 0);
    outb(LBA_HIGH, 0);
    outb(COMMAND, 0xEC); // IDENTIFY
    if inb(COMMAND) == 0 {
        return false; // nothing there
    }
    if wait_idle().is_err() {
        return false;
    }
    // ATAPI and SATA devices put a signature here instead of answering
    if inb(LBA_MID) != 0 || inb(LBA_HIGH) != 0 {
        return false;
    }
    if wait_data().is_err() {
        return false;
    }
    let mut words = [0u16; 256];
    let mut data = Port::<u16>::new(DATA);
    for word in words.iter_mut() {
        *word = unsafe { data.read() };
    }
    let sectors = words[60] as u64 | (words[61] as u64) << 16;
    if sectors == 0 {
        return false;
    }
    *DISK.lock() = Some(Disk { sectors });
    true
}

fn select(lba: u64, count: u8) {
    outb(DRIVE, 0xE0 | ((lba >> 24) & 0x0F) as u8);
    outb(SECTOR_COUNT, count);
    outb(LBA_LOW, lba as u8);
    outb(LBA_MID, (lba >> 8) as u8);
    outb(LBA_HIGH, (lba >> 16) as u8);
}

/// Reads whole sectors starting at `lba`; `buf` must be a multiple of 512 bytes.
pub fn read(lba: u64, buf: &mut [u8]) -> Result<(), DiskError> {
    let guard = DISK.lock();
    let disk = guard.as_ref().ok_or(DiskError::NoDisk)?;
    let count = (buf.len() / SECTOR) as u64;
    if lba + count > disk.sectors {
        return Err(DiskError::OutOfRange);
    }
    let mut data = Port::<u16>::new(DATA);
    for (i, sector) in buf.chunks_exact_mut(SECTOR).enumerate() {
        select(lba + i as u64, 1);
        outb(COMMAND, 0x20); // READ SECTORS
        wait_data()?;
        for pair in sector.chunks_exact_mut(2) {
            let word = unsafe { data.read() };
            pair[0] = word as u8;
            pair[1] = (word >> 8) as u8;
        }
    }
    Ok(())
}

/// Writes whole sectors starting at `lba`; `buf` must be a multiple of 512 bytes.
pub fn write(lba: u64, buf: &[u8]) -> Result<(), DiskError> {
    let guard = DISK.lock();
    let disk = guard.as_ref().ok_or(DiskError::NoDisk)?;
    let count = (buf.len() / SECTOR) as u64;
    if lba + count > disk.sectors {
        return Err(DiskError::OutOfRange);
    }
    let mut data = Port::<u16>::new(DATA);
    for (i, sector) in buf.chunks_exact(SECTOR).enumerate() {
        select(lba + i as u64, 1);
        outb(COMMAND, 0x30); // WRITE SECTORS
        wait_data()?;
        for pair in sector.chunks_exact(2) {
            unsafe { data.write(pair[0] as u16 | (pair[1] as u16) << 8) };
        }
    }
    // make sure it reached the platter
    outb(COMMAND, 0xE7); // FLUSH CACHE
    wait_idle()?;
    Ok(())
}
