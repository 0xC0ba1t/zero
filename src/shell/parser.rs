//! Splits a command line into words.

use alloc::string::String;
use alloc::vec::Vec;

/// Splits `line` on whitespace.
pub fn parse(line: &str) -> Vec<String> {
    line.split_whitespace().map(String::from).collect()
}

#[test_case]
fn test_parse_splits_on_whitespace() {
    let args = parse("  echo   hello  world ");
    assert_eq!(args, ["echo", "hello", "world"]);
}

#[test_case]
fn test_parse_blank_line() {
    assert!(parse("   ").is_empty());
}
