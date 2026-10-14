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

/// Input clock of the PIT, in Hz.
const PIT_FREQUENCY: u32 = 1_193_182;

/// How often the timer interrupt fires once `init_pit` has run.
pub const TICK_HZ: u32 = 100;

/// Programs PIT channel 0 to fire `TICK_HZ` times per second instead of the
/// power-on default of ~18.2 Hz. Scripts need the finer resolution to animate.
pub fn init_pit() {
    use x86_64::instructions::port::Port;

    let divisor = (PIT_FREQUENCY / TICK_HZ) as u16;
    unsafe {
        let mut command: Port<u8> = Port::new(0x43);
        let mut channel0: Port<u8> = Port::new(0x40);
        // channel 0, low byte then high byte, mode 3 (square wave), binary counter
        command.write(0x36);
        channel0.write((divisor & 0xFF) as u8);
        channel0.write((divisor >> 8) as u8);
    }
}

/// Splits a number of seconds into `(hours, minutes, seconds)`.
pub fn split_hms(total_secs: u64) -> (u64, u64, u64) {
    (total_secs / 3600, (total_secs / 60) % 60, total_secs % 60)
}

#[test_case]
fn test_split_hms() {
    assert_eq!(split_hms(0), (0, 0, 0));
    assert_eq!(split_hms(59), (0, 0, 59));
    assert_eq!(split_hms(61), (0, 1, 1));
    assert_eq!(split_hms(3_723), (1, 2, 3));
}

#[test_case]
fn test_tick_increments_counter() {
    let before = ticks();
    tick();
    assert!(ticks() > before);
}
