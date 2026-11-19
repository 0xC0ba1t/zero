//! Saving the file system to the data partition and loading it back.

use crate::fs::{Filesystem, FILESYSTEM};
use crate::partition::{self, PARTITION};

const MAGIC: &[u8; 8] = b"ZTREE001";

fn area() -> Option<(u64, u64)> {
    PARTITION.lock().as_ref().map(|p| (p.tree_lba, p.tree_sectors))
}

/// Loads the saved file system, if there is one.
pub fn load() -> Option<Filesystem> {
    let (lba, sectors) = area()?;
    let bytes = partition::load_blob(lba, sectors, MAGIC);
    crate::serial_println!("[pfs] load: {:?} bytes", bytes.as_ref().map(|b| b.len()));
    let fs = Filesystem::from_bytes(&bytes?);
    crate::serial_println!("[pfs] parsed: {}", fs.is_some());
    fs
}

/// Writes the file system to the partition if it changed since the last save.
/// Returns whether anything was written.
pub fn sync_if_dirty() -> bool {
    let (lba, sectors) = match area() {
        Some(area) => area,
        None => return false,
    };
    let bytes = {
        let mut guard = FILESYSTEM.lock();
        match guard.as_mut() {
            Some(fs) => {
                if !fs.take_dirty() {
                    return false;
                }
                fs.to_bytes()
            }
            None => return false,
        }
    };
    let ok = partition::save_blob(lba, sectors, MAGIC, &bytes);
    crate::serial_println!("[pfs] saved {} bytes: {}", bytes.len(), if ok { "ok" } else { "FAILED" });
    if !ok {
        // try again next time
        if let Some(fs) = FILESYSTEM.lock().as_mut() {
            fs.mark_dirty();
        }
    }
    ok
}
