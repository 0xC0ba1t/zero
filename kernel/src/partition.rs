//! The persistent partition at the end of the disk image.
//!
//! `build.rs` appends it to both images (100 MiB at a fixed offset). It holds
//! a small header, a table of read-only assets (the Doom game data), the
//! settings, and a saved copy of the file system. The layout must match
//! `build.rs`.

use crate::disk::{self, SECTOR};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

const MAGIC: &[u8; 8] = b"ZEROPART";

pub struct Partition {
    /// Where the partition starts on the disk, in sectors.
    start: u64,
    /// Its size in sectors.
    pub sectors: u64,
    table_lba: u64,
    pub settings_lba: u64,
    pub tree_lba: u64,
    pub tree_sectors: u64,
}

pub static PARTITION: Mutex<Option<Partition>> = Mutex::new(None);

/// Set while the partition is being erased: nothing else may write to it any more.
pub static RESETTING: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// How many steps `wipe_step` needs.
pub const WIPE_STEPS: u64 = 17;

/// Erases the settings and the saved file system, a piece at a time (the read-only game data
/// stays). Returns true after the last piece.
pub fn wipe_step(i: u64) -> bool {
    RESETTING.store(true, core::sync::atomic::Ordering::SeqCst);
    let (start, settings_lba, tree_lba, tree_sectors) = match PARTITION.lock().as_ref() {
        Some(p) => (p.start, p.settings_lba, p.tree_lba, p.tree_sectors),
        None => return true,
    };
    let zeros = vec![0u8; 1024 * SECTOR];
    if i == 0 {
        let _ = disk::write(start + settings_lba, &zeros[..8 * SECTOR]);
    } else {
        let first = (i - 1) * 1024;
        if first < tree_sectors {
            let n = (tree_sectors - first).min(1024) as usize;
            let _ = disk::write(start + tree_lba + first, &zeros[..n * SECTOR]);
        }
    }
    i + 1 >= WIPE_STEPS
}

/// A file in the read-only asset area.
#[derive(Clone, Copy)]
pub struct Asset {
    /// First sector, relative to the partition.
    pub lba: u64,
    /// Size in bytes.
    pub size: u64,
}

fn le32(bytes: &[u8], at: usize) -> u64 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as u64
}

fn le64(bytes: &[u8], at: usize) -> u64 {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(raw)
}

/// Finds the disk and the partition on it. Returns whether the partition exists.
pub fn init() -> bool {
    if !disk::init() {
        return false;
    }
    let total = match disk::DISK.lock().as_ref() {
        Some(d) => d.sectors,
        None => return false,
    };
    // the partition starts on a 1 MiB boundary: look for its header there
    let mut sector = [0u8; SECTOR];
    let mut lba = 2048;
    while lba + 16 < total {
        if disk::read(lba, &mut sector).is_ok() && &sector[..8] == MAGIC {
            *PARTITION.lock() = Some(Partition {
                start: lba,
                sectors: le64(&sector, 16),
                table_lba: le32(&sector, 24),
                settings_lba: le32(&sector, 28),
                tree_lba: le32(&sector, 32),
                tree_sectors: le32(&sector, 36),
            });
            return true;
        }
        lba += 2048;
    }
    false
}

pub fn present() -> bool {
    PARTITION.lock().is_some()
}

/// Reads sectors relative to the partition.
pub fn read(lba: u64, buf: &mut [u8]) -> bool {
    let start = match PARTITION.lock().as_ref() {
        Some(p) => p.start,
        None => return false,
    };
    disk::read(start + lba, buf).is_ok()
}

/// Writes sectors relative to the partition.
pub fn write(lba: u64, buf: &[u8]) -> bool {
    if RESETTING.load(core::sync::atomic::Ordering::SeqCst) {
        return false;
    }
    let start = match PARTITION.lock().as_ref() {
        Some(p) => p.start,
        None => return false,
    };
    disk::write(start + lba, buf).is_ok()
}

/// Looks an asset up by name.
pub fn asset(name: &str) -> Option<Asset> {
    let table_lba = PARTITION.lock().as_ref()?.table_lba;
    let mut table = vec![0u8; 8 * SECTOR];
    if !read(table_lba, &mut table) {
        return None;
    }
    for entry in table.chunks_exact(64) {
        let len = entry[..48].iter().position(|&b| b == 0).unwrap_or(48);
        if len > 0 && &entry[..len] == name.as_bytes() {
            return Some(Asset { lba: le64(entry, 48), size: le64(entry, 56) });
        }
    }
    None
}

impl Asset {
    /// Reads `buf.len()` bytes starting at byte `offset` of the asset.
    /// Returns how many bytes were read.
    pub fn read_at(&self, offset: u64, buf: &mut [u8]) -> usize {
        if offset >= self.size || buf.is_empty() {
            return 0;
        }
        let want = (buf.len() as u64).min(self.size - offset) as usize;
        let first = offset / SECTOR as u64;
        let last = (offset + want as u64 - 1) / SECTOR as u64;
        let mut raw = vec![0u8; ((last - first + 1) as usize) * SECTOR];
        if !read(self.lba + first, &mut raw) {
            return 0;
        }
        let skip = (offset % SECTOR as u64) as usize;
        buf[..want].copy_from_slice(&raw[skip..skip + want]);
        want
    }
}

/// Storage for a length-prefixed blob in the sectors `lba..lba + max_sectors`:
/// the first sector holds a magic number, the length and a checksum.
pub fn save_blob(lba: u64, max_sectors: u64, magic: &[u8; 8], data: &[u8]) -> bool {
    let payload_sectors = (data.len() + SECTOR - 1) / SECTOR;
    if payload_sectors as u64 + 1 > max_sectors {
        return false;
    }
    let mut payload = vec![0u8; payload_sectors * SECTOR];
    payload[..data.len()].copy_from_slice(data);
    // the payload first, then the header that makes it valid
    if !payload.is_empty() && !write(lba + 1, &payload) {
        return false;
    }
    let mut head = [0u8; SECTOR];
    head[..8].copy_from_slice(magic);
    head[8..16].copy_from_slice(&(data.len() as u64).to_le_bytes());
    head[16..20].copy_from_slice(&checksum(data).to_le_bytes());
    write(lba, &head)
}

/// Reads back a blob written by [`save_blob`], if it is there and intact.
pub fn load_blob(lba: u64, max_sectors: u64, magic: &[u8; 8]) -> Option<Vec<u8>> {
    let mut head = [0u8; SECTOR];
    if !read(lba, &mut head) || &head[..8] != magic {
        return None;
    }
    let len = le64(&head, 8) as usize;
    if len == 0 {
        return Some(Vec::new());
    }
    let sectors = (len + SECTOR - 1) / SECTOR;
    if sectors as u64 + 1 > max_sectors {
        return None;
    }
    let mut payload = vec![0u8; sectors * SECTOR];
    if !read(lba + 1, &mut payload) {
        return None;
    }
    payload.truncate(len);
    if le32(&head, 16) as u32 != checksum(&payload) {
        return None;
    }
    Some(payload)
}

/// A simple checksum (FNV-1a) to notice a half-written blob.
fn checksum(data: &[u8]) -> u32 {
    let mut hash: u32 = 0x811C_9DC5;
    for &byte in data {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Description for the `disk` command.
pub fn describe() -> String {
    let disk_sectors = disk::DISK.lock().as_ref().map(|d| d.sectors);
    let part = PARTITION.lock();
    match (disk_sectors, part.as_ref()) {
        (None, _) => String::from("no disk found (nothing will be saved)"),
        (Some(d), None) => alloc::format!("disk of {} MiB, but no data partition on it", d / 2048),
        (Some(d), Some(p)) => alloc::format!(
            "disk of {} MiB, data partition of {} MiB at sector {}",
            d / 2048,
            p.sectors / 2048,
            p.start
        ),
    }
}
