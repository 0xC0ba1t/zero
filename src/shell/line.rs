//! Line editing for the shell prompt.

use alloc::string::String;

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
}
