//! The interactive shell.

mod commands;
mod complete;
mod history;
mod line;
mod parser;

use crate::fs::FILESYSTEM;
use crate::graphics::console::CONSOLE;
use crate::graphics::ui;
use crate::input;
use crate::{fb_print, fb_println};
use alloc::string::String;
use commands::CommandError;
use history::HISTORY;
use line::{Edit, LineEditor};

const GREEN: &str = "\x1b[92m";
const BLUE: &str = "\x1b[94m";
const RED: &str = "\x1b[91m";
const GRAY: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

/// Where the line being edited starts on screen, and how much of it is drawn.
struct Prompt {
    col: usize,
    row: usize,
    /// Characters of input currently on screen (so a shorter line can wipe the rest).
    drawn: usize,
}

/// Prints the prompt and tells the editor how much room is left on the line.
fn show_prompt(editor: &mut LineEditor) -> Prompt {
    let cwd = FILESYSTEM
        .lock()
        .as_ref()
        .map(|fs| fs.get_cwd())
        .unwrap_or_else(|| String::from("/"));
    fb_print!("{}{}{} {}>{} ", BLUE, cwd, RESET, GREEN, RESET);
    let (cols, _) = CONSOLE.lock().size();
    let (col, row) = CONSOLE.lock().pos();
    // keep the whole line on one screen row, so editing never has to scroll
    editor.set_max_len(cols.saturating_sub(col + 1).max(8));
    Prompt { col, row, drawn: 0 }
}

/// Redraws the input line and puts the cursor where the editor has it.
fn redraw(editor: &LineEditor, prompt: &mut Prompt) {
    let text = editor.as_str();
    let wipe = prompt.drawn.saturating_sub(text.len());
    CONSOLE
        .lock()
        .redraw_input(prompt.col, prompt.row, text, wipe, editor.cursor());
    prompt.drawn = text.len();
}

/// Waits for the next key. While idle the status bar keeps its clock moving.
fn next_key() -> input::Key {
    loop {
        if let Some(key) = input::try_read_key() {
            return key;
        }
        ui::tick_status();
        x86_64::instructions::hlt();
    }
}

/// Runs the shell forever: reads keys, edits the current line, and executes it on enter.
pub fn run() -> ! {
    fb_println!(
        "{}{}{} shell v{} - type {}help{} for a list of commands",
        GREEN,
        env!("CARGO_PKG_NAME"),
        RESET,
        env!("CARGO_PKG_VERSION"),
        "\x1b[96m",
        RESET
    );

    let mut editor = LineEditor::new();
    let mut prompt = show_prompt(&mut editor);

    loop {
        match editor.handle(next_key()) {
            Edit::Changed => redraw(&editor, &mut prompt),
            Edit::Submitted => {
                fb_println!();
                let line = editor.take();
                HISTORY.lock().push(&line);
                execute(&line);
                ui::refresh_status();
                prompt = show_prompt(&mut editor);
            }
            Edit::Older => {
                let entry = HISTORY.lock().older().map(String::from);
                if let Some(entry) = entry {
                    editor.set(&entry);
                    redraw(&editor, &mut prompt);
                }
            }
            Edit::Newer => {
                let entry = HISTORY.lock().newer().map(String::from);
                if let Some(entry) = entry {
                    editor.set(&entry);
                    redraw(&editor, &mut prompt);
                }
            }
            Edit::Complete => tab(&mut editor, &mut prompt),
            Edit::Cancelled => {
                fb_println!("{}^C{}", GRAY, RESET);
                let _ = editor.take();
                prompt = show_prompt(&mut editor);
            }
            Edit::ClearScreen => {
                CONSOLE.lock().clear();
                prompt = show_prompt(&mut editor);
                redraw(&editor, &mut prompt);
            }
            Edit::ScrollUp => scroll(1),
            Edit::ScrollDown => scroll(-1),
            Edit::Ignored => {}
        }
    }
}

/// Scrolls the view half a screen: `1` towards older output, `-1` towards newer.
fn scroll(direction: isize) {
    let mut console = CONSOLE.lock();
    let page = (console.size().1 / 2).max(1) as isize;
    console.scroll_view(direction * page);
}

/// Handles the tab key: completes the word under the cursor or lists the choices.
fn tab(editor: &mut LineEditor, prompt: &mut Prompt) {
    let completion = complete::complete(editor.as_str(), editor.cursor());
    let typed = editor.cursor() - completion.start;
    let end = editor.cursor();

    match completion.candidates.as_slice() {
        [] => {}
        [only] => {
            let mut word = only.clone();
            if !word.ends_with('/') {
                word.push(' ');
            }
            if editor.replace_range(completion.start, end, &word) {
                redraw(editor, prompt);
            }
        }
        many => {
            let common = complete::common_prefix(many);
            if common.len() > typed {
                if editor.replace_range(completion.start, end, &common) {
                    redraw(editor, prompt);
                }
                return;
            }
            // nothing more to add: show the choices, then the prompt again
            fb_println!();
            let (cols, _) = CONSOLE.lock().size();
            let width = many.iter().map(|name| name.len()).max().unwrap_or(1) + 2;
            let per_row = (cols / width).max(1);
            for row in many.chunks(per_row) {
                for name in row {
                    let color = if name.ends_with('/') { BLUE } else { "" };
                    fb_print!("{}{:width$}{}", color, name, RESET, width = width);
                }
                fb_println!();
            }
            *prompt = show_prompt(editor);
            redraw(editor, prompt);
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
            fb_println!("{}parse error: {}{}", RED, error, RESET);
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
            fb_println!("{}{}: command not found{}", RED, name, RESET);
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
            fb_println!("{}{}: {}{}", RED, command.name, reason, RESET);
            Outcome::Failed
        }
        Err(CommandError::Reported) => Outcome::Failed,
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

#[test_case]
fn test_execute_file_commands() {
    *FILESYSTEM.lock() = Some(crate::fs::Filesystem::new());
    assert_eq!(execute("mkdir docs"), Outcome::Success);
    assert_eq!(execute("cd docs"), Outcome::Success);
    assert_eq!(execute("write note.txt hello there"), Outcome::Success);
    assert_eq!(execute("cat note.txt"), Outcome::Success);
    assert_eq!(execute("cp note.txt copy.txt"), Outcome::Success);
    assert_eq!(execute("mv copy.txt moved.txt"), Outcome::Success);
    assert_eq!(execute("cd .."), Outcome::Success);
    assert_eq!(execute("ls -l docs"), Outcome::Success);
    assert_eq!(execute("cat docs/copy.txt"), Outcome::Failed);
}
