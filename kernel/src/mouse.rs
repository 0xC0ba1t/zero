//! PS/2 mouse.
//!
//! The mouse is wired to the same controller as the keyboard and raises its
//! own interrupt (IRQ 12). It is only switched on while a program wants it
//! (Doom), so the shell is never disturbed by stray mouse bytes.
//!
//! Each movement arrives as a three byte packet: buttons and sign bits, then
//! the horizontal and vertical movement since the last packet. The interrupt
//! handler adds the movement to running totals that [`take`] hands out.

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering};
use x86_64::instructions::port::Port;

static ENABLED: AtomicBool = AtomicBool::new(false);
static DX: AtomicI32 = AtomicI32::new(0);
static DY: AtomicI32 = AtomicI32::new(0);
static BUTTONS: AtomicU8 = AtomicU8::new(0);
static REPORTED: AtomicU8 = AtomicU8::new(0);

static PACKET: [AtomicU8; 3] = [AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0)];
static FILLED: AtomicU8 = AtomicU8::new(0);

fn status() -> u8 {
    unsafe { Port::<u8>::new(0x64).read() }
}

fn wait_to_write() {
    for _ in 0..200_000 {
        if status() & 2 == 0 {
            return;
        }
    }
}

fn read_data() -> Option<u8> {
    for _ in 0..200_000 {
        if status() & 1 != 0 {
            return Some(unsafe { Port::<u8>::new(0x60).read() });
        }
    }
    None
}

fn controller_command(command: u8) {
    wait_to_write();
    unsafe { Port::<u8>::new(0x64).write(command) };
}

fn controller_data(data: u8) {
    wait_to_write();
    unsafe { Port::<u8>::new(0x60).write(data) };
}

/// Sends a command to the mouse itself and waits for its acknowledge.
fn mouse_command(command: u8) -> bool {
    controller_command(0xD4);
    controller_data(command);
    read_data() == Some(0xFA)
}

/// Starts reporting movement. Returns false if there is no mouse.
pub fn enable() -> bool {
    // the controller's replies would otherwise be eaten by the keyboard interrupt handler
    x86_64::instructions::interrupts::without_interrupts(enable_inner)
}

fn enable_inner() -> bool {
    if ENABLED.load(Ordering::SeqCst) {
        return true;
    }
    // throw away anything the controller still holds
    while status() & 1 != 0 {
        unsafe { Port::<u8>::new(0x60).read() };
    }
    controller_command(0xA8); // enable the mouse port
    controller_command(0x20); // read the configuration byte
    let Some(mut config) = read_data() else { return false };
    config |= 0x02; // interrupt for the mouse
    config &= !0x20; // mouse clock on
    controller_command(0x60);
    controller_data(config);

    if !mouse_command(0xF6) || !mouse_command(0xF4) {
        return false;
    }
    DX.store(0, Ordering::SeqCst);
    DY.store(0, Ordering::SeqCst);
    BUTTONS.store(0, Ordering::SeqCst);
    REPORTED.store(0, Ordering::SeqCst);
    FILLED.store(0, Ordering::SeqCst);
    ENABLED.store(true, Ordering::SeqCst);

    // let IRQ 12 through (it arrives via the second PIC, behind IRQ 2)
    unsafe { crate::interrupts::PICS.lock().write_masks(0xF8, 0xEF) };
    true
}

/// Stops reporting movement and masks the interrupt again.
pub fn disable() {
    x86_64::instructions::interrupts::without_interrupts(disable_inner)
}

fn disable_inner() {
    if !ENABLED.swap(false, Ordering::SeqCst) {
        return;
    }
    unsafe { crate::interrupts::PICS.lock().write_masks(0xFC, 0xFF) };
    mouse_command(0xF5);
    controller_command(0xA7); // disable the mouse port
    while status() & 1 != 0 {
        unsafe { Port::<u8>::new(0x60).read() };
    }
}

/// Called from the IRQ 12 handler with the byte the mouse sent.
pub fn handle_byte(byte: u8) {
    let have = FILLED.load(Ordering::Relaxed) as usize;
    // the first byte of a packet always has bit 3 set: use that to stay in step
    if have == 0 && byte & 0x08 == 0 {
        return;
    }
    PACKET[have].store(byte, Ordering::Relaxed);
    if have + 1 < 3 {
        FILLED.store(have as u8 + 1, Ordering::Relaxed);
        return;
    }
    FILLED.store(0, Ordering::Relaxed);

    let flags = PACKET[0].load(Ordering::Relaxed);
    if flags & 0xC0 != 0 {
        return; // the movement overflowed: ignore this packet
    }
    let mut dx = PACKET[1].load(Ordering::Relaxed) as i32;
    let mut dy = PACKET[2].load(Ordering::Relaxed) as i32;
    if flags & 0x10 != 0 {
        dx -= 256;
    }
    if flags & 0x20 != 0 {
        dy -= 256;
    }
    DX.fetch_add(dx, Ordering::Relaxed);
    DY.fetch_add(dy, Ordering::Relaxed);
    BUTTONS.store(flags & 7, Ordering::Relaxed);
}

/// The movement since the last call and the buttons held (bit 0 left,
/// bit 1 right, bit 2 middle), or `None` if nothing changed. Movement up is
/// positive in `dy`.
pub fn take() -> Option<(i32, i32, u8)> {
    let dx = DX.swap(0, Ordering::Relaxed);
    let dy = DY.swap(0, Ordering::Relaxed);
    let buttons = BUTTONS.load(Ordering::Relaxed);
    if dx != 0 || dy != 0 || buttons != REPORTED.load(Ordering::Relaxed) {
        REPORTED.store(buttons, Ordering::Relaxed);
        Some((dx, dy, buttons))
    } else {
        None
    }
}
