//! The desktop.
//!
//! A menu bar on top, a dock at the bottom and windows in between, modelled
//! on macOS: frosted glass bars, a magnifying dock, rounded windows with soft
//! shadows and animations, Spotlight, Mission Control, Launchpad, a control
//! centre and notifications, plus window snapping and tiling.
//!
//! Every window is an application running in its own thread (see `sched`).
//! The application draws into the window's own surface; the desktop thread
//! composes the surfaces (on all processors) and routes input.

pub mod apps;
pub mod browser;
pub mod buddy_desk;
pub mod desktop;
pub mod files;
pub mod gfx;
pub mod icons;
pub mod media;
pub mod onboard;
pub mod settings;
pub mod theme;
pub mod ui;
pub mod zsapp;

use crate::input::Key;
use alloc::collections::VecDeque;
use alloc::string::String;
use core::sync::atomic::Ordering;
use spin::Mutex;

/// What the desktop tells an application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A key with the modifiers held (see `ui::SHIFT` ...).
    Key(Key, u8),
    MouseMove { x: i32, y: i32 },
    MouseLeave,
    /// A button went down (0 left, 1 right, 2 middle); `clicks` counts quick repeats.
    MouseDown { x: i32, y: i32, button: u8, clicks: u8 },
    MouseUp { x: i32, y: i32, button: u8 },
    Wheel { dy: i32 },
    /// Raw mouse movement, for a program that captured the mouse (Doom).
    MouseRaw { dx: i32, dy: i32, buttons: u8 },
    /// The window was resized: its surface now has this size.
    Resize { w: usize, h: usize },
    Focus(bool),
    /// A command from the menu bar: (menu, item) of the app's own menus.
    Menu(usize, usize),
    /// A file was dropped on the window.
    Drop(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Cursor {
    Arrow = 0,
    Hand = 1,
    Text = 2,
    ResizeH = 3,
    ResizeV = 4,
    ResizeD1 = 5,
    ResizeD2 = 6,
    Move = 7,
}

impl Cursor {
    pub fn from_u8(v: u8) -> Cursor {
        match v {
            1 => Cursor::Hand,
            2 => Cursor::Text,
            3 => Cursor::ResizeH,
            4 => Cursor::ResizeV,
            5 => Cursor::ResizeD1,
            6 => Cursor::ResizeD2,
            7 => Cursor::Move,
            _ => Cursor::Arrow,
        }
    }
}

/// Something an app asks the desktop to do.
pub enum Request {
    Open(apps::AppId, Option<String>),
    OpenFile(String),
    Notify(String, String),
    BuddySay(String),
    ShowDesktop,
}

static REQUESTS: Mutex<VecDeque<Request>> = Mutex::new(VecDeque::new());

pub fn request(r: Request) {
    REQUESTS.lock().push_back(r);
}

pub(crate) fn take_request() -> Option<Request> {
    REQUESTS.lock().pop_front()
}

/// Shows a notification banner.
pub fn notify(title: &str, body: &str) {
    request(Request::Notify(String::from(title), String::from(body)));
}

/// Opens a file with the app that handles its type.
pub fn open_file(path: &str) {
    request(Request::OpenFile(String::from(path)));
}

/// Called by the thread that owns a window when it sees a `Resize` event.
pub fn note_resize() {
    if let Some(ctx) = crate::sched::context() {
        ctx.resized.store(true, Ordering::SeqCst);
        if ctx.terminal.load(Ordering::Relaxed) {
            crate::graphics::console::CONSOLE.lock().relayout();
        }
    }
}

/// Whether the desktop is up.
pub fn active() -> bool {
    crate::graphics::ui::is_gui()
}

/// The argument the app was started with (a file to open), if any.
pub fn app_arg() -> Option<String> {
    crate::sched::context().and_then(|c| c.arg.lock().clone())
}

/// Starts the desktop on the calling thread.
pub fn run() {
    desktop::run();
}
