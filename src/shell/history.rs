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

    /// Steps back to an older line. Returns `None` at the oldest one.
    pub fn older(&mut self) -> Option<&str> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        Some(self.entries[self.cursor].as_str())
    }

    /// Steps forward to a newer line. Stepping past the newest one yields an
    /// empty line; after that there is nothing newer and it returns `None`.
    pub fn newer(&mut self) -> Option<&str> {
        if self.cursor >= self.entries.len() {
            return None;
        }
        self.cursor += 1;
        if self.cursor == self.entries.len() {
            Some("")
        } else {
            Some(self.entries[self.cursor].as_str())
        }
    }

    /// Iterates over the remembered lines, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(String::as_str)
    }
}
