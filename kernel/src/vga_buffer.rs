//! Legacy module name retained for minimal import churn.
//!
//! All output now goes directly to the framebuffer via `graphics::framebuffer`.
//! These thin wrappers exist only so that existing call sites that reference
//! `crate::vga_buffer` continue to compile while pointing at the framebuffer
//! console.

pub use crate::graphics::framebuffer::CONSOLE;

/// Clears the framebuffer screen.
pub fn clear_screen() {
    let mut console = crate::graphics::framebuffer::CONSOLE.lock();
    console.clear();
}

/// Moves the cursor up by one page.
pub fn scroll_up() {
    let mut console = crate::graphics::framebuffer::CONSOLE.lock();
    console.scroll_up();
}

/// Moves the cursor down by one page.
pub fn scroll_down() {
    let mut console = crate::graphics::framebuffer::CONSOLE.lock();
    console.scroll_down();
}
