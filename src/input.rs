//! Keyboard input queue.
//!
//! The keyboard interrupt handler pushes decoded keys into a fixed-size
//! ring buffer and the shell pops them off again. Nothing in here allocates,
//! so it is safe to use from inside an interrupt handler.

/// A key press that the shell cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A decoded character (`\n` is enter, `\u{8}` is backspace).
    Char(char),
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
}
