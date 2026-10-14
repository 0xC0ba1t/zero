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
