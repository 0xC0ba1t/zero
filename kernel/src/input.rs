//! Keyboard input queue.
//!
//! The keyboard interrupt handler pushes decoded keys into a fixed-size
//! ring buffer and the shell pops them off again. Nothing in here allocates,
//! so it is safe to use from inside an interrupt handler.

use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;
use x86_64::instructions::interrupts;

/// A key press that the shell cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A decoded character (`\n` is enter, `\u{8}` is backspace).
    Char(char),
    /// The up arrow.
    Up,
    /// The down arrow.
    Down,
    /// The left arrow.
    Left,
    /// The right arrow.
    Right,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Home.
    Home,
    /// End.
    End,
    /// Delete (forward delete).
    Delete,
}

/// A fixed-capacity FIFO queue of keys, backed by a plain array.
pub struct RingBuffer<const N: usize> {
    buf: [Key; N],
    /// index of the oldest key
    head: usize,
    /// number of keys currently stored
    len: usize,
}

impl<const N: usize> RingBuffer<N> {
    /// Creates an empty queue.
    pub const fn new() -> Self {
        RingBuffer {
            buf: [Key::Char('\0'); N],
            head: 0,
            len: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len == N
    }

    /// Appends a key to the back of the queue.
    ///
    /// Hands the key back as the error value if the queue is full.
    pub fn push(&mut self, key: Key) -> Result<(), Key> {
        if self.is_full() {
            return Err(key);
        }
        let tail = (self.head + self.len) % N;
        self.buf[tail] = key;
        self.len += 1;
        Ok(())
    }

    /// Removes and returns the oldest key, if there is one.
    pub fn pop(&mut self) -> Option<Key> {
        if self.is_empty() {
            return None;
        }
        let key = self.buf[self.head];
        self.head = (self.head + 1) % N;
        self.len -= 1;
        Some(key)
    }
}

/// Capacity of the global input queue.
const QUEUE_SIZE: usize = 128;

/// Keys waiting to be consumed by the shell.
static INPUT: Mutex<RingBuffer<QUEUE_SIZE>> = Mutex::new(RingBuffer::new());

/// Queues a key, silently dropping it if the queue is full.
///
/// This is called from the keyboard interrupt handler, which runs with
/// interrupts disabled.
pub fn push_key(key: Key) {
    let _ = INPUT.lock().push(key);
}

/// Blocks until a key is available and returns it.
///
/// While waiting, the CPU is halted until the next interrupt. Interrupts are
/// disabled while the queue is checked, so the keyboard handler can neither
/// deadlock on the queue lock nor slip a key in between the check and the
/// `hlt` (`enable_and_hlt` enables interrupts right before halting).
pub fn read_key() -> Key {
    loop {
        if let Some(key) = try_read_key() {
            crate::sched::note_progress();
            return key;
        }
        crate::sched::yield_idle();
    }
}

/// Returns the oldest queued key without waiting, if there is one.
pub fn try_read_key() -> Option<Key> {
    // a program in a window gets the keys the desktop sends to that window
    if let Some(ctx) = crate::sched::context() {
        if ctx.is_closed() {
            // tell the program to stop (ctrl+c)
            request_interrupt();
            return Some(Key::Char('\u{3}'));
        }
        loop {
            let event = ctx.events.lock().pop_front()?;
            match event {
                crate::gui::Event::Key(key) => return Some(key),
                crate::gui::Event::Resize { .. } => crate::gui::note_resize(),
                _ => {}
            }
        }
    }
    interrupts::without_interrupts(|| INPUT.lock().pop())
}

/// Takes the next key straight from the keyboard queue (for the desktop itself).
pub fn pop_raw_key() -> Option<Key> {
    interrupts::without_interrupts(|| INPUT.lock().pop())
}

/// Set when the user presses ctrl+c. Long running programs poll it to know
/// when to stop.
static INTERRUPT: AtomicBool = AtomicBool::new(false);

/// Records that ctrl+c was pressed.
pub fn request_interrupt() {
    INTERRUPT.store(true, Ordering::SeqCst);
}

/// Returns whether ctrl+c was pressed since the flag was last cleared.
pub fn interrupt_requested() -> bool {
    INTERRUPT.load(Ordering::SeqCst)
}

/// Clears the flag and returns what it was.
pub fn take_interrupt() -> bool {
    INTERRUPT.swap(false, Ordering::SeqCst)
}

#[test_case]
fn test_ring_buffer_fifo_order() {
    let mut queue: RingBuffer<4> = RingBuffer::new();
    queue.push(Key::Char('a')).unwrap();
    queue.push(Key::Char('b')).unwrap();
    assert_eq!(queue.pop(), Some(Key::Char('a')));
    assert_eq!(queue.pop(), Some(Key::Char('b')));
    assert_eq!(queue.pop(), None);
}

#[test_case]
fn test_ring_buffer_rejects_push_when_full() {
    let mut queue: RingBuffer<2> = RingBuffer::new();
    assert!(queue.push(Key::Char('a')).is_ok());
    assert!(queue.push(Key::Char('b')).is_ok());
    assert!(queue.is_full());
    assert_eq!(queue.push(Key::Char('c')), Err(Key::Char('c')));
}

#[test_case]
fn test_ring_buffer_wraps_around() {
    let mut queue: RingBuffer<3> = RingBuffer::new();
    for round in 0..10u8 {
        let c = (b'a' + round) as char;
        queue.push(Key::Char(c)).unwrap();
        assert_eq!(queue.pop(), Some(Key::Char(c)));
    }
    assert!(queue.is_empty());
}

#[test_case]
fn test_global_queue_round_trip() {
    push_key(Key::Char('h'));
    push_key(Key::Char('i'));
    assert_eq!(read_key(), Key::Char('h'));
    assert_eq!(read_key(), Key::Char('i'));
}

#[test_case]
fn test_try_read_key_does_not_block() {
    assert_eq!(try_read_key(), None);
    push_key(Key::Char('z'));
    assert_eq!(try_read_key(), Some(Key::Char('z')));
    assert_eq!(try_read_key(), None);
}

#[test_case]
fn test_interrupt_flag() {
    take_interrupt();
    assert!(!interrupt_requested());
    request_interrupt();
    assert!(interrupt_requested());
    assert!(take_interrupt());
    assert!(!interrupt_requested());
}

// ---- held keys ----------------------------------------------------------

/// One bit per physical key (indexed by `KeyCode`), set while the key is down.
static HELD_LOW: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static HELD_HIGH: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Records that the key with this `KeyCode` index went down or up.
pub fn set_held(index: usize, down: bool) {
    let (word, bit) = if index < 64 { (&HELD_LOW, index) } else { (&HELD_HIGH, index - 64) };
    if bit >= 64 {
        return;
    }
    if down {
        word.fetch_or(1 << bit, Ordering::Relaxed);
    } else {
        word.fetch_and(!(1 << bit), Ordering::Relaxed);
    }
}

/// The `KeyCode` index for a key as scripts name it: a letter, an arrow
/// (256 to 259), space (32), enter (10) or escape (27).
fn key_index(code: i32) -> Option<usize> {
    use pc_keyboard::KeyCode as K;
    const LETTERS: [K; 26] = [
        K::A, K::B, K::C, K::D, K::E, K::F, K::G, K::H, K::I, K::J, K::K, K::L, K::M,
        K::N, K::O, K::P, K::Q, K::R, K::S, K::T, K::U, K::V, K::W, K::X, K::Y, K::Z,
    ];
    let key = match code {
        97..=122 => LETTERS[(code - 97) as usize],
        65..=90 => LETTERS[(code - 65) as usize],
        256 => K::ArrowUp,
        257 => K::ArrowDown,
        258 => K::ArrowLeft,
        259 => K::ArrowRight,
        32 => K::Spacebar,
        10 => K::Return,
        27 => K::Escape,
        _ => return None,
    };
    Some(key as usize)
}

/// Whether the key is being held down right now.
pub fn key_down(code: i32) -> bool {
    // a window that is not in front does not see the keyboard
    if let Some(ctx) = crate::sched::context() {
        if !ctx.focused.load(Ordering::Relaxed) {
            return false;
        }
    }
    match key_index(code) {
        Some(index) if index < 64 => HELD_LOW.load(Ordering::Relaxed) & (1 << index) != 0,
        Some(index) if index < 128 => HELD_HIGH.load(Ordering::Relaxed) & (1 << (index - 64)) != 0,
        _ => false,
    }
}

// ---- key events for Doom ------------------------------------------------
//
// Doom wants separate "pressed" and "released" events for every key, which the
// ordinary shell queue (characters only) cannot give it. While Doom is
// running, the keyboard interrupt also feeds this lock-free ring buffer.

use core::sync::atomic::{AtomicU16, AtomicUsize};

static DOOM_ACTIVE: AtomicBool = AtomicBool::new(false);
const DOOM_QUEUE: usize = 256;
static DOOM_EVENTS: [AtomicU16; DOOM_QUEUE] = [const { AtomicU16::new(0) }; DOOM_QUEUE];
static DOOM_HEAD: AtomicUsize = AtomicUsize::new(0);
static DOOM_TAIL: AtomicUsize = AtomicUsize::new(0);

pub fn set_doom_active(active: bool) {
    DOOM_ACTIVE.store(active, Ordering::SeqCst);
    DOOM_HEAD.store(0, Ordering::SeqCst);
    DOOM_TAIL.store(0, Ordering::SeqCst);
}

/// Doom's key code for a physical key, if it uses it (see `doomkeys.h`).
fn doom_code(code: pc_keyboard::KeyCode) -> Option<u8> {
    use pc_keyboard::KeyCode as K;
    const LETTERS: [K; 26] = [
        K::A, K::B, K::C, K::D, K::E, K::F, K::G, K::H, K::I, K::J, K::K, K::L, K::M,
        K::N, K::O, K::P, K::Q, K::R, K::S, K::T, K::U, K::V, K::W, K::X, K::Y, K::Z,
    ];
    const DIGITS: [K; 10] = [K::Key0, K::Key1, K::Key2, K::Key3, K::Key4, K::Key5, K::Key6, K::Key7, K::Key8, K::Key9];
    const FUNCTION: [K; 12] = [K::F1, K::F2, K::F3, K::F4, K::F5, K::F6, K::F7, K::F8, K::F9, K::F10, K::F11, K::F12];
    if let Some(i) = LETTERS.iter().position(|k| *k == code) {
        return Some(b'a' + i as u8);
    }
    if let Some(i) = DIGITS.iter().position(|k| *k == code) {
        return Some(b'0' + i as u8);
    }
    if let Some(i) = FUNCTION.iter().position(|k| *k == code) {
        // F1..F10 are 0x80 + 0x3b.., F11 and F12 are 0x80 + 0x57 and 0x58
        return Some(if i < 10 { 0x80 + 0x3b + i as u8 } else { 0x80 + 0x57 + (i as u8 - 10) });
    }
    Some(match code {
        K::ArrowUp => 0xad,
        K::ArrowDown => 0xaf,
        K::ArrowLeft => 0xac,
        K::ArrowRight => 0xae,
        K::Escape => 27,
        K::Return | K::NumpadEnter => 13,
        K::Tab => 9,
        K::Spacebar => b' ',
        K::Backspace => 0x7f,
        K::LControl | K::RControl => 0x80 + 0x1d, // fire
        K::LShift | K::RShift => 0x80 + 0x36,     // run
        K::LAlt | K::RAltGr => 0x80 + 0x38,       // strafe
        K::OemComma => b',',
        K::OemPeriod => b'.',
        K::OemMinus => b'-',
        K::OemPlus => b'=',
        K::Oem1 => b';',
        K::Oem2 => b'/',
        K::Oem3 => 39, // '
        K::Oem4 => b'[',
        K::Oem6 => b']',
        K::Oem5 => 92, // backslash
        K::Oem8 => b'`',
        K::PauseBreak => 0xff,
        K::Home => 0x80 + 0x47,
        K::End => 0x80 + 0x4f,
        K::PageUp => 0x80 + 0x49,
        K::PageDown => 0x80 + 0x51,
        K::Insert => 0x80 + 0x52,
        K::Delete => 0x80 + 0x53,
        K::NumpadSubtract => b'-',
        K::NumpadAdd => b'=',
        _ => return None,
    })
}

/// Whether W A S D E act as game controls (off by default: the original game has
/// no such keys, and the letters are needed for its cheat codes).
static DOOM_WASD: AtomicBool = AtomicBool::new(false);

pub fn set_doom_wasd(on: bool) {
    DOOM_WASD.store(on, Ordering::SeqCst);
}

/// Which of W A S D E are currently held as game controls (not as letters).
static DOOM_GAMEKEYS: AtomicU16 = AtomicU16::new(0);

/// W A S D E act as controls while playing (forward, strafe, back, use), but
/// stay letters while a menu is open so names can be typed.
fn doom_game_key(code: pc_keyboard::KeyCode) -> Option<(u16, u8)> {
    use pc_keyboard::KeyCode as K;
    match code {
        K::W => Some((1, 0xad)),
        K::S => Some((2, 0xaf)),
        K::A => Some((4, b',')),
        K::D => Some((8, b'.')),
        K::E => Some((16, b' ')),
        _ => None,
    }
}

/// Called from the keyboard interrupt for every key going down or up.
pub fn doom_key_event(code: pc_keyboard::KeyCode, down: bool) {
    if !DOOM_ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    let mut mapped = doom_code(code);
    if let Some((bit, key)) = doom_game_key(code) {
        if down {
            if DOOM_WASD.load(Ordering::Relaxed) && !crate::doom::menu_open() {
                DOOM_GAMEKEYS.fetch_or(bit, Ordering::Relaxed);
                mapped = Some(key);
            }
        } else if DOOM_GAMEKEYS.fetch_and(!bit, Ordering::Relaxed) & bit != 0 {
            mapped = Some(key); // release what was pressed as a control
        }
    }
    let Some(key) = mapped else { return };
    let tail = DOOM_TAIL.load(Ordering::Relaxed);
    let next = (tail + 1) % DOOM_QUEUE;
    if next == DOOM_HEAD.load(Ordering::Acquire) {
        return; // full: drop the event
    }
    DOOM_EVENTS[tail].store((down as u16) << 8 | key as u16, Ordering::Relaxed);
    DOOM_TAIL.store(next, Ordering::Release);
}

/// The next key event for Doom: `(pressed, key)`.
pub fn doom_pop() -> Option<(bool, u8)> {
    let head = DOOM_HEAD.load(Ordering::Relaxed);
    if head == DOOM_TAIL.load(Ordering::Acquire) {
        return None;
    }
    let event = DOOM_EVENTS[head].load(Ordering::Relaxed);
    DOOM_HEAD.store((head + 1) % DOOM_QUEUE, Ordering::Release);
    Some((event >> 8 != 0, event as u8))
}

/// Whether a physical key (by name) is held down right now (for the desktop's
/// modifier keys).
pub fn held(code: pc_keyboard::KeyCode) -> bool {
    let index = code as usize;
    if index < 64 {
        HELD_LOW.load(Ordering::Relaxed) & (1 << index) != 0
    } else if index < 128 {
        HELD_HIGH.load(Ordering::Relaxed) & (1 << (index - 64)) != 0
    } else {
        false
    }
}
