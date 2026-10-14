//! Line editing for the shell prompt.

use crate::input::Key;
use alloc::string::String;

/// Longest line the editor accepts. Together with the prompt this stays below
/// the width of the screen, so the line being edited never wraps onto a
/// second row (backspace can't move back up to the previous row).
pub const MAX_LEN: usize = 72;

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

    /// Adds a character to the end of the line. Returns `false` if the line is full.
    pub fn insert(&mut self, c: char) -> bool {
        if self.buf.len() >= MAX_LEN {
            return false;
        }
        self.buf.push(c);
        true
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
                if self.insert(c) {
                    Edit::Inserted(c)
                } else {
                    Edit::Ignored
                }
            }
            _ => Edit::Ignored,
        }
    }
}

#[cfg(test)]
fn type_str(editor: &mut LineEditor, text: &str) {
    for c in text.chars() {
        editor.handle(Key::Char(c));
    }
}

#[test_case]
fn test_line_editor_collects_characters() {
    let mut editor = LineEditor::new();
    type_str(&mut editor, "help");
    assert_eq!(editor.as_str(), "help");
}

#[test_case]
fn test_line_editor_backspace() {
    let mut editor = LineEditor::new();
    type_str(&mut editor, "hex\u{8}lp");
    assert_eq!(editor.as_str(), "help");

    let mut empty = LineEditor::new();
    assert_eq!(empty.handle(Key::Char('\u{8}')), Edit::Ignored);
}

#[test_case]
fn test_line_editor_submit_takes_the_line() {
    let mut editor = LineEditor::new();
    type_str(&mut editor, "ab");
    assert_eq!(editor.handle(Key::Char('\n')), Edit::Submitted);
    assert_eq!(editor.take(), "ab");
    assert_eq!(editor.as_str(), "");
}

#[test_case]
fn test_line_editor_enforces_max_len() {
    let mut editor = LineEditor::new();
    for _ in 0..MAX_LEN + 10 {
        editor.handle(Key::Char('x'));
    }
    assert_eq!(editor.as_str().len(), MAX_LEN);
}

#[test_case]
fn test_line_editor_ignores_unprintable_keys() {
    let mut editor = LineEditor::new();
    assert_eq!(editor.handle(Key::Char('\u{1b}')), Edit::Ignored);
    assert_eq!(editor.handle(Key::Char('\u{e9}')), Edit::Ignored);
    assert_eq!(editor.as_str(), "");
}
