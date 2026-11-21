//! The applications the desktop can start, and how each one runs.

use super::icons::Icon;
use crate::sched::Context;
use alloc::string::String;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppId {
    Files,
    Terminal,
    Browser,
    Media,
    Notes,
    Buddy,
    Activity,
    Settings,
    About,
    /// A built-in ZeroScript app, by name (`programs/gui/<name>.zs`).
    Game(&'static str),
    /// A ZeroScript file from the file system (the path is the argument).
    Script,
    Doom,
}

pub struct AppInfo {
    pub id: AppId,
    pub name: &'static str,
    pub icon: Icon,
    pub size: (i32, i32),
    /// Only one window at a time.
    pub single: bool,
    /// Closing the window only hides it (the program keeps running).
    pub keep_alive: bool,
    pub category: &'static str,
}

const fn app(id: AppId, name: &'static str, icon: Icon, size: (i32, i32), single: bool, keep_alive: bool, category: &'static str) -> AppInfo {
    AppInfo { id, name, icon, size, single, keep_alive, category }
}

pub const APPS: &[AppInfo] = &[
    app(AppId::Files, "Files", Icon::Files, (820, 500), false, false, "System"),
    app(AppId::Terminal, "Terminal", Icon::Terminal, (760, 460), true, true, "System"),
    app(AppId::Browser, "Browser", Icon::Browser, (900, 560), false, false, "Internet"),
    app(AppId::Media, "Media", Icon::Media, (760, 520), false, false, "Media"),
    app(AppId::Notes, "Notes", Icon::Notes, (680, 480), false, false, "Productivity"),
    app(AppId::Buddy, "Bit", Icon::Buddy, (460, 560), true, false, "System"),
    app(AppId::Activity, "Activity", Icon::Activity, (620, 460), true, false, "System"),
    app(AppId::Settings, "Settings", Icon::Settings, (800, 540), true, false, "System"),
    app(AppId::About, "About zero", Icon::Settings, (420, 300), true, false, "System"),
    app(AppId::Game("snake"), "Snake", Icon::Snake, (640, 520), true, false, "Games"),
    app(AppId::Game("pong"), "Pong", Icon::Pong, (720, 480), true, false, "Games"),
    app(AppId::Game("invaders"), "Space Invaders", Icon::Invaders, (640, 560), true, false, "Games"),
    app(AppId::Game("tictactoe"), "Tic Tac Toe", Icon::TicTacToe, (480, 560), true, false, "Games"),
    app(AppId::Game("connect4"), "Connect Four", Icon::Connect4, (620, 600), true, false, "Games"),
    app(AppId::Game("calculator"), "Calculator", Icon::Calculator, (300, 460), true, false, "Productivity"),
    app(AppId::Game("bounce"), "Bounce", Icon::Bounce, (640, 460), true, false, "Games"),
    app(AppId::Script, "Script", Icon::Script, (640, 480), false, false, "Developer"),
    app(AppId::Doom, "DOOM", Icon::Doom, (960, 600), true, true, "Games"),
];

/// What the dock shows on the left, always.
pub const DOCK: &[AppId] = &[
    AppId::Files,
    AppId::Browser,
    AppId::Terminal,
    AppId::Media,
    AppId::Notes,
    AppId::Buddy,
    AppId::Activity,
    AppId::Settings,
];

pub fn info(id: AppId) -> &'static AppInfo {
    APPS.iter().find(|i| i.id == id).unwrap_or(&APPS[0])
}

/// The app that opens a file, by its name.
pub fn for_file(path: &str) -> AppId {
    let lower = path.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "png" | "bmp" | "gif" | "ppm" | "pgm" | "wav" | "zimg" => AppId::Media,
        "zs" => AppId::Script,
        _ => AppId::Notes,
    }
}

pub fn file_icon(name: &str, is_dir: bool) -> Icon {
    if is_dir {
        return Icon::Folder;
    }
    let lower = name.to_ascii_lowercase();
    match lower.rsplit('.').next().unwrap_or("") {
        "png" | "bmp" | "ppm" | "pgm" | "zimg" => Icon::Image,
        "gif" => Icon::Video,
        "wav" => Icon::Audio,
        "zs" => Icon::Code,
        _ => Icon::Document,
    }
}

/// Starts the program for a window. The returned flag is set when it ends.
pub fn start(id: AppId, ctx: Arc<Context>, arg: Option<String>) -> Arc<AtomicBool> {
    let done = Arc::new(AtomicBool::new(false));
    let finished = done.clone();
    *ctx.arg.lock() = arg;
    let name = info(id).name;
    let stack_kib = match id {
        AppId::Doom => 4096,
        AppId::Terminal => 1024,
        AppId::Game(_) | AppId::Script => 1024,
        _ => 512,
    };
    crate::sched::spawn_with(name, stack_kib, Some(ctx), move || {
        run(id);
        finished.store(true, Ordering::SeqCst);
    });
    done
}

fn run(id: AppId) {
    use crate::graphics::console::CONSOLE;
    match id {
        AppId::Terminal => {
            if let Some(ctx) = crate::sched::context() {
                ctx.terminal.store(true, Ordering::SeqCst);
                *ctx.title.lock() = String::from("Terminal");
            }
            CONSOLE.lock().init();
            crate::shell::run();
        }
        AppId::Files => super::ui::run(&mut super::files::Files::new(super::app_arg())),
        AppId::Notes => super::ui::run(&mut super::files::Notes::new(super::app_arg())),
        AppId::Settings => super::ui::run(&mut super::settings::Settings::new(super::app_arg())),
        AppId::About => super::ui::run(&mut super::settings::About::new()),
        AppId::Activity => super::ui::run(&mut super::settings::Activity::new()),
        AppId::Browser => super::ui::run(&mut super::browser::Browser::new(super::app_arg())),
        AppId::Media => super::ui::run(&mut super::media::Media::new(super::app_arg())),
        AppId::Buddy => super::ui::run(&mut super::buddy_desk::Chat::new()),
        AppId::Game(name) => super::zsapp::run_builtin(name),
        AppId::Script => {
            let path = super::app_arg().unwrap_or_default();
            super::zsapp::run_file(&path);
        }
        AppId::Doom => {
            if let Some(ctx) = crate::sched::context() {
                ctx.raw_mouse.store(true, Ordering::SeqCst);
                *ctx.title.lock() = String::from("DOOM");
            }
            if let Err(message) = crate::doom::run() {
                super::zsapp::show_error("DOOM", &message);
            }
        }
    }
}
