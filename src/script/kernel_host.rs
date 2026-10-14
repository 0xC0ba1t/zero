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
        0
    }

    fn wait_key(&mut self) -> i32 {
        0
    }

    fn sleep(&mut self, _ms: u32) {}

    fn ticks(&mut self) -> u64 {
        time::uptime_ms()
    }

    fn interrupted(&mut self) -> bool {
        input::interrupt_requested()
    }
}
