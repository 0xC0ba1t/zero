//! Remembers previously entered lines.

use alloc::string::String;
use alloc::vec::Vec;

/// How many lines are remembered.
const MAX_ENTRIES: usize = 16;

/// A list of previously entered lines plus a cursor for browsing through them.
pub struct History {
    entries: Vec<String>,
    /// Index of the entry being shown; `entries.len()` means "not browsing".
    cursor: usize,
}

impl History {
    pub const fn new() -> Self {
        History {
            entries: Vec::new(),
            cursor: 0,
        }
    }

    /// Remembers a line. Blank lines and repeats of the previous line are skipped.
    /// Also resets the browsing cursor.
    pub fn push(&mut self, line: &str) {
        let repeated = self.entries.last().map(String::as_str) == Some(line);
        if !line.is_empty() && !repeated {
            if self.entries.len() == MAX_ENTRIES {
                self.entries.remove(0);
            }
            self.entries.push(String::from(line));
        }
        self.cursor = self.entries.len();
    }
}
