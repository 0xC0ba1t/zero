//! Line editing for the shell prompt.
//!
//! The editor only deals with ASCII, so a character index is also a byte
//! index. It knows nothing about the screen: the shell redraws the line
//! whenever [`Edit::Changed`] comes back.

use crate::input::Key;
use alloc::string::String;

/// Longest line the editor accepts unless the shell says otherwise.
pub const DEFAULT_MAX_LEN: usize = 200;

/// What happened as a result of a key press.
#[derive(Debug, PartialEq, Eq)]
pub enum Edit {
    /// The key did nothing.
    Ignored,
    /// The text or the cursor changed: redraw the line.
    Changed,
    /// The user pressed enter.
    Submitted,
    /// The user wants the previous entry from the history.
    Older,
    /// The user wants the next entry from the history.
    Newer,
    /// The user pressed tab.
    Complete,
    /// The user pressed ctrl+c and wants to abandon the line.
    Cancelled,
    /// The user pressed ctrl+l and wants the screen cleared.
    ClearScreen,
    /// The user wants to look further back in the output.
    ScrollUp,
    /// The user wants to look at newer output.
    ScrollDown,
}

/// Collects the characters of the line being typed.
pub struct LineEditor {
    buf: String,
    cursor: usize,
    max_len: usize,
}

impl LineEditor {
    /// Creates an editor holding an empty line.
    pub const fn new() -> Self {
        LineEditor { buf: String::new(), cursor: 0, max_len: DEFAULT_MAX_LEN }
    }

    /// Limits the length of the line (the shell uses the room left on screen).
    pub fn set_max_len(&mut self, max_len: usize) {
        self.max_len = max_len.max(1);
    }

    /// The text typed so far.
    pub fn as_str(&self) -> &str {
        &self.buf
    }

    /// Where the cursor is, counted in characters from the start of the line.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Inserts a character at the cursor. Returns `false` if the line is full.
    pub fn insert(&mut self, c: char) -> bool {
        if self.buf.len() >= self.max_len {
            return false;
        }
        self.buf.insert(self.cursor, c);
        self.cursor += 1;
        true
    }

    /// Removes the character before the cursor.
    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        self.buf.remove(self.cursor);
        true
    }

    /// Removes the character under the cursor.
    pub fn delete(&mut self) -> bool {
        if self.cursor >= self.buf.len() {
            return false;
        }
        self.buf.remove(self.cursor);
        true
    }

    fn left(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        true
    }

    fn right(&mut self) -> bool {
        if self.cursor >= self.buf.len() {
            return false;
        }
        self.cursor += 1;
        true
    }

    fn home(&mut self) -> bool {
        let moved = self.cursor != 0;
        self.cursor = 0;
        moved
    }

    fn end(&mut self) -> bool {
        let moved = self.cursor != self.buf.len();
        self.cursor = self.buf.len();
        moved
    }

    /// Deletes from the cursor to the end of the line.
    fn kill_to_end(&mut self) -> bool {
        let changed = self.cursor < self.buf.len();
        self.buf.truncate(self.cursor);
        changed
    }

    /// Deletes from the start of the line to the cursor.
    fn kill_to_start(&mut self) -> bool {
        let changed = self.cursor > 0;
        self.buf.drain(..self.cursor);
        self.cursor = 0;
        changed
    }

    /// Deletes the word before the cursor.
    fn kill_word(&mut self) -> bool {
        let bytes = self.buf.as_bytes();
        let mut start = self.cursor;
        while start > 0 && bytes[start - 1] == b' ' {
            start -= 1;
        }
        while start > 0 && bytes[start - 1] != b' ' {
            start -= 1;
        }
        let changed = start < self.cursor;
        self.buf.drain(start..self.cursor);
        self.cursor = start;
        changed
    }

    /// Replaces the whole line, cutting it off at the maximum length, and
    /// puts the cursor at the end.
    pub fn set(&mut self, text: &str) {
        self.buf.clear();
        for c in text.chars().take(self.max_len) {
            self.buf.push(c);
        }
        self.cursor = self.buf.len();
    }

    /// Replaces the text in `start..end` and puts the cursor after the new
    /// text. Returns `false` (changing nothing) if the result would be too long.
    pub fn replace_range(&mut self, start: usize, end: usize, with: &str) -> bool {
        let end = end.min(self.buf.len());
        let start = start.min(end);
        if self.buf.len() - (end - start) + with.len() > self.max_len {
            return false;
        }
        self.buf.replace_range(start..end, with);
        self.cursor = start + with.len();
        true
    }

    /// Takes the finished line out of the editor, leaving it empty.
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        core::mem::take(&mut self.buf)
    }

    /// Applies a key press to the line and reports what changed.
    pub fn handle(&mut self, key: Key) -> Edit {
        let changed = |yes: bool| if yes { Edit::Changed } else { Edit::Ignored };
        match key {
            Key::Char('\n') | Key::Char('\r') => Edit::Submitted,
            Key::Char('\u{8}') => changed(self.backspace()),
            Key::Char('\u{7f}') | Key::Delete => changed(self.delete()),
            Key::Char('\t') => Edit::Complete,
            Key::Char('\u{3}') => Edit::Cancelled,
            Key::Char('\u{c}') => Edit::ClearScreen,
            Key::Char('\u{1}') | Key::Home => changed(self.home()), // ctrl+a
            Key::Char('\u{5}') | Key::End => changed(self.end()),   // ctrl+e
            Key::Char('\u{2}') | Key::Left => changed(self.left()), // ctrl+b
            Key::Char('\u{6}') | Key::Right => changed(self.right()), // ctrl+f
            Key::Char('\u{b}') => changed(self.kill_to_end()),      // ctrl+k
            Key::Char('\u{15}') => changed(self.kill_to_start()),   // ctrl+u
            Key::Char('\u{17}') => changed(self.kill_word()),       // ctrl+w
            Key::Char('\u{4}') => changed(self.delete()),           // ctrl+d
            Key::Char(c) if c.is_ascii() && !c.is_ascii_control() => changed(self.insert(c)),
            Key::Up => Edit::Older,
            Key::Down => Edit::Newer,
            Key::PageUp => Edit::ScrollUp,
            Key::PageDown => Edit::ScrollDown,
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
    assert_eq!(editor.cursor(), 4);
}

#[test_case]
fn test_line_editor_backspace_and_delete() {
    let mut editor = LineEditor::new();
    type_str(&mut editor, "hex\u{8}lp");
    assert_eq!(editor.as_str(), "help");
    editor.handle(Key::Home);
    editor.handle(Key::Delete);
    assert_eq!(editor.as_str(), "elp");

    let mut empty = LineEditor::new();
    assert_eq!(empty.handle(Key::Char('\u{8}')), Edit::Ignored);
}

#[test_case]
fn test_line_editor_inserts_at_the_cursor() {
    let mut editor = LineEditor::new();
    type_str(&mut editor, "hlp");
    editor.handle(Key::Left);
    editor.handle(Key::Left);
    editor.handle(Key::Char('e'));
    assert_eq!(editor.as_str(), "help");
    assert_eq!(editor.cursor(), 2);
    editor.handle(Key::End);
    assert_eq!(editor.cursor(), 4);
}

#[test_case]
fn test_line_editor_kill_commands() {
    let mut editor = LineEditor::new();
    type_str(&mut editor, "echo hello world");
    editor.handle(Key::Char('\u{17}')); // ctrl+w
    assert_eq!(editor.as_str(), "echo hello ");
    editor.handle(Key::Left);
    editor.handle(Key::Left);
    editor.handle(Key::Char('\u{b}')); // ctrl+k
    assert_eq!(editor.as_str(), "echo hell");
    editor.handle(Key::Char('\u{15}')); // ctrl+u
    assert_eq!(editor.as_str(), "");
}

#[test_case]
fn test_line_editor_submit_takes_the_line() {
    let mut editor = LineEditor::new();
    type_str(&mut editor, "ab");
    assert_eq!(editor.handle(Key::Char('\n')), Edit::Submitted);
    assert_eq!(editor.take(), "ab");
    assert_eq!(editor.as_str(), "");
    assert_eq!(editor.cursor(), 0);
}

#[test_case]
fn test_line_editor_enforces_max_len() {
    let mut editor = LineEditor::new();
    editor.set_max_len(10);
    for _ in 0..20 {
        editor.handle(Key::Char('x'));
    }
    assert_eq!(editor.as_str().len(), 10);
}

#[test_case]
fn test_line_editor_ignores_unprintable_keys() {
    let mut editor = LineEditor::new();
    assert_eq!(editor.handle(Key::Char('\u{1b}')), Edit::Ignored);
    assert_eq!(editor.handle(Key::Char('\u{e9}')), Edit::Ignored);
    assert_eq!(editor.as_str(), "");
}

#[test_case]
fn test_line_editor_maps_navigation_keys() {
    let mut editor = LineEditor::new();
    assert_eq!(editor.handle(Key::Up), Edit::Older);
    assert_eq!(editor.handle(Key::Down), Edit::Newer);
    assert_eq!(editor.handle(Key::Char('\t')), Edit::Complete);
    assert_eq!(editor.handle(Key::Char('\u{3}')), Edit::Cancelled);
    assert_eq!(editor.handle(Key::PageUp), Edit::ScrollUp);
}

#[test_case]
fn test_line_editor_set_replaces_the_line() {
    let mut editor = LineEditor::new();
    type_str(&mut editor, "old");
    editor.set("new line");
    assert_eq!(editor.as_str(), "new line");
    assert_eq!(editor.cursor(), 8);
}
