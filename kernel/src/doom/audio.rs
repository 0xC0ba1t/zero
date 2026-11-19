//! Audio output through an Intel AC97 sound card (what QEMU's `AC97` device
//! emulates).
//!
//! The card plays from a ring of buffers by DMA: it is given the *physical*
//! addresses of 32 small buffers and walks around the ring by itself. This
//! driver keeps the next few buffers filled with audio from the engine's mixer
//! (`DG_MixAudio` in `i_zerosound.c`). It needs no interrupt: [`pump`] is
//! called often enough while the game runs (every frame and while it sleeps)
//! to stay ahead of the card.
//!
//! If the machine has no AC97 card, [`init`] returns false and the game runs
//! silently.

use alloc::alloc::{alloc_zeroed, Layout};
use core::sync::atomic::Ordering;
use spin::Mutex;
use x86_64::instructions::port::Port;

/// Stereo frames per buffer, and how many buffers are in the ring.
const BUFFER_FRAMES: usize = 512;
const BUFFER_BYTES: usize = BUFFER_FRAMES * 4;
const RING: usize = 32;
/// How many buffers to keep queued ahead of the one being played.
const QUEUED: usize = 12;

extern "C" {
    fn DG_MixAudio(out: *mut i16, frames: i32);
}

struct Card {
    mixer: u16,
    bus: u16,
    list: *mut u8,    // buffer descriptor list
    buffers: *mut u8, // RING buffers of BUFFER_BYTES
    next: usize,      // the next buffer to fill
}

unsafe impl Send for Card {}

static CARD: Mutex<Option<Card>> = Mutex::new(None);

// ---- PCI configuration space ------------------------------------------------

fn pci_address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    0x8000_0000 | (bus as u32) << 16 | (device as u32) << 11 | (function as u32) << 8 | (offset as u32 & 0xFC)
}

fn pci_read(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    unsafe {
        Port::<u32>::new(0xCF8).write(pci_address(bus, device, function, offset));
        Port::<u32>::new(0xCFC).read()
    }
}

fn pci_write(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    unsafe {
        Port::<u32>::new(0xCF8).write(pci_address(bus, device, function, offset));
        Port::<u32>::new(0xCFC).write(value);
    }
}

/// Finds the first PCI device with this vendor and device id.
fn find_pci(vendor: u16, device_id: u16) -> Option<(u8, u8, u8)> {
    for bus in 0..8u8 {
        for device in 0..32u8 {
            let id = pci_read(bus, device, 0, 0);
            if id & 0xFFFF == 0xFFFF {
                continue;
            }
            if id & 0xFFFF == vendor as u32 && id >> 16 == device_id as u32 {
                return Some((bus, device, 0));
            }
        }
    }
    None
}

// ---- virtual to physical -----------------------------------------------------

/// Looks an address up in the page tables.
fn physical(virt: u64) -> Option<u64> {
    let offset = crate::memory::PHYS_OFFSET.load(Ordering::Relaxed);
    let (frame, _) = x86_64::registers::control::Cr3::read();
    let mut table = frame.start_address().as_u64();
    for shift in [39u32, 30, 21, 12] {
        let index = (virt >> shift) & 0x1FF;
        let entry = unsafe { ((offset + table + index * 8) as *const u64).read_volatile() };
        if entry & 1 == 0 {
            return None;
        }
        let address = entry & 0x000F_FFFF_FFFF_F000;
        if shift == 12 || entry & 0x80 != 0 {
            let low = (1u64 << shift) - 1;
            return Some((address & !low) | (virt & low));
        }
        table = address;
    }
    None
}

// ---- the card ----------------------------------------------------------------

// native audio bus registers (offsets from the second base address)
const PCM_OUT: u16 = 0x10;
const REG_BDBAR: u16 = PCM_OUT;
const REG_CIV: u16 = PCM_OUT + 0x04;
const REG_LVI: u16 = PCM_OUT + 0x05;
const REG_SR: u16 = PCM_OUT + 0x06;
const REG_CR: u16 = PCM_OUT + 0x0B;
const GLOBAL_CONTROL: u16 = 0x2C;
const GLOBAL_STATUS: u16 = 0x30;

impl Card {
    fn out8(&self, register: u16, value: u8) {
        unsafe { Port::<u8>::new(self.bus + register).write(value) }
    }
    fn out16(&self, register: u16, value: u16) {
        unsafe { Port::<u16>::new(self.bus + register).write(value) }
    }
    fn out32(&self, register: u16, value: u32) {
        unsafe { Port::<u32>::new(self.bus + register).write(value) }
    }
    fn in8(&self, register: u16) -> u8 {
        unsafe { Port::<u8>::new(self.bus + register).read() }
    }
    fn in16(&self, register: u16) -> u16 {
        unsafe { Port::<u16>::new(self.bus + register).read() }
    }

    fn buffer(&self, index: usize) -> *mut i16 {
        unsafe { self.buffers.add(index * BUFFER_BYTES) as *mut i16 }
    }

    fn fill(&self, index: usize) {
        unsafe { DG_MixAudio(self.buffer(index), BUFFER_FRAMES as i32) };
    }

    /// (Re)starts playback from buffer 0 with the ring freshly filled.
    fn start(&mut self) {
        self.out8(REG_CR, 0x02); // reset the channel
        for _ in 0..1000 {
            if self.in8(REG_CR) & 0x02 == 0 {
                break;
            }
        }
        for i in 0..QUEUED {
            self.fill(i);
        }
        self.next = QUEUED;
        let list = physical(self.list as u64).unwrap_or(0) as u32;
        self.out32(REG_BDBAR, list);
        self.out8(REG_LVI, (QUEUED - 1) as u8);
        self.out8(REG_CR, 0x01); // run
    }

    fn pump(&mut self) {
        // halted (the card reached the last valid buffer before we caught up)?
        if self.in16(REG_SR) & 0x01 != 0 {
            self.start();
            return;
        }
        let playing = (self.in8(REG_CIV) & 0x1F) as usize;
        let mut queued = (self.next + RING - playing) % RING;
        while queued < QUEUED {
            self.fill(self.next);
            self.next = (self.next + 1) % RING;
            queued += 1;
        }
        self.out8(REG_LVI, ((self.next + RING - 1) % RING) as u8);
    }
}

/// Finds and starts the sound card. Returns false if there is none.
pub fn init() -> bool {
    let Some((bus, device, function)) = find_pci(0x8086, 0x2415) else { return false };
    // I/O space and bus mastering on (the upper half is the status register: leave it alone)
    let command = pci_read(bus, device, function, 0x04) & 0xFFFF;
    pci_write(bus, device, function, 0x04, command | 0x05);
    let mixer = (pci_read(bus, device, function, 0x10) & 0xFFFC) as u16;
    let native = (pci_read(bus, device, function, 0x14) & 0xFFFC) as u16;
    if mixer == 0 || native == 0 {
        return false;
    }

    let list_layout = Layout::from_size_align(4096, 4096).unwrap();
    let buffer_layout = Layout::from_size_align(RING * BUFFER_BYTES, 4096).unwrap();
    let (list, buffers) = unsafe { (alloc_zeroed(list_layout), alloc_zeroed(buffer_layout)) };
    if list.is_null() || buffers.is_null() {
        return false;
    }
    let mut card = Card { mixer, bus: native, list, buffers, next: 0 };

    // each descriptor: physical address, length in 16 bit samples
    for i in 0..RING {
        let Some(address) = physical(card.buffer(i) as u64) else { return false };
        if address > u32::MAX as u64 {
            return false;
        }
        unsafe {
            let entry = card.list.add(i * 8) as *mut u32;
            entry.write(address as u32);
            (entry.add(1) as *mut u16).write((BUFFER_FRAMES * 2) as u16);
            (entry.add(1) as *mut u16).add(1).write(0);
        }
    }

    // take the codec out of reset and turn the volume up
    card.out32(GLOBAL_CONTROL, 0x02);
    for _ in 0..100_000 {
        if unsafe { Port::<u32>::new(card.bus + GLOBAL_STATUS).read() } & 0x100 != 0 {
            break;
        }
    }
    unsafe {
        Port::<u16>::new(card.mixer).write(0); // reset the mixer
        Port::<u16>::new(card.mixer + 0x02).write(0); // master volume: maximum, not muted
        Port::<u16>::new(card.mixer + 0x18).write(0); // PCM volume
    }
    card.start();
    *CARD.lock() = Some(card);
    true
}

/// Keeps the sound card supplied; call often.
pub fn pump() {
    if let Some(mut guard) = CARD.try_lock() {
        if let Some(card) = guard.as_mut() {
            card.pump();
        }
    }
}

/// Stops the sound card.
pub fn shutdown() {
    if let Some(card) = CARD.lock().take() {
        card.out8(REG_CR, 0x00);
        card.out8(REG_CR, 0x02);
        // the buffers are leaked on purpose: the card must not be writing
        // into freed memory, and Doom only runs once per boot anyway
    }
}
