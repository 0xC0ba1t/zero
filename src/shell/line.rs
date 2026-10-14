//! Line editing for the shell prompt.

use crate::input::Key;
use alloc::string::String;

/// What happened as a result of a key press.
#[derive(Debug, PartialEq, Eq)]
pub enum Edit {
    /// The key did nothing.
    Ignored,
    /// A character was added to the end of the line.
    Inserted(char),
    /// The last character was removed.
    Erased,
    /// The user pressed enter.
    Submitted,
}

/// Collects the characters of the line being typed.
pub struct LineEditor {
    buf: String,
}

impl LineEditor {
    /// Creates an editor holding an empty line.
    pub const fn new() -> Self {
        LineEditor { buf: String::new() }
    }

    /// The text typed so far.
    pub fn as_str(&self) -> &str {
        &self.buf
    }

    /// Adds a character to the end of the line.
    pub fn insert(&mut self, c: char) {
        self.buf.push(c);
    }

    /// Removes the last character. Returns `false` if the line was already empty.
    pub fn erase(&mut self) -> bool {
        self.buf.pop().is_some()
    }

    /// Takes the finished line out of the editor, leaving it empty.
    pub fn take(&mut self) -> String {
        core::mem::take(&mut self.buf)
    }

    /// Applies a key press to the line and reports what changed.
    pub fn handle(&mut self, key: Key) -> Edit {
        match key {
            Key::Char('\n') | Key::Char('\r') => Edit::Submitted,
            Key::Char('\u{8}') | Key::Char('\u{7f}') => {
                if self.erase() {
                    Edit::Erased
                } else {
                    Edit::Ignored
                }
            }
            Key::Char(c) if c.is_ascii() && !c.is_ascii_control() => {
                self.insert(c);
                Edit::Inserted(c)
            }
            _ => Edit::Ignored,
        }
    }
}
