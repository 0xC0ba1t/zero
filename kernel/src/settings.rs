//! Small persistent key/value settings (your name, whether the welcome tour
//! has run, ...), kept in the data partition. Without a partition they simply
//! last until the next boot.

use crate::partition::{self, PARTITION};
use alloc::collections::BTreeMap;
use alloc::string::String;
use spin::Mutex;

const MAGIC: &[u8; 8] = b"ZSETTING";
/// The settings area is 8 sectors (4 KiB).
const SECTORS: u64 = 8;

static SETTINGS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

fn lba() -> Option<u64> {
    PARTITION.lock().as_ref().map(|p| p.settings_lba)
}

/// Reads the saved settings (call after [`partition::init`]).
pub fn load() {
    let Some(lba) = lba() else { return };
    let Some(bytes) = partition::load_blob(lba, SECTORS, MAGIC) else { return };
    let text = String::from_utf8_lossy(&bytes);
    let mut map = SETTINGS.lock();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            map.insert(String::from(key), String::from(value));
        }
    }
}

pub fn get(key: &str) -> Option<String> {
    SETTINGS.lock().get(key).cloned()
}

pub fn get_or(key: &str, default: &str) -> String {
    get(key).unwrap_or_else(|| String::from(default))
}

/// Stores a setting and saves them all.
pub fn set(key: &str, value: &str) {
    let text = {
        let mut map = SETTINGS.lock();
        map.insert(String::from(key), String::from(value));
        let mut text = String::new();
        for (k, v) in map.iter() {
            text.push_str(k);
            text.push('=');
            text.push_str(v);
            text.push('\n');
        }
        text
    };
    if let Some(lba) = lba() {
        partition::save_blob(lba, SECTORS, MAGIC, text.as_bytes());
    }
}

/// Forgets every setting (the `onboard reset` command).
pub fn clear() {
    SETTINGS.lock().clear();
    if let Some(lba) = lba() {
        partition::save_blob(lba, SECTORS, MAGIC, b"");
    }
}
