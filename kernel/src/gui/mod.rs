//! The desktop.
//!
//! A menu bar on top, a dock at the bottom and windows in between, in the
//! spirit of macOS (traffic-light buttons, a magnifying dock, rounded
//! windows with shadows) and Omarchy (keyboard shortcuts, window snapping,
//! optional automatic tiling).
//!
//! Every window is an application running in its own thread (see `sched`).
//! The application draws into the window's own surface as if it were the
//! whole screen; the desktop thread composes all the surfaces, draws the
//! frames, and passes keyboard and mouse input to the window in front.

pub mod apps;
pub mod browser;
pub mod desktop;
pub mod draw;
pub mod files;
pub mod settings;
pub mod widgets;

use crate::input::Key;
use core::sync::atomic::Ordering;

/// What the desktop tells an application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A key was pressed (while the window has the keyboard).
    Key(Key),
    /// The pointer moved over the window; coordinates are inside the window.
    MouseMove { x: i32, y: i32 },
    /// A button went down (0 left, 1 right, 2 middle).
    MouseDown { x: i32, y: i32, button: u8 },
    MouseUp { x: i32, y: i32, button: u8 },
    /// Raw mouse movement, for a program that grabbed the mouse (Doom).
    MouseRaw { dx: i32, dy: i32, buttons: u8 },
    /// The window was resized: its surface now has this size.
    Resize { w: usize, h: usize },
}

/// Called by the thread that owns a window when it sees a `Resize` event.
pub fn note_resize() {
    if let Some(ctx) = crate::sched::context() {
        ctx.resized.store(true, Ordering::SeqCst);
        if ctx.terminal.load(Ordering::Relaxed) {
            // the text console lays itself out again for the new size
            crate::graphics::console::CONSOLE.lock().relayout();
        }
    }
}

/// Whether the desktop is up.
pub fn active() -> bool {
    crate::graphics::ui::is_gui()
}

/// Starts the desktop on the calling thread; returns when it is shut down.
pub fn run() {
    desktop::run();
}
