//! Time keeping based on the PIT timer interrupt.

use core::sync::atomic::{AtomicU64, Ordering};

/// Number of timer interrupts since boot.
static TICKS: AtomicU64 = AtomicU64::new(0);

/// Called by the timer interrupt handler on every tick.
pub fn tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}

/// Returns the number of timer ticks since boot.
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// The PIT is left at its power-on divisor of 65536, so it fires at
/// 1_193_182 / 65536 = ~18.2065 Hz, i.e. once every ~54.925 ms.
const MICROS_PER_TICK: u64 = 54_925;

/// Milliseconds since boot (approximate).
pub fn uptime_ms() -> u64 {
    ticks() * MICROS_PER_TICK / 1000
}

/// Whole seconds since boot (approximate).
pub fn uptime_secs() -> u64 {
    uptime_ms() / 1000
}
