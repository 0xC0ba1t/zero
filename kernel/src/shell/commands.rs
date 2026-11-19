//! Built-in shell commands.

use crate::fs::{Filesystem, FsError, FILESYSTEM};
use crate::graphics::console::CONSOLE;
use crate::graphics::vga;
use crate::input::{self, Key};
use crate::{fb_print, fb_println};
use alloc::format;
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

macro_rules! command {
    ($name:expr, $usage:expr, $about:expr, $handler:expr) => {
        Command { name: $name, usage: $usage, about: $about, handler: $handler }
    };
}

/// Every command the shell knows about.
pub static COMMANDS: &[Command] = &[
    command!("help", "help [command]", "list commands, or show how to use one", help),
    command!("echo", "echo [text...]", "print its arguments", echo),
    command!("clear", "clear", "clear the screen", clear),
    command!("ls", "ls [-l] [path]", "list directory contents", ls),
    command!("cd", "cd [path]", "change the current directory", cd),
    command!("pwd", "pwd", "print the current directory", pwd),
    command!("cat", "cat <path>", "print file contents", cat),
    command!("write", "write <path> <text...>", "write text to a file (creates it)", write),
    command!("touch", "touch <path>", "create a new empty file", touch),
    command!("mkdir", "mkdir <path>", "create a new directory", mkdir),
    command!("rm", "rm <path>", "remove a file or directory", rm),
    command!("cp", "cp <from> <to>", "copy a file", cp),
    command!("mv", "mv <from> <to>", "move or rename a file", mv),
    command!("edit", "edit <path>", "edit a file (esc saves, ctrl+c discards)", edit),
    command!("run", "run <program>", "run a built-in script (see programs)", run),
    command!("programs", "programs", "list the built-in scripts", programs),
    command!("src", "src <program>", "show the source of a built-in script", src),
    command!("exec", "exec <script>", "run a one-line script, `;` separates statements", exec),
    command!("eval", "eval <expression>", "print the value of an expression", eval),
    command!("color", "color [name]", "set the text color, or list the colors", color),
    command!("history", "history", "list the lines you entered", history),
    command!("uptime", "uptime", "show the time since boot", uptime),
    command!("mem", "mem", "show heap usage", mem),
    command!("screen", "screen", "show the display mode", screen),
    command!("bench", "bench", "measure how fast the screen can be drawn", bench),
    command!("cube", "cube", "show a spinning 3D cube", cube_demo),
    command!("version", "version", "print the kernel name and version", version),
    command!("breakpoint", "breakpoint", "trigger a breakpoint exception", breakpoint),
    command!("panic", "panic", "panic on purpose", crash),
    command!("reboot", "reboot", "restart the machine", reboot),
    command!("shutdown", "shutdown", "power off (virtual machines only)", shutdown),
];

/// Looks up a command by name.
pub fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| command.name == name)
}

// ----- helpers -----

/// Fails with a usage error if any arguments were given.
fn expect_no_args(args: &[String]) -> Result<(), CommandError> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(CommandError::Usage)
    }
}

fn fs_error(error: FsError) -> CommandError {
    CommandError::Failed(match error {
        FsError::NotFound => "no such file or directory",
        FsError::AlreadyExists => "already exists",
        FsError::NotADirectory => "not a directory",
        FsError::IsADirectory => "is a directory",
        FsError::InvalidPath => "invalid path",
    })
}

/// Runs `f` with the file system.
fn with_fs<R>(f: impl FnOnce(&mut Filesystem) -> Result<R, FsError>) -> Result<R, CommandError> {
    let mut guard = FILESYSTEM.lock();
    let fs = guard.as_mut().ok_or(CommandError::Failed("file system not ready"))?;
    f(fs).map_err(fs_error)
}

const BLUE: &str = "\x1b[94m";
const CYAN: &str = "\x1b[96m";
const GRAY: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

// ----- general -----

fn echo(args: &[String]) -> Result<(), CommandError> {
    fb_println!("{}", args.join(" "));
    Ok(())
}

fn help(args: &[String]) -> Result<(), CommandError> {
    match args {
        [] => {
            for command in COMMANDS {
                fb_println!("  {}{:<12}{}{}", CYAN, command.name, RESET, command.about);
            }
            fb_println!("{}  tab completes, up/down browse history, page up/down scroll{}", GRAY, RESET);
            Ok(())
        }
        [name] => {
            let command = find(name).ok_or(CommandError::Failed("no such command"))?;
            fb_println!("usage: {}", command.usage);
            fb_println!("{}", command.about);
            Ok(())
        }
        _ => Err(CommandError::Usage),
    }
}

fn clear(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    CONSOLE.lock().clear();
    Ok(())
}

fn version(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    fb_println!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    Ok(())
}

fn uptime(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    let (hours, minutes, seconds) = crate::time::split_hms(crate::time::uptime_secs());
    fb_println!("up {}h {:02}m {:02}s", hours, minutes, seconds);
    Ok(())
}

fn mib(bytes: usize) -> String {
    format!("{}.{} MiB", bytes / (1024 * 1024), (bytes % (1024 * 1024)) * 10 / (1024 * 1024))
}

fn mem(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    let used = crate::allocator::heap_used();
    let free = crate::allocator::heap_free();
    fb_println!("heap  {} total", mib(crate::allocator::HEAP_SIZE));
    fb_println!("      {} used", mib(used));
    fb_println!("      {} free", mib(free));
    Ok(())
}

fn color(args: &[String]) -> Result<(), CommandError> {
    match args {
        [] => {
            for (index, name) in vga::COLOR_NAMES.iter().enumerate() {
                fb_print!("\x1b[38;5;{}m{}\x1b[0m ", index, name);
            }
            fb_println!();
            Ok(())
        }
        [name] => {
            let index = vga::color_index(name).ok_or(CommandError::Failed("unknown color"))?;
            CONSOLE.lock().set_default_fg(index);
            Ok(())
        }
        _ => Err(CommandError::Usage),
    }
}

fn history(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    for (index, line) in super::history::HISTORY.lock().iter().enumerate() {
        fb_println!("{}{:>4}{}  {}", GRAY, index + 1, RESET, line);
    }
    Ok(())
}

fn screen(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    let mode = crate::graphics::framebuffer::with(|fb| {
        (fb.info.width, fb.info.height, fb.info.bytes_per_pixel * 8, fb.info.pixel_format)
    })
    .ok_or(CommandError::Failed("no framebuffer"))?;
    let (cols, rows) = CONSOLE.lock().size();
    fb_println!("display  {}x{}, {} bits per pixel, {:?}", mode.0, mode.1, mode.2, mode.3);
    fb_println!("console  {} columns x {} rows", cols, rows);
    fb_println!("scripts  80x25 cell grid, {}x{} pixels per cell", mode.0 / 80, mode.1 / 25);
    Ok(())
}

/// Fills the whole screen repeatedly and reports the frame rate.
fn bench(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    const FRAMES: u64 = 120;
    let start = crate::time::uptime_ms();
    let size = crate::graphics::framebuffer::with(|fb| {
        use crate::graphics::display::Display;
        for frame in 0..FRAMES {
            let shade = (frame * 2 % 256) as u32;
            fb.clear_to(shade << 16 | shade << 8 | shade);
            fb.flush();
        }
        (fb.width(), fb.height())
    })
    .ok_or(CommandError::Failed("no framebuffer"))?;
    let ms = (crate::time::uptime_ms() - start).max(1);
    CONSOLE.lock().restore();
    let pixels = size.0 as u64 * size.1 as u64 * FRAMES;
    fb_println!(
        "{} full-screen frames at {}x{} in {} ms: {} fps, {} Mpixel/s",
        FRAMES, size.0, size.1, ms, FRAMES * 1000 / ms, pixels / ms / 1000
    );
    Ok(())
}

fn cube_demo(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    crate::graphics::framebuffer::with(crate::graphics::demo::run_demo)
        .ok_or(CommandError::Failed("graphics not initialized"))?;
    // The demo drew straight onto the screen; bring the console back.
    CONSOLE.lock().restore();
    Ok(())
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
    fb_println!("rebooting...");
    crate::power::reboot()
}

fn shutdown(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    fb_println!("shutting down...");
    crate::power::shutdown()
}

// ----- scripts -----

/// Runs script source on the real screen and keyboard, reporting errors.
fn run_script(source: &str) -> Result<(), CommandError> {
    let mut host = crate::script::kernel_host::KernelHost::new();
    match crate::script::run(source, &mut host) {
        Ok(()) => Ok(()),
        Err(error) => {
            drop(host);
            fb_println!("script error: {}", error);
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
        fb_println!("  {}{:<13}{}{}", CYAN, program.name, RESET, program.about);
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
    run_script(&format!("print {}", args.join(" ")))
}

fn src(args: &[String]) -> Result<(), CommandError> {
    let name = match args {
        [name] => name,
        _ => return Err(CommandError::Usage),
    };
    let program = crate::script::programs::find(name)
        .ok_or(CommandError::Failed("no such program, see `programs`"))?;
    for line in program.source.lines() {
        fb_println!("{}", line);
    }
    Ok(())
}

// ----- files -----

fn ls(args: &[String]) -> Result<(), CommandError> {
    let (long, path) = match args {
        [] => (false, "."),
        [flag] if flag == "-l" => (true, "."),
        [path] => (false, path.as_str()),
        [flag, path] if flag == "-l" => (true, path.as_str()),
        _ => return Err(CommandError::Usage),
    };
    let entries = with_fs(|fs| fs.entries(path))?;
    if entries.is_empty() {
        return Ok(());
    }
    if long {
        for entry in &entries {
            if entry.is_dir {
                fb_println!("{}  {:>8}  {}{}/{}", GRAY, "-", BLUE, entry.name, RESET);
            } else {
                fb_println!("{}  {:>8}{}  {}", GRAY, entry.size, RESET, entry.name);
            }
        }
        return Ok(());
    }
    // short form: fill the screen width with columns
    let (cols, _) = CONSOLE.lock().size();
    let width = entries.iter().map(|e| e.name.len() + 1).max().unwrap_or(1) + 2;
    let per_row = (cols / width).max(1);
    for chunk in entries.chunks(per_row) {
        for entry in chunk {
            let pad = width - entry.name.len() - if entry.is_dir { 1 } else { 0 };
            if entry.is_dir {
                fb_print!("{}{}/{}{:pad$}", BLUE, entry.name, RESET, "", pad = pad);
            } else {
                fb_print!("{}{:pad$}", entry.name, "", pad = pad);
            }
        }
        fb_println!();
    }
    Ok(())
}

fn cd(args: &[String]) -> Result<(), CommandError> {
    let path = match args {
        [] => "/",
        [path] => path.as_str(),
        _ => return Err(CommandError::Usage),
    };
    with_fs(|fs| fs.set_cwd(path))
}

fn pwd(args: &[String]) -> Result<(), CommandError> {
    expect_no_args(args)?;
    let cwd = with_fs(|fs| Ok(fs.get_cwd()))?;
    fb_println!("{}", cwd);
    Ok(())
}

fn cat(args: &[String]) -> Result<(), CommandError> {
    let path = match args {
        [path] => path,
        _ => return Err(CommandError::Usage),
    };
    let data = with_fs(|fs| fs.read_file(path))?;
    let text = String::from_utf8_lossy(&data);
    fb_print!("{}", text);
    if !text.ends_with('\n') {
        fb_println!();
    }
    Ok(())
}

fn write(args: &[String]) -> Result<(), CommandError> {
    let (path, text) = match args {
        [path, text @ ..] if !text.is_empty() => (path, text.join(" ")),
        _ => return Err(CommandError::Usage),
    };
    let mut data = text.into_bytes();
    data.push(b'\n');
    with_fs(|fs| fs.write_file(path, data))
}

fn touch(args: &[String]) -> Result<(), CommandError> {
    match args {
        [path] => with_fs(|fs| fs.create_file(path)),
        _ => Err(CommandError::Usage),
    }
}

fn mkdir(args: &[String]) -> Result<(), CommandError> {
    match args {
        [path] => with_fs(|fs| fs.create_dir(path)),
        _ => Err(CommandError::Usage),
    }
}

fn rm(args: &[String]) -> Result<(), CommandError> {
    match args {
        [path] => with_fs(|fs| fs.delete(path)),
        _ => Err(CommandError::Usage),
    }
}

fn cp(args: &[String]) -> Result<(), CommandError> {
    match args {
        [from, to] => with_fs(|fs| {
            let data = fs.read_file(from)?;
            // copying into a directory keeps the file name
            let target = if fs.is_dir(to) {
                let name = from.rsplit('/').next().unwrap_or(from);
                format!("{}/{}", to, name)
            } else {
                to.clone()
            };
            fs.write_file(&target, data)
        }),
        _ => Err(CommandError::Usage),
    }
}

fn mv(args: &[String]) -> Result<(), CommandError> {
    match args {
        [from, to] => {
            cp(args)?;
            with_fs(|fs| fs.delete(from))?;
            let _ = to;
            Ok(())
        }
        _ => Err(CommandError::Usage),
    }
}

// ----- the editor -----

/// A small full-screen text editor. Escape saves and quits, ctrl+c quits
/// without saving, ctrl+s saves.
fn edit(args: &[String]) -> Result<(), CommandError> {
    let path = match args {
        [path] => path,
        _ => return Err(CommandError::Usage),
    };
    let mut lines: Vec<String> = match with_fs(|fs| fs.read_file(path)) {
        Ok(data) => String::from_utf8_lossy(&data).lines().map(String::from).collect(),
        Err(_) => Vec::new(),
    };
    if lines.is_empty() {
        lines.push(String::new());
    }

    let (cols, rows) = CONSOLE.lock().size();
    let text_rows = rows.saturating_sub(1).max(1);
    let max_len = cols.saturating_sub(1).max(1);
    let (mut cx, mut cy, mut top) = (0usize, 0usize, 0usize);
    let mut modified = false;
    let mut save = false;

    let store = |lines: &[String]| -> Result<(), CommandError> {
        let mut data = lines.join("\n").into_bytes();
        data.push(b'\n');
        with_fs(|fs| fs.write_file(path, data))
    };

    loop {
        if cy < top {
            top = cy;
        } else if cy >= top + text_rows {
            top = cy + 1 - text_rows;
        }
        draw_editor(&lines, top, text_rows, cols, cx, cy, path, modified);

        match input::read_key() {
            Key::Char('\u{1b}') => {
                save = true;
                break;
            }
            Key::Char('\u{3}') => break,
            Key::Char('\u{13}') => {
                store(&lines)?;
                modified = false;
            }
            Key::Char('\n') | Key::Char('\r') => {
                let rest = lines[cy].split_off(cx);
                lines.insert(cy + 1, rest);
                cy += 1;
                cx = 0;
                modified = true;
            }
            Key::Char('\u{8}') => {
                if cx > 0 {
                    lines[cy].remove(cx - 1);
                    cx -= 1;
                    modified = true;
                } else if cy > 0 {
                    let line = lines.remove(cy);
                    cy -= 1;
                    cx = lines[cy].len();
                    lines[cy].push_str(&line);
                    lines[cy].truncate(max_len);
                    modified = true;
                }
            }
            Key::Char('\u{7f}') | Key::Delete => {
                if cx < lines[cy].len() {
                    lines[cy].remove(cx);
                    modified = true;
                } else if cy + 1 < lines.len() {
                    let next = lines.remove(cy + 1);
                    lines[cy].push_str(&next);
                    lines[cy].truncate(max_len);
                    modified = true;
                }
            }
            Key::Char('\t') => {
                for _ in 0..4 {
                    if lines[cy].len() < max_len {
                        lines[cy].insert(cx, ' ');
                        cx += 1;
                        modified = true;
                    }
                }
            }
            Key::Char(c) if c.is_ascii() && !c.is_ascii_control() => {
                if lines[cy].len() < max_len {
                    lines[cy].insert(cx, c);
                    cx += 1;
                    modified = true;
                }
            }
            Key::Left => {
                if cx > 0 {
                    cx -= 1;
                } else if cy > 0 {
                    cy -= 1;
                    cx = lines[cy].len();
                }
            }
            Key::Right => {
                if cx < lines[cy].len() {
                    cx += 1;
                } else if cy + 1 < lines.len() {
                    cy += 1;
                    cx = 0;
                }
            }
            Key::Up => {
                cy = cy.saturating_sub(1);
                cx = cx.min(lines[cy].len());
            }
            Key::Down => {
                if cy + 1 < lines.len() {
                    cy += 1;
                }
                cx = cx.min(lines[cy].len());
            }
            Key::PageUp => {
                cy = cy.saturating_sub(text_rows);
                cx = cx.min(lines[cy].len());
            }
            Key::PageDown => {
                cy = (cy + text_rows).min(lines.len() - 1);
                cx = cx.min(lines[cy].len());
            }
            Key::Home => cx = 0,
            Key::End => cx = lines[cy].len(),
            _ => {}
        }
    }

    let result = if save { store(&lines) } else { Ok(()) };
    fb_print!("\x1b[2J");
    result
}

/// Paints the whole editor in one go (one console write, so one screen update).
fn draw_editor(
    lines: &[String],
    top: usize,
    text_rows: usize,
    cols: usize,
    cx: usize,
    cy: usize,
    path: &str,
    modified: bool,
) {
    let mut out = String::with_capacity((cols + 1) * (text_rows + 2));
    out.push_str("\x1b[2J\x1b[H");
    for row in 0..text_rows {
        match lines.get(top + row) {
            Some(line) => out.push_str(line),
            None => out.push_str("\x1b[90m~\x1b[0m"),
        }
        out.push('\n');
    }
    let status = format!(
        " {}{}   Ln {}, Col {}   esc: save+quit  ctrl+s: save  ctrl+c: discard",
        path,
        if modified { " [modified]" } else { "" },
        cy + 1,
        cx + 1
    );
    out.push_str("\x1b[30;47m");
    for i in 0..cols {
        out.push(status.as_bytes().get(i).map(|&b| b as char).unwrap_or(' '));
    }
    out.push_str("\x1b[0m");
    out.push_str(&format!("\x1b[{};{}H", cy - top + 1, cx + 1));
    fb_print!("{}", out);
}
