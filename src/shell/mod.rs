//! A tiny interactive shell.

mod commands;
mod line;
mod parser;

use crate::println;
use commands::CommandError;

/// Parses and runs a single command line.
pub fn execute(line: &str) {
    let args = match parser::parse(line) {
        Ok(args) => args,
        Err(error) => {
            println!("parse error: {}", error);
            return;
        }
    };

    let (name, rest) = match args.split_first() {
        Some(split) => split,
        None => return,
    };

    match commands::find(name) {
        Some(command) => match (command.handler)(rest) {
            Ok(()) => {}
            Err(CommandError::Usage) => println!("usage: {}", command.usage),
            Err(CommandError::Failed(reason)) => println!("{}: {}", command.name, reason),
        },
        None => println!("{}: command not found", name),
    }
}
