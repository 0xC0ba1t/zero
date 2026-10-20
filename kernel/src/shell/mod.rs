//! A tiny interactive shell.

mod commands;
mod history;
mod line;
mod parser;

use alloc::string::String;
use crate::input;
use crate::{fb_print, fb_println};
use commands::CommandError;
use history::HISTORY;
use line::{Edit, LineEditor};

const PROMPT: &str = "zero> ";

/// Runs the shell forever: reads keys, edits the current line, and executes it on enter.
pub fn run() -> ! {
    fb_println!(
        "{} shell v{} - type `help` for a list of commands",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let mut editor = LineEditor::new();
    fb_print!("{}", PROMPT);

    loop {
        match editor.handle(input::read_key()) {
            Edit::Inserted(c) => fb_print!("{}", c),
            Edit::Erased => fb_print!("\u{8}"),
            Edit::Submitted => {
                fb_println!();
                let line = editor.take();
                HISTORY.lock().push(&line);
                execute(&line);
                fb_print!("{}", PROMPT);
            }
            Edit::Older => {
                let entry = HISTORY.lock().older().map(String::from);
                if let Some(entry) = entry {
                    replace_line(&mut editor, &entry);
                }
            }
            Edit::Newer => {
                let entry = HISTORY.lock().newer().map(String::from);
                if let Some(entry) = entry {
                    replace_line(&mut editor, &entry);
                }
            }
            Edit::Complete => complete_line(&mut editor),
            Edit::Cancelled => {
                fb_println!("^C");
                let _ = editor.take();
                fb_print!("{}", PROMPT);
            }
            Edit::ScrollUp => crate::graphics::framebuffer::CONSOLE.lock().scroll_up(),
            Edit::ScrollDown => crate::graphics::framebuffer::CONSOLE.lock().scroll_down(),
            Edit::ClearScreen => {
                crate::graphics::framebuffer::CONSOLE.lock().clear();
                fb_print!("{}{}", PROMPT, editor.as_str());
            }
            Edit::KillLine => replace_line(&mut editor, ""),
            Edit::Ignored => {}
        }
    }
}

/// What happened when a line was executed.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The line was blank.
    Empty,
    /// The command ran successfully.
    Success,
    /// There is no command with that name.
    NotFound,
    /// The command was called with the wrong arguments.
    Usage,
    /// The command ran but failed.
    Failed,
    /// The line could not be parsed.
    ParseError,
}

/// Parses and runs a single command line.
pub fn execute(line: &str) -> Outcome {
    let args = match parser::parse(line) {
        Ok(args) => args,
        Err(error) => {
            fb_println!("parse error: {}", error);
            return Outcome::ParseError;
        }
    };

    let (name, rest) = match args.split_first() {
        Some(split) => split,
        None => return Outcome::Empty,
    };

    let command = match commands::find(name) {
        Some(command) => command,
        None => {
            fb_println!("{}: command not found", name);
            return Outcome::NotFound;
        }
    };

    match (command.handler)(rest) {
        Ok(()) => Outcome::Success,
        Err(CommandError::Usage) => {
            fb_println!("usage: {}", command.usage);
            Outcome::Usage
        }
        Err(CommandError::Failed(reason)) => {
            fb_println!("{}: {}", command.name, reason);
            Outcome::Failed
        }
        Err(CommandError::Reported) => Outcome::Failed,
    }
}

/// Swaps the line being edited for `new`, both on screen and in the editor.
fn replace_line(editor: &mut LineEditor, new: &str) {
    for _ in 0..editor.as_str().len() {
        fb_print!("\u{8}");
    }
    editor.set(new);
    fb_print!("{}", editor.as_str());
}

/// Completes the command name being typed, if it is unambiguous.
fn complete_line(editor: &mut LineEditor) {
    let name = match commands::complete(editor.as_str()) {
        Some(name) => name,
        None => return,
    };
    let typed = editor.as_str().len();
    for c in name[typed..].chars().chain(core::iter::once(' ')) {
        if editor.insert(c) {
            fb_print!("{}", c);
        }
    }
}

#[test_case]
fn test_execute_blank_line() {
    assert_eq!(execute("   "), Outcome::Empty);
}

#[test_case]
fn test_execute_unknown_command() {
    assert_eq!(execute("frobnicate"), Outcome::NotFound);
}

#[test_case]
fn test_execute_echo() {
    assert_eq!(execute("echo hello world"), Outcome::Success);
}

#[test_case]
fn test_execute_reports_parse_errors() {
    assert_eq!(execute("echo \"oops"), Outcome::ParseError);
}

#[test_case]
fn test_execute_bad_usage() {
    assert_eq!(execute("help a b"), Outcome::Usage);
}

#[test_case]
fn test_execute_help_for_unknown_command() {
    assert_eq!(execute("help frobnicate"), Outcome::Failed);
}

#[test_case]
fn test_execute_color() {
    assert_eq!(execute("color chartreuse"), Outcome::Failed);
    assert_eq!(execute("color black"), Outcome::Failed);
    assert_eq!(execute("color a b"), Outcome::Usage);
}

#[test_case]
fn test_execute_eval() {
    assert_eq!(execute("eval 2 + 3 * 4"), Outcome::Success);
    assert_eq!(execute("eval 2 +"), Outcome::Failed);
    assert_eq!(execute("eval"), Outcome::Usage);
}

#[test_case]
fn test_execute_exec() {
    assert_eq!(
        execute("exec \"let i = 0; while i < 3; i = i + 1; end; print i\""),
        Outcome::Success
    );
    assert_eq!(execute("exec \"print 1 / 0\""), Outcome::Failed);
}

#[test_case]
fn test_execute_run_and_programs() {
    assert_eq!(execute("programs"), Outcome::Success);
    assert_eq!(execute("run hello"), Outcome::Success);
    assert_eq!(execute("run no_such_program"), Outcome::Failed);
    assert_eq!(execute("run"), Outcome::Usage);
}

#[test_case]
fn test_execute_src() {
    assert_eq!(execute("src hello"), Outcome::Success);
    assert_eq!(execute("src no_such_program"), Outcome::Failed);
    assert_eq!(execute("src"), Outcome::Usage);
}
