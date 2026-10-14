//! The interface between the interpreter and the outside world.

/// Everything a script can do to the machine it runs on.
///
/// The kernel implements this on top of the screen and keyboard; tests use a
/// fake that records what happened.
pub trait Host {
    /// Clears the screen.
    fn clear(&mut self);
    /// Prints a line of text at the bottom of the screen, scrolling up.
    fn print(&mut self, text: &str);
    /// Draws text at a position, without scrolling.
    fn text(&mut self, x: i32, y: i32, text: &str);
    /// Draws one character (a code page 437 byte) at a position.
    fn plot(&mut self, x: i32, y: i32, ch: u8);
    /// Sets the text color (a VGA palette index, 0 to 15).
    fn set_color(&mut self, color: u8);
    /// Returns the most recent key pressed since the last call, or 0.
    fn key(&mut self) -> i32;
    /// Waits for a key press and returns it.
    fn wait_key(&mut self) -> i32;
    /// Pauses for the given number of milliseconds.
    fn sleep(&mut self, ms: u32);
    /// Milliseconds since boot.
    fn ticks(&mut self) -> u64;
    /// Returns true if the user asked to stop the script (ctrl+c).
    fn interrupted(&mut self) -> bool;
}
