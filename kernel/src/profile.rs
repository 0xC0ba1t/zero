//! A tiny sampling profiler: the timer interrupt notes where the CPU was
//! (the interrupted instruction pointer, in 16 byte buckets). `prof show`
//! lists the busiest places; look the addresses up on the host with
//! `llvm-symbolizer -e <kernel elf> <address>`.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

const SLOTS: usize = 4096;

static ON: AtomicBool = AtomicBool::new(false);
static KEYS: [AtomicU64; SLOTS] = [const { AtomicU64::new(0) }; SLOTS];
static COUNTS: [AtomicU32; SLOTS] = [const { AtomicU32::new(0) }; SLOTS];
static TOTAL: AtomicU32 = AtomicU32::new(0);

pub fn enable(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

pub fn clear() {
    for i in 0..SLOTS {
        KEYS[i].store(0, Ordering::Relaxed);
        COUNTS[i].store(0, Ordering::Relaxed);
    }
    TOTAL.store(0, Ordering::Relaxed);
}

/// Called from the timer interrupt with the interrupted instruction pointer.
#[inline]
pub fn sample(rip: u64) {
    if !ON.load(Ordering::Relaxed) {
        return;
    }
    let key = (rip >> 4) | 1 << 63;
    let mut i = (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 52) as usize % SLOTS;
    for _ in 0..8 {
        let k = KEYS[i].load(Ordering::Relaxed);
        if k == key || k == 0 {
            KEYS[i].store(key, Ordering::Relaxed);
            COUNTS[i].fetch_add(1, Ordering::Relaxed);
            TOTAL.fetch_add(1, Ordering::Relaxed);
            return;
        }
        i = (i + 1) % SLOTS;
    }
}

/// The busiest places: (address, share in permille).
pub fn top(n: usize) -> Vec<(u64, u32)> {
    let total = TOTAL.load(Ordering::Relaxed).max(1);
    let mut all: Vec<(u64, u32)> = (0..SLOTS)
        .filter(|&i| KEYS[i].load(Ordering::Relaxed) != 0)
        .map(|i| ((KEYS[i].load(Ordering::Relaxed) & !(1 << 63)) << 4, COUNTS[i].load(Ordering::Relaxed)))
        .collect();
    all.sort_by(|a, b| b.1.cmp(&a.1));
    all.truncate(n);
    all.into_iter().map(|(a, c)| (a, c * 1000 / total)).collect()
}

pub fn samples() -> u32 {
    TOTAL.load(Ordering::Relaxed)
}
