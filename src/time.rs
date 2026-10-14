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
