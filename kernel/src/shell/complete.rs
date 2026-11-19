//! Tab completion.

use super::commands::COMMANDS;
use crate::fs::FILESYSTEM;
use crate::graphics::vga::COLOR_NAMES;
use alloc::string::String;
use alloc::vec::Vec;

/// What could be typed instead of the word under the cursor.
pub struct Completion {
    /// Where the word starts in the line.
    pub start: usize,
    /// The possible replacements for the word, sorted. Directories end in `/`.
    pub candidates: Vec<String>,
}

/// Finds completions for the word that ends at `cursor`.
///
/// The first word is completed from the command names. Later words depend on
/// the command: `run` and `src` take program names, `help` takes commands,
/// `color` takes color names, everything else takes paths.
pub fn complete(line: &str, cursor: usize) -> Completion {
    let upto = &line[..cursor.min(line.len())];
    let start = upto.rfind(' ').map(|i| i + 1).unwrap_or(0);
    let word = &upto[start..];
    let before = upto[..start].trim();

    let mut candidates: Vec<String> = if before.is_empty() {
        starting_with(COMMANDS.iter().map(|c| c.name), word)
    } else {
        let mut words = before.split_whitespace();
        let command = words.next().unwrap_or("");
        let first_argument = words.next().is_none();
        match command {
            "help" if first_argument => starting_with(COMMANDS.iter().map(|c| c.name), word),
            "run" | "src" if first_argument => starting_with(
                crate::script::programs::PROGRAMS.iter().map(|p| p.name),
                word,
            ),
            "color" if first_argument => starting_with(COLOR_NAMES.iter().copied(), word),
            _ => paths(word),
        }
    };
    candidates.sort();
    candidates.dedup();
    Completion { start, candidates }
}

fn starting_with<'a>(names: impl Iterator<Item = &'a str>, prefix: &str) -> Vec<String> {
    names.filter(|name| name.starts_with(prefix)).map(String::from).collect()
}

/// Files and directories matching a partly typed path.
fn paths(word: &str) -> Vec<String> {
    let (dir, name) = match word.rfind('/') {
        Some(i) => (&word[..=i], &word[i + 1..]),
        None => ("", word),
    };
    let guard = FILESYSTEM.lock();
    let fs = match guard.as_ref() {
        Some(fs) => fs,
        None => return Vec::new(),
    };
    let entries = match fs.entries(if dir.is_empty() { "." } else { dir }) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    entries
        .into_iter()
        .filter(|entry| entry.name.starts_with(name))
        .map(|entry| {
            let mut full = String::from(dir);
            full.push_str(&entry.name);
            if entry.is_dir {
                full.push('/');
            }
            full
        })
        .collect()
}

/// The longest prefix all of `names` share.
pub fn common_prefix(names: &[String]) -> String {
    let first = match names.first() {
        Some(first) => first.as_str(),
        None => return String::new(),
    };
    let mut len = first.len();
    for name in &names[1..] {
        len = len.min(
            first
                .bytes()
                .zip(name.bytes())
                .take_while(|(a, b)| a == b)
                .count(),
        );
    }
    String::from(&first[..len])
}

#[test_case]
fn test_complete_unique_command() {
    let c = complete("ec", 2);
    assert_eq!(c.start, 0);
    assert_eq!(c.candidates, ["echo"]);
}

#[test_case]
fn test_complete_ambiguous_and_unknown() {
    let c = complete("c", 1);
    assert!(c.candidates.len() > 1);
    assert!(complete("zzz", 3).candidates.is_empty());
}

#[test_case]
fn test_complete_arguments_depend_on_the_command() {
    assert_eq!(complete("run hel", 7).candidates, ["hello"]);
    assert_eq!(complete("run hel", 7).start, 4);
    assert_eq!(complete("color lightg", 12).candidates, ["lightgray", "lightgreen"]);
    assert_eq!(complete("help ech", 8).candidates, ["echo"]);
}

#[test_case]
fn test_common_prefix() {
    let names = [String::from("lightgray"), String::from("lightgreen")];
    assert_eq!(common_prefix(&names), "lightgr");
    assert_eq!(common_prefix(&[]), "");
}
