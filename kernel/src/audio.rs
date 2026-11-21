//! System sound: a driver for the Intel AC97 sound card (what QEMU's `AC97`
//! device emulates) and a small mixer.
//!
//! The card plays from a ring of buffers by DMA: it is given the *physical*
//! addresses of 32 small buffers and walks around the ring by itself. The
//! mixer keeps the next dozen buffers filled. It needs no interrupt of its
//! own: [`pump`] is called from the timer interrupt and by programs that are
//! busy, so the sound keeps flowing while a program is drawing a slow frame.
//!
//! Sound comes from two places: an optional *source* (Doom's mixer, which
//! plays sound effects and music) and a handful of synthesized *effects*
//! (window sounds, game blips). Everything is scaled by the master volume.
//!
//! If the machine has no AC97 card, everything here quietly does nothing.

use crate::{memory, pci};
use alloc::alloc::{alloc_zeroed, Layout};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;
use x86_64::instructions::port::Port;

/// Master volume, 0 to 100.
pub static VOLUME: AtomicU32 = AtomicU32::new(80);
/// Sound effects of the desktop and the games.
pub static EFFECTS_ON: AtomicBool = AtomicBool::new(true);

/// Stereo frames per buffer, and how many buffers are in the ring.
const BUFFER_FRAMES: usize = 512;
const BUFFER_BYTES: usize = BUFFER_FRAMES * 4;
const RING: usize = 32;
/// How many buffers to keep queued ahead of the one being played.
const QUEUED: usize = 12;
const RATE: u32 = 48000;

/// Something that produces audio: writes `frames` stereo frames of signed
/// 16 bit samples (replacing what is in the buffer).
pub type Source = fn(out: *mut i16, frames: usize);

struct Card {
    mixer: u16,
    bus: u16,
    list: *mut u8,    // buffer descriptor list
    buffers: *mut u8, // RING buffers of BUFFER_BYTES
    next: usize,      // the next buffer to fill
}

unsafe impl Send for Card {}

static CARD: Mutex<Option<Card>> = Mutex::new(None);
static SOURCE: Mutex<Option<Source>> = Mutex::new(None);
static PRESENT: AtomicBool = AtomicBool::new(false);

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
        mix(self.buffer(index), BUFFER_FRAMES);
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
        let list = memory::virt_to_phys(self.list as u64).unwrap_or(0) as u32;
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

/// Whether the machine has a sound card we can drive.
pub fn available() -> bool {
    PRESENT.load(Ordering::Relaxed) || pci::find(&[(0x8086, 0x2415)]).is_some()
}

/// Finds and starts the sound card (once). Returns false if there is none.
pub fn start() -> bool {
    if CARD.lock().is_some() {
        return true;
    }
    let Some(dev) = pci::find(&[(0x8086, 0x2415)]) else { return false };
    pci::enable(&dev);
    let mixer = (pci::read32(&dev, 0x10) & 0xFFFC) as u16;
    let native = (pci::read32(&dev, 0x14) & 0xFFFC) as u16;
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
        let Some(address) = memory::virt_to_phys(card.buffer(i) as u64) else { return false };
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

    // take the codec out of reset and turn its own volume up (ours is the master volume)
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
    if let Some(v) = crate::settings::get("volume") {
        VOLUME.store(v.parse().unwrap_or(80).min(100), Ordering::Relaxed);
    }
    if let Some(v) = crate::settings::get("effects") {
        EFFECTS_ON.store(v != "0", Ordering::Relaxed);
    }
    card.start();
    *CARD.lock() = Some(card);
    PRESENT.store(true, Ordering::SeqCst);
    true
}

/// Keeps the sound card supplied; call often (never blocks).
pub fn pump() {
    if let Some(mut guard) = CARD.try_lock() {
        if let Some(card) = guard.as_mut() {
            card.pump();
        }
    }
}

/// Stops the sound card.
pub fn stop() {
    if let Some(card) = CARD.lock().take() {
        card.out8(REG_CR, 0x00);
        card.out8(REG_CR, 0x02);
        // the buffers are leaked on purpose: the card must not be writing
        // into freed memory
    }
    PRESENT.store(false, Ordering::SeqCst);
}

/// Plays audio from `source` (replaces the previous one); `None` stops it.
pub fn set_source(source: Option<Source>) {
    x86_64::instructions::interrupts::without_interrupts(|| *SOURCE.lock() = source);
}

// ---------------------------------------------------------------- mixing

fn mix(out: *mut i16, frames: usize) {
    let buffer = unsafe { core::slice::from_raw_parts_mut(out, frames * 2) };
    let source = SOURCE.try_lock().and_then(|s| *s);
    match source {
        Some(f) => f(out, frames),
        None => buffer.fill(0),
    }
    if let Some(mut st) = STREAM.try_lock() {
        if let Some(st) = st.as_mut() {
            if st.playing {
                let n = buffer.len().min(st.data.len().saturating_sub(st.pos));
                for (d, s) in buffer[..n].iter_mut().zip(&st.data[st.pos..st.pos + n]) {
                    *d = (*d as i32 + *s as i32).clamp(-32768, 32767) as i16;
                }
                st.pos += n;
                if st.pos >= st.data.len() {
                    st.playing = false;
                }
            }
        }
    }
    if let Some(mut voices) = VOICES.try_lock() {
        for voice in voices.iter_mut().filter(|v| v.active) {
            voice.render(buffer);
        }
    }
    let volume = VOLUME.load(Ordering::Relaxed).min(100) as i32;
    // a gentle curve: low settings are really quiet
    let gain = volume * volume / 100;
    for s in buffer.iter_mut() {
        let v = (*s as i32 * gain) / 100;
        *s = v.clamp(-32768, 32767) as i16;
    }
}

// ---------------------------------------------------------------- streams

/// A recording being played (48 kHz stereo samples).
pub struct Stream {
    pub data: alloc::sync::Arc<alloc::vec::Vec<i16>>,
    pub pos: usize,
    pub playing: bool,
}

pub static STREAM: Mutex<Option<Stream>> = Mutex::new(None);

pub fn play_stream(data: alloc::sync::Arc<alloc::vec::Vec<i16>>) {
    start();
    x86_64::instructions::interrupts::without_interrupts(|| *STREAM.lock() = Some(Stream { data, pos: 0, playing: true }));
}

/// (position, length) in samples, and whether it plays.
pub fn stream_state() -> Option<(usize, usize, bool)> {
    x86_64::instructions::interrupts::without_interrupts(|| STREAM.lock().as_ref().map(|s| (s.pos, s.data.len(), s.playing)))
}

pub fn stream_set(pos: Option<usize>, playing: Option<bool>) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        if let Some(s) = STREAM.lock().as_mut() {
            if let Some(p) = pos {
                s.pos = p.min(s.data.len()) & !1;
            }
            if let Some(pl) = playing {
                s.playing = pl;
                if pl && s.pos >= s.data.len() {
                    s.pos = 0;
                }
            }
        }
    });
}

pub fn stream_stop() {
    x86_64::instructions::interrupts::without_interrupts(|| *STREAM.lock() = None);
}

// ---------------------------------------------------------------- effects

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Click,
    Open,
    Close,
    Score,
    Hurt,
    Win,
    Lose,
    Danger,
}

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    /// 0 square, 1 triangle, 2 noise
    wave: u8,
    phase: u32,
    step: u32,
    /// Added to `step` each sample (a pitch slide).
    slide: i32,
    delay: u32,
    left: u32,
    total: u32,
    volume: i32,
    noise: u32,
}

const IDLE: Voice = Voice { active: false, wave: 0, phase: 0, step: 0, slide: 0, delay: 0, left: 0, total: 1, volume: 0, noise: 1 };

static VOICES: Mutex<[Voice; 10]> = Mutex::new([IDLE; 10]);

fn step_for(freq: u32) -> u32 {
    // 2^32 / 48000 = 89478.48
    (freq as u64 * 89478) as u32
}

impl Voice {
    fn render(&mut self, out: &mut [i16]) {
        for frame in out.chunks_exact_mut(2) {
            if self.delay > 0 {
                self.delay -= 1;
                continue;
            }
            if self.left == 0 {
                self.active = false;
                return;
            }
            self.left -= 1;
            self.phase = self.phase.wrapping_add(self.step);
            self.step = (self.step as i64 + self.slide as i64).clamp(0, 0x7FFF_FFFF) as u32;
            // the sound fades out over its length
            let envelope = (self.left as i64 * 256 / self.total as i64) as i32;
            let sample = match self.wave {
                0 => {
                    let w = if self.phase & 0x8000_0000 != 0 { 1 } else { -1 };
                    w * self.volume * envelope / 256
                }
                1 => {
                    // a triangle wave
                    let p = (self.phase >> 16) as i32;
                    let tri = if p < 32768 { p } else { 65535 - p };
                    (tri - 16384) * self.volume / 16384 * envelope / 256
                }
                _ => {
                    // xorshift noise, changing at the pitch rate
                    if self.phase < self.step {
                        self.noise ^= self.noise << 13;
                        self.noise ^= self.noise >> 17;
                        self.noise ^= self.noise << 5;
                    }
                    let w = if self.noise & 1 == 0 { 1 } else { -1 };
                    w * self.volume * envelope / 256
                }
            };
            for ch in frame.iter_mut() {
                *ch = (*ch as i32 + sample).clamp(-32768, 32767) as i16;
            }
        }
    }
}

fn add_voice(wave: u8, freq: u32, end_freq: u32, ms: u32, delay_ms: u32, volume: i32) {
    let total = ms * RATE / 1000;
    let slide = if total > 0 { ((step_for(end_freq) as i64 - step_for(freq) as i64) / total as i64) as i32 } else { 0 };
    let voice = Voice { active: true, wave, phase: 0, step: step_for(freq), slide, delay: delay_ms * RATE / 1000, left: total, total: total.max(1), volume, noise: 0x1234_5678 };
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut voices = VOICES.lock();
        let slot = voices.iter().position(|v| !v.active).unwrap_or(0);
        voices[slot] = voice;
    });
}

/// Plays a short synthesized sound (does nothing without a sound card).
pub fn effect(e: Effect) {
    if !PRESENT.load(Ordering::Relaxed) || !EFFECTS_ON.load(Ordering::Relaxed) {
        return;
    }
    match e {
        Effect::Click => add_voice(1, 1800, 1200, 25, 0, 3500),
        Effect::Open => {
            add_voice(1, 520, 880, 110, 0, 4000);
        }
        Effect::Close => {
            add_voice(1, 880, 440, 110, 0, 4000);
        }
        Effect::Score => {
            add_voice(0, 880, 880, 55, 0, 2600);
            add_voice(0, 1320, 1320, 80, 55, 2600);
        }
        Effect::Hurt => {
            add_voice(0, 220, 70, 220, 0, 3200);
            add_voice(2, 120, 60, 140, 0, 2200);
        }
        Effect::Win => {
            for (i, f) in [523u32, 659, 784, 1047].iter().enumerate() {
                add_voice(1, *f, *f, 120, i as u32 * 110, 4200);
            }
        }
        Effect::Lose => {
            for (i, f) in [392u32, 330, 262, 196].iter().enumerate() {
                add_voice(0, *f, *f * 9 / 10, 170, i as u32 * 160, 2800);
            }
        }
        Effect::Danger => {
            add_voice(0, 700, 700, 70, 0, 2400);
            add_voice(0, 520, 520, 70, 90, 2400);
        }
    }
}
