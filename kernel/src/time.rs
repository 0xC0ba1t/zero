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

/// CPU timestamp counter cycles per millisecond (0 until calibrated).
static TSC_PER_MS: AtomicU64 = AtomicU64::new(0);
/// Timestamp counter value at calibration (time zero).
static TSC_BASE: AtomicU64 = AtomicU64::new(0);
/// 2^40 / cycles per millisecond: turns a division into a multiply and a shift.
static TSC_MUL: AtomicU64 = AtomicU64::new(0);

/// Milliseconds since boot.
///
/// Taken from the timestamp counter once it is calibrated: counting timer
/// interrupts is not reliable (an emulator or a busy machine can lose some),
/// the counter never lags.
pub fn uptime_ms() -> u64 {
    let per_ms = TSC_PER_MS.load(Ordering::Relaxed);
    if per_ms == 0 {
        return ticks() * 1000 / TICK_HZ as u64;
    }
    let now = unsafe { core::arch::x86_64::_rdtsc() };
    let delta = now.wrapping_sub(TSC_BASE.load(Ordering::Relaxed));
    (((delta as u128) * (TSC_MUL.load(Ordering::Relaxed) as u128)) >> 40) as u64
}

/// Measures how fast the timestamp counter runs by timing a 10 ms one-shot of
/// PIT channel 2 (which, unlike channel 0, needs no interrupt to read).
pub fn calibrate_tsc() {
    use x86_64::instructions::port::Port;
    unsafe {
        let mut gate: Port<u8> = Port::new(0x61);
        let mut command: Port<u8> = Port::new(0x43);
        let mut channel2: Port<u8> = Port::new(0x42);
        let old = gate.read();
        // gate low, speaker off; then program channel 2, mode 0, lobyte/hibyte
        gate.write(old & 0xFC);
        command.write(0xB0);
        let count: u16 = (PIT_FREQUENCY / 100) as u16;
        channel2.write((count & 0xFF) as u8);
        channel2.write((count >> 8) as u8);
        // raising the gate starts the count
        gate.write((old & 0xFC) | 1);
        let start = core::arch::x86_64::_rdtsc();
        let mut spins = 0u64;
        while gate.read() & 0x20 == 0 {
            spins += 1;
            if spins > 500_000_000 {
                gate.write(old);
                return;
            }
        }
        let end = core::arch::x86_64::_rdtsc();
        gate.write(old);
        let per_ms = (end - start) / 10;
        if per_ms > 0 {
            TSC_BASE.store(end, Ordering::Relaxed);
            TSC_MUL.store((1u64 << 40) / per_ms, Ordering::Relaxed);
            TSC_PER_MS.store(per_ms, Ordering::Relaxed);
        }
    }
}

/// Whole seconds since boot (approximate).
pub fn uptime_secs() -> u64 {
    uptime_ms() / 1000
}

/// Input clock of the PIT, in Hz.
const PIT_FREQUENCY: u32 = 1_193_182;

/// How often the timer interrupt fires once `init_pit` has run.
pub const TICK_HZ: u32 = 1000;

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
