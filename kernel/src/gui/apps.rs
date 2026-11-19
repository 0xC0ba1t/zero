//! The applications the desktop can start, and how each one runs.

use super::draw::Icon;
use super::{files, settings, widgets};
use crate::sched::Context;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum App {
    Terminal,
    Files,
    Settings,
    Monitor,
    Browser,
    /// A built-in game or program, by name (see `script::programs`).
    Game(&'static str),
    Doom,
}

pub struct AppInfo {
    pub app: App,
    pub name: &'static str,
    pub icon: Icon,
    /// Size of the window's contents at 1280 pixels of screen width.
    pub size: (i32, i32),
    /// Only one window of this application at a time.
    pub single: bool,
    /// Closing the window only hides it (the program keeps its state).
    pub keep_alive: bool,
}

/// Everything in the dock, in order. A `None` entry draws a separator.
pub const DOCK: &[Option<AppInfo>] = &[
    Some(AppInfo { app: App::Terminal, name: "Terminal", icon: Icon::Terminal, size: (760, 450), single: true, keep_alive: true }),
    Some(AppInfo { app: App::Files, name: "Files", icon: Icon::Files, size: (760, 480), single: false, keep_alive: false }),
    Some(AppInfo { app: App::Settings, name: "Settings", icon: Icon::Settings, size: (720, 480), single: true, keep_alive: false }),
    Some(AppInfo { app: App::Browser, name: "Browser", icon: Icon::Browser, size: (860, 540), single: false, keep_alive: false }),
    Some(AppInfo { app: App::Monitor, name: "Activity", icon: Icon::Monitor, size: (560, 420), single: true, keep_alive: false }),
    None,
    Some(AppInfo { app: App::Game("snake"), name: "Snake", icon: Icon::Snake, size: (640, 400), single: true, keep_alive: false }),
    Some(AppInfo { app: App::Game("pong"), name: "Pong", icon: Icon::Pong, size: (640, 400), single: true, keep_alive: false }),
    Some(AppInfo { app: App::Game("invaders"), name: "Space Invaders", icon: Icon::Invaders, size: (640, 400), single: true, keep_alive: false }),
    Some(AppInfo { app: App::Game("tictactoe"), name: "Tic Tac Toe", icon: Icon::TicTacToe, size: (640, 400), single: true, keep_alive: false }),
    Some(AppInfo { app: App::Game("connect4"), name: "Connect Four", icon: Icon::Connect4, size: (640, 400), single: true, keep_alive: false }),
    Some(AppInfo { app: App::Game("calculator"), name: "Calculator", icon: Icon::Calculator, size: (640, 400), single: true, keep_alive: false }),
    Some(AppInfo { app: App::Game("bounce"), name: "Bounce", icon: Icon::Bounce, size: (640, 400), single: true, keep_alive: false }),
    Some(AppInfo { app: App::Doom, name: "DOOM", icon: Icon::Doom, size: (960, 600), single: true, keep_alive: true }),
];

pub fn info(app: App) -> Option<&'static AppInfo> {
    DOCK.iter().flatten().find(|i| i.app == app)
}

/// Starts the program for a window. The returned flag is set when it ends.
pub fn start(app: App, ctx: Arc<Context>) -> Arc<AtomicBool> {
    let done = Arc::new(AtomicBool::new(false));
    let finished = done.clone();
    let name = info(app).map(|i| i.name).unwrap_or("app");
    let stack_kib = match app {
        App::Doom => 4096,
        App::Terminal => 1024,
        _ => 512,
    };
    crate::sched::spawn_with(name, stack_kib, Some(ctx), move || {
        run(app);
        finished.store(true, Ordering::SeqCst);
    });
    done
}

fn run(app: App) {
    use crate::graphics::console::CONSOLE;
    match app {
        App::Terminal => {
            if let Some(ctx) = crate::sched::context() {
                ctx.terminal.store(true, Ordering::SeqCst);
            }
            CONSOLE.lock().init();
            crate::shell::run();
        }
        App::Files => widgets::run(&mut files::Files::new()),
        App::Settings => widgets::run(&mut settings::Settings::new()),
        App::Monitor => widgets::run(&mut settings::Monitor::new()),
        App::Browser => widgets::run(&mut super::browser::Browser::new()),
        App::Game(name) => run_game(name),
        App::Doom => {
            if let Some(ctx) = crate::sched::context() {
                ctx.raw_mouse.store(true, Ordering::SeqCst);
            }
            if let Err(message) = crate::doom::run() {
                show_error("DOOM", &message);
            }
        }
    }
}

fn run_game(name: &str) {
    let Some(entry) = crate::script::programs::find(name) else {
        show_error(name, "no such program");
        return;
    };
    let mut host = crate::script::kernel_host::KernelHost::new().with_title(name).standalone();
    if let Err(error) = crate::script::run(entry.source, &mut host) {
        drop(host);
        let mut text = alloc::format!("{}", error);
        text.insert_str(0, "error: ");
        show_error(name, &text);
    }
}

/// Shows an error in the window until it is closed.
fn show_error(title: &str, message: &str) {
    use super::draw;
    let Some(ctx) = crate::sched::context() else { return };
    let font = draw::font(16, false);
    let mut drawn = false;
    while !ctx.is_closed() {
        if !drawn {
            crate::graphics::framebuffer::with(|fb| {
                use crate::graphics::display::Display;
                let (w, h) = (fb.width() as i32, fb.height() as i32);
                draw::rect(fb, 0, 0, w, h, draw::BG);
                draw::text(fb, &draw::font(20, true), 20, 20, title, draw::RED);
                let max = ((w - 40) / font.width() as i32).max(10) as usize;
                let mut y = 56;
                let chars: alloc::vec::Vec<char> = message.chars().collect();
                for line in chars.chunks(max) {
                    let s: alloc::string::String = line.iter().collect();
                    draw::text(fb, &font, 20, y, &s, draw::TEXT);
                    y += font.height() as i32 + 4;
                }
            });
            drawn = true;
        }
        while ctx.events.lock().pop_front().is_some() {}
        crate::sched::yield_idle();
    }
}
