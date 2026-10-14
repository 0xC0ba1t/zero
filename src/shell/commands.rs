//! Built-in shell commands.

use crate::{print, println};
use alloc::string::String;

/// Why a command did not succeed.
#[derive(Debug, PartialEq, Eq)]
pub enum CommandError {
    /// The command was called with the wrong arguments.
    Usage,
    /// The command failed for the given reason.
    Failed(&'static str),
}

/// Signature shared by every built-in. It receives the arguments that follow
/// the command name.
pub type Handler = fn(args: &[String]) -> Result<(), CommandError>;

/// A built-in command.
pub struct Command {
    pub name: &'static str,
    pub usage: &'static str,
    pub about: &'static str,
    pub handler: Handler,
}

/// Every command the shell knows about.
pub static COMMANDS: &[Command] = &[
    Command {
        name: "echo",
        usage: "echo [text...]",
        about: "print its arguments",
        handler: echo,
    },
    Command {
        name: "help",
        usage: "help [command]",
        about: "list commands, or show how to use one",
        handler: help,
    },
    Command {
        name: "clear",
        usage: "clear",
        about: "clear the screen",
        handler: clear,
    },
    Command {
        name: "version",
        usage: "version",
        about: "print the kernel name and version",
        handler: version,
    },
    Command {
        name: "uptime",
        usage: "uptime",
        about: "show the time since boot",
        handler: uptime,
    },
    Command {
        name: "mem",
        usage: "mem",
        about: "show heap usage",
        handler: mem,
    },
    Command {
        name: "color",
        usage: "color [name]",
        about: "set the text color, or list the colors",
        handler: color,
    },
];

/// Looks up a command by name.
pub fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| command.name == name)
}

fn echo(args: &[String]) -> Result<(), CommandError> {
    println!("{}", args.join(" "));
    Ok(())
}

/// Fails with a usage error if any arguments were given.
fn expect_no_args(args: &[String]) -> Result<(), CommandError> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(CommandError::Usage)
    }
}

fn help(args: &[String]) -> Result<(), CommandError> {
    match args {
        [] => {
            for command in COMMANDS {
                println!("  {:<11}{}", command.name, command.about);
            }
            Ok(())
        }
        [name] => {
            let command = find(name).ok_or(CommandError::Failed("no such command"))?;
            println!("usage: {}", command.usage);
            println!("{}", command.about);
            Ok(())
        }
        _ => Err(CommandError::Usage),
    }
}

fn clear(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    crate::vga_buffer::clear_screen();
    Ok(())
}

fn version(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    println!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    Ok(())
}

fn uptime(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    let (hours, minutes, seconds) = crate::time::split_hms(crate::time::uptime_secs());
    println!("up {}h {:02}m {:02}s", hours, minutes, seconds);
    Ok(())
}

fn mem(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    println!("heap: {} bytes total", crate::allocator::HEAP_SIZE);
    println!("      {} bytes used", crate::allocator::heap_used());
    println!("      {} bytes free", crate::allocator::heap_free());
    Ok(())
}

fn color(args: &[String]) -> Result<(), CommandError> {
    use crate::vga_buffer::{self, Color};

    match args {
        [] => {
            for (name, _) in Color::NAMES.iter() {
                print!("{} ", name);
            }
            println!();
            Ok(())
        }
        [name] => {
            let foreground =
                Color::from_name(name).ok_or(CommandError::Failed("unknown color"))?;
            if foreground == Color::Black {
                return Err(CommandError::Failed("black on black would be invisible"));
            }
            vga_buffer::set_color(foreground, Color::Black);
            Ok(())
        }
        _ => Err(CommandError::Usage),
    }
}
