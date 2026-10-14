//! A tiny interactive shell.

mod commands;
mod history;
mod line;
mod parser;

use crate::input;
use crate::{print, println};
use commands::CommandError;
use line::{Edit, LineEditor};

const PROMPT: &str = "zero> ";

/// Runs the shell forever: reads keys, edits the current line, and executes it on enter.
pub fn run() -> ! {
    println!(
        "{} shell v{} - type `help` for a list of commands",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let mut editor = LineEditor::new();
    print!("{}", PROMPT);

    loop {
        match editor.handle(input::read_key()) {
            Edit::Inserted(c) => print!("{}", c),
            Edit::Erased => print!("\u{8}"),
            Edit::Submitted => {
                println!();
                let line = editor.take();
                execute(&line);
                print!("{}", PROMPT);
            }
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
            println!("parse error: {}", error);
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
            println!("{}: command not found", name);
            return Outcome::NotFound;
        }
    };

    match (command.handler)(rest) {
        Ok(()) => Outcome::Success,
        Err(CommandError::Usage) => {
            println!("usage: {}", command.usage);
            Outcome::Usage
        }
        Err(CommandError::Failed(reason)) => {
            println!("{}: {}", command.name, reason);
            Outcome::Failed
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
