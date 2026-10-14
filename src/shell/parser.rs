//! Splits a command line into words.

use alloc::string::String;
use alloc::vec::Vec;

/// Splits `line` on whitespace.
pub fn parse(line: &str) -> Vec<String> {
    line.split_whitespace().map(String::from).collect()
}
