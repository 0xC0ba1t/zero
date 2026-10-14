//! Splits a command line into words.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// Why a command line could not be parsed.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    /// A `"` was opened but never closed.
    UnterminatedQuote,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::UnterminatedQuote => write!(f, "unterminated quote"),
        }
    }
}

/// Splits `line` into words. Words are separated by whitespace, except inside
/// double quotes, which group several words (or none, for an empty argument).
pub fn parse(line: &str) -> Result<Vec<String>, ParseError> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    // true once the current word has started, so that `""` yields an empty argument
    let mut in_word = false;

    for c in line.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                in_word = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if in_word {
                    args.push(core::mem::take(&mut current));
                    in_word = false;
                }
            }
            c => {
                current.push(c);
                in_word = true;
            }
        }
    }

    if in_quotes {
        return Err(ParseError::UnterminatedQuote);
    }
    if in_word {
        args.push(current);
    }
    Ok(args)
}

#[test_case]
fn test_parse_splits_on_whitespace() {
    let args = parse("  echo   hello  world ").unwrap();
    assert_eq!(args, ["echo", "hello", "world"]);
}

#[test_case]
fn test_parse_blank_line() {
    assert!(parse("   ").unwrap().is_empty());
}

#[test_case]
fn test_parse_quoted_argument() {
    let args = parse("echo \"hello   world\" end").unwrap();
    assert_eq!(args, ["echo", "hello   world", "end"]);
}

#[test_case]
fn test_parse_empty_quotes() {
    let args = parse("a \"\" b").unwrap();
    assert_eq!(args, ["a", "", "b"]);
}

#[test_case]
fn test_parse_unterminated_quote() {
    assert_eq!(parse("echo \"oops"), Err(ParseError::UnterminatedQuote));
}
