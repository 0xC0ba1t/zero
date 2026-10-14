//! zs, a tiny scripting language that runs inside the kernel.
//!
//! A script is compiled into a flat list of statements and then executed by a
//! small virtual machine. The machine never touches the hardware itself: all
//! input and output goes through the `host::Host` trait, which is what lets
//! the interpreter be tested with a fake host.

pub mod ast;
pub mod host;
pub mod lexer;

use alloc::string::String;
use core::fmt;

/// An error found while compiling or running a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError {
    /// 1-based line number, or 0 if there is no particular line.
    pub line: usize,
    pub message: String,
}

impl ScriptError {
    pub fn new(line: usize, message: &str) -> ScriptError {
        ScriptError {
            line,
            message: String::from(message),
        }
    }
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}
