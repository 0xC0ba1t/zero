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
    /// The command failed and has already printed its own message.
    Reported,
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
    Command {
        name: "breakpoint",
        usage: "breakpoint",
        about: "trigger a breakpoint exception",
        handler: breakpoint,
    },
    Command {
        name: "panic",
        usage: "panic",
        about: "panic on purpose",
        handler: crash,
    },
    Command {
        name: "reboot",
        usage: "reboot",
        about: "restart the machine",
        handler: reboot,
    },
    Command {
        name: "shutdown",
        usage: "shutdown",
        about: "power off (virtual machines only)",
        handler: shutdown,
    },
    Command {
        name: "history",
        usage: "history",
        about: "list the lines you entered",
        handler: history,
    },
    Command {
        name: "run",
        usage: "run <program>",
        about: "run a built-in script (see programs)",
        handler: run,
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

fn breakpoint(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    x86_64::instructions::interrupts::int3();
    Ok(())
}

fn crash(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    panic!("panic requested from the shell");
}

fn reboot(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    println!("rebooting...");
    crate::power::reboot()
}

fn shutdown(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    println!("shutting down...");
    crate::power::shutdown()
}

fn history(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    for (index, line) in super::history::HISTORY.lock().iter().enumerate() {
        println!("{:>3}  {}", index + 1, line);
    }
    Ok(())
}

/// Runs script source on the real screen and keyboard, reporting errors.
fn run_script(source: &str) -> Result<(), CommandError> {
    let mut host = crate::script::kernel_host::KernelHost::new();
    match crate::script::run(source, &mut host) {
        Ok(()) => Ok(()),
        Err(error) => {
            println!("script error: {}", error);
            Err(CommandError::Reported)
        }
    }
}

fn run(args: &[String]) -> Result<(), CommandError> {
    let name = match args {
        [name] => name,
        _ => return Err(CommandError::Usage),
    };
    let program = crate::script::programs::find(name)
        .ok_or(CommandError::Failed("no such program, see `programs`"))?;
    run_script(program.source)
}

/// Returns the name of the only command that starts with `prefix`, if there is exactly one.
pub fn complete(prefix: &str) -> Option<&'static str> {
    if prefix.is_empty() {
        return None;
    }
    let mut matches = COMMANDS
        .iter()
        .filter(|command| command.name.starts_with(prefix));
    let first = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(first.name)
    }
}

#[test_case]
fn test_complete_unique_prefix() {
    assert_eq!(complete("ec"), Some("echo"));
    assert_eq!(complete("shut"), Some("shutdown"));
}

#[test_case]
fn test_complete_rejects_ambiguous_or_unknown_prefixes() {
    assert_eq!(complete("c"), None); // clear, color
    assert_eq!(complete("zzz"), None);
    assert_eq!(complete(""), None);
}
