//! Built-in shell commands.

use crate::{print, println};
use alloc::string::String;
use alloc::vec::Vec;

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
    Command {
        name: "programs",
        usage: "programs",
        about: "list the built-in scripts",
        handler: programs,
    },
    Command {
        name: "exec",
        usage: "exec <script>",
        about: "run a one-line script, `;` separates statements",
        handler: exec,
    },
    Command {
        name: "eval",
        usage: "eval <expression>",
        about: "print the value of an expression",
        handler: eval,
    },
    Command {
        name: "src",
        usage: "src <program>",
        about: "show the source of a built-in script",
        handler: src,
    },
    Command {
        name: "ls",
        usage: "ls [path]",
        about: "list directory contents",
        handler: ls,
    },
    Command {
        name: "cat",
        usage: "cat <path>",
        about: "print file contents",
        handler: cat,
    },
    Command {
        name: "touch",
        usage: "touch <path>",
        about: "create a new empty file",
        handler: touch,
    },
    Command {
        name: "rm",
        usage: "rm <path>",
        about: "remove a file or directory",
        handler: rm,
    },
    Command {
        name: "mkdir",
        usage: "mkdir <path>",
        about: "create a new directory",
        handler: mkdir,
    },
    Command {
        name: "pwd",
        usage: "pwd",
        about: "print current working directory",
        handler: pwd,
    },
    Command {
        name: "cd",
        usage: "cd <path>",
        about: "change current working directory",
        handler: cd,
    },
    Command {
        name: "edit",
        usage: "edit <path>",
        about: "edit a file (simple line editor)",
        handler: edit,
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

fn programs(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    for program in crate::script::programs::PROGRAMS {
        println!("  {:<11}{}", program.name, program.about);
    }
    Ok(())
}

fn exec(args: &[String]) -> Result<(), CommandError> {
    if args.is_empty() {
        return Err(CommandError::Usage);
    }
    run_script(&args.join(" "))
}

fn eval(args: &[String]) -> Result<(), CommandError> {
    if args.is_empty() {
        return Err(CommandError::Usage);
    }
    run_script(&alloc::format!("print {}", args.join(" ")))
}

fn src(args: &[String]) -> Result<(), CommandError> {
    let name = match args {
        [name] => name,
        _ => return Err(CommandError::Usage),
    };
    let program = crate::script::programs::find(name)
        .ok_or(CommandError::Failed("no such program, see `programs`"))?;
    for line in program.source.lines() {
        println!("{}", line);
    }
    Ok(())
}

fn ls(args: &[String]) -> Result<(), CommandError> {
    let path = match args {
        [] => "/",
        [path] => path,
        _ => return Err(CommandError::Usage),
    };
    let fs = crate::fs::FILESYSTEM.lock();
    let fs = fs.as_ref().ok_or(CommandError::Failed("fs not initialized"))?;
    let list = fs.list(path)
        .map_err(|_| CommandError::Failed("no such directory"))?;
    for item in list {
        println!("{}", item);
    }
    Ok(())
}

fn cat(args: &[String]) -> Result<(), CommandError> {
    let path = match args {
        [path] => path,
        _ => return Err(CommandError::Usage),
    };
    let fs = crate::fs::FILESYSTEM.lock();
    let fs = fs.as_ref().ok_or(CommandError::Failed("fs not initialized"))?;
    let data = fs.read_file(path)
        .map_err(|_| CommandError::Failed("no such file"))?;
    println!("{}", alloc::string::String::from_utf8_lossy(&data));
    Ok(())
}

fn touch(args: &[String]) -> Result<(), CommandError> {
    let path = match args {
        [path] => path,
        _ => return Err(CommandError::Usage),
    };
    crate::fs::FILESYSTEM
        .lock()
        .as_mut()
        .ok_or(CommandError::Failed("fs not initialized"))?
        .create_file(path)
        .map_err(|_| CommandError::Failed("could not create file"))?;
    Ok(())
}

fn rm(args: &[String]) -> Result<(), CommandError> {
    let path = match args {
        [path] => path,
        _ => return Err(CommandError::Usage),
    };
    crate::fs::FILESYSTEM
        .lock()
        .as_mut()
        .ok_or(CommandError::Failed("fs not initialized"))?
        .delete(path)
        .map_err(|_| CommandError::Failed("could not remove file/directory"))?;
    Ok(())
}

fn mkdir(args: &[String]) -> Result<(), CommandError> {
    let path = match args {
        [path] => path,
        _ => return Err(CommandError::Usage),
    };
    crate::fs::FILESYSTEM
        .lock()
        .as_mut()
        .ok_or(CommandError::Failed("fs not initialized"))?
        .create_dir(path)
        .map_err(|_| CommandError::Failed("could not create directory"))?;
    Ok(())
}

fn pwd(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    let fs = crate::fs::FILESYSTEM.lock();
    let fs = fs.as_ref().ok_or(CommandError::Failed("fs not initialized"))?;
    println!("{}", fs.get_cwd());
    Ok(())
}

fn cd(args: &[String]) -> Result<(), CommandError> {
    let path = match args {
        [path] => path,
        _ => return Err(CommandError::Usage),
    };
    crate::fs::FILESYSTEM
        .lock()
        .as_mut()
        .ok_or(CommandError::Failed("fs not initialized"))?
        .set_cwd(path)
        .map_err(|_| CommandError::Failed("could not change directory"))?;
    Ok(())
}

fn edit(args: &[String]) -> Result<(), CommandError> {
    let path = match args {
        [path] => path,
        _ => return Err(CommandError::Usage),
    };

    let content = {
        let fs = crate::fs::FILESYSTEM.lock();
        let fs = fs.as_ref().ok_or(CommandError::Failed("fs not initialized"))?;
        fs.read_file(path).unwrap_or_default()
    };

    // Fullscreen editor implementation
    crate::vga_buffer::clear_screen();

    let mut lines: Vec<String> = alloc::string::String::from_utf8_lossy(&content)
        .lines()
        .map(String::from)
        .collect();
    if lines.is_empty() { lines.push(String::new()); }

    let mut cursor_x = 0;
    let mut cursor_y = 0;
    let _scroll_y = 0;

    loop {
        // Redraw screen
        crate::vga_buffer::clear_screen();
        for i in 0..25 {
            if i < lines.len() {
                let row_content = &lines[i];
                crate::vga_buffer::write_at(0, i as i32, row_content);
            }
        }

        // Input loop
        let key = crate::input::read_key();
        match key {
            crate::input::Key::Char(c) => {
                if c == '\u{1b}' { // ESC to quit/save menu
                    break;
                } else if c == '\u{8}' {
                    if cursor_x > 0 {
                        lines[cursor_y].remove(cursor_x - 1);
                        cursor_x -= 1;
                    }
                } else if c == '\n' {
                    let split = lines[cursor_y].split_off(cursor_x);
                    lines.insert(cursor_y + 1, split);
                    cursor_y += 1;
                    cursor_x = 0;
                } else {
                    lines[cursor_y].insert(cursor_x, c);
                    cursor_x += 1;
                }
            }
            crate::input::Key::Up => if cursor_y > 0 { cursor_y -= 1; cursor_x = core::cmp::min(cursor_x, lines[cursor_y].len()); },
            crate::input::Key::Down => if cursor_y < lines.len() - 1 { cursor_y += 1; cursor_x = core::cmp::min(cursor_x, lines[cursor_y].len()); },
            crate::input::Key::Left => if cursor_x > 0 { cursor_x -= 1; },
            crate::input::Key::Right => if cursor_x < lines[cursor_y].len() { cursor_x += 1; },
            _ => {}
        }
    }

    // Simplified save logic for now
    let new_content = lines.join("\n").into_bytes();
    crate::fs::FILESYSTEM
        .lock()
        .as_mut()
        .ok_or(CommandError::Failed("fs not initialized"))?
        .write_file(path, new_content)
        .map_err(|_| CommandError::Failed("could not save file"))?;

    crate::vga_buffer::clear_screen();
    Ok(())
}

/// Returns the name of the only command or file that starts with `prefix`, if there is exactly one.
pub fn complete(prefix: &str) -> Option<String> {
    if prefix.is_empty() {
        return None;
    }

    // Try completing commands
    let command_matches: Vec<_> = COMMANDS
        .iter()
        .filter(|command| command.name.starts_with(prefix))
        .collect();

    // Try completing files (if filesystem is initialized)
    let file_matches = if let Some(Ok(fs)) = crate::fs::FILESYSTEM.lock().as_ref().map(|fs| fs.list("/")) {
        fs.iter()
            .filter(|name| name.starts_with(prefix))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let all_matches: Vec<String> = command_matches
        .iter()
        .map(|c| String::from(c.name))
        .chain(file_matches.into_iter())
        .collect();

    if all_matches.len() == 1 {
        Some(all_matches[0].clone())
    } else {
        None
    }
}

#[test_case]
fn test_complete_unique_prefix() {
    assert_eq!(complete("ec"), Some(String::from("echo")));
    assert_eq!(complete("shut"), Some(String::from("shutdown")));
}

#[test_case]
fn test_complete_rejects_ambiguous_or_unknown_prefixes() {
    assert_eq!(complete("c"), None); // clear, color
    assert_eq!(complete("zzz"), None);
    assert_eq!(complete(""), None);
}
