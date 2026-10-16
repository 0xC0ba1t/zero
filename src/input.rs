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
    fn new() -> Self {
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
        interrupts::disable();
        let key = INPUT.lock().pop();
        match key {
            Some(key) => {
                interrupts::enable();
                return key;
            }
            None => interrupts::enable_and_hlt(),
        }
    }
}

/// Returns the oldest queued key without waiting, if there is one.
pub fn try_read_key() -> Option<Key> {
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
