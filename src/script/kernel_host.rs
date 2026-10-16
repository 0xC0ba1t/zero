//! The host that runs scripts on the real screen and keyboard.

use super::host::Host;
use crate::input::{self, Key};
use crate::vga_buffer::{self, Color};
use crate::{println, time};

/// Draws on the VGA text screen and reads the keyboard queue.
pub struct KernelHost {
    saved_color: u8,
    /// true once the script has drawn at a fixed position
    graphics: bool,
}

impl KernelHost {
    pub fn new() -> KernelHost {
        input::take_interrupt();
        KernelHost {
            saved_color: vga_buffer::color_code(),
            graphics: false,
        }
    }

    /// Hides the hardware cursor the first time a script draws something.
    fn enter_graphics(&mut self) {
        if !self.graphics {
            self.graphics = true;
            vga_buffer::set_cursor_visible(false);
        }
    }
}

/// The number a script sees for a key press.
fn key_code(key: Key) -> i32 {
    match key {
        Key::Char(c) => c as i32,
        Key::Up => 256,
        Key::Down => 257,
        Key::Left => 258,
        Key::Right => 259,
        Key::PageUp => 260,
        Key::PageDown => 261,
    }
}

impl Drop for KernelHost {
    /// Puts the screen back the way the shell expects it.
    fn drop(&mut self) {
        vga_buffer::set_color_code(self.saved_color);
        vga_buffer::set_cursor_visible(true);
        input::take_interrupt();
    }
}

impl Host for KernelHost {
    fn clear(&mut self) {
        self.enter_graphics();
        vga_buffer::clear_screen();
    }

    fn print(&mut self, text: &str) {
        println!("{}", text);
    }

    fn text(&mut self, x: i32, y: i32, text: &str) {
        self.enter_graphics();
        vga_buffer::write_at(x, y, text);
    }

    fn plot(&mut self, x: i32, y: i32, ch: u8) {
        self.enter_graphics();
        vga_buffer::put_at(x, y, ch);
    }

    fn set_color(&mut self, color: u8) {
        vga_buffer::set_color(Color::from_index(color), Color::Black);
    }

    fn key(&mut self) -> i32 {
        // drain the queue so a held key can't build up a backlog; the last key wins
        let mut last = 0;
        while let Some(key) = input::try_read_key() {
            last = key_code(key);
        }
        last
    }

    fn wait_key(&mut self) -> i32 {
        key_code(input::read_key())
    }

    fn sleep(&mut self, ms: u32) {
        let target = time::uptime_ms() + ms as u64;
        while time::uptime_ms() < target && !input::interrupt_requested() {
            x86_64::instructions::hlt();
        }
    }

    fn ticks(&mut self) -> u64 {
        time::uptime_ms()
    }

    fn interrupted(&mut self) -> bool {
        input::interrupt_requested()
    }
}

#[test_case]
fn test_key_codes() {
    assert_eq!(key_code(Key::Char('q')), 113);
    assert_eq!(key_code(Key::Char('\n')), 10);
    assert_eq!(key_code(Key::Up), 256);
    assert_eq!(key_code(Key::Down), 257);
}

#[test_case]
fn test_script_runs_on_the_kernel_host() {
    let saved = vga_buffer::color_code();
    {
        let mut host = KernelHost::new();
        super::run("color 12\nprint 6 * 7", &mut host).unwrap();
    }
    // dropping the host restored the color the script had changed
    assert_eq!(vga_buffer::color_code(), saved);
}
