//! The host that runs scripts on the real screen and keyboard.

use super::host::Host;
use crate::input::{self, Key};
use crate::{fb_println, time};

/// Renders directly to the framebuffer and reads the keyboard queue.
pub struct KernelHost {
    saved_fg: u32,
    saved_bg: u32,
}

impl KernelHost {
    pub fn new() -> KernelHost {
        input::take_interrupt();
        let (fg, bg) = {
            let console = crate::graphics::framebuffer::CONSOLE.lock();
            (console.fg, console.bg)
        };
        KernelHost {
            saved_fg: fg,
            saved_bg: bg,
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
        let mut console = crate::graphics::framebuffer::CONSOLE.lock();
        console.set_fg_bg(self.saved_fg, self.saved_bg);
        console.clear();
        input::take_interrupt();
    }
}

impl Host for KernelHost {
    fn clear(&mut self) {
        let mut console = crate::graphics::framebuffer::CONSOLE.lock();
        console.clear();
    }

    fn print(&mut self, text: &str) {
        fb_println!("{}", text);
    }

    fn text(&mut self, x: i32, y: i32, text: &str) {
        if x < 0 || y < 0 { return; }
        let mut console = crate::graphics::framebuffer::CONSOLE.lock();
        console.write_at(x as usize, y as usize, text);
    }

    fn plot(&mut self, x: i32, y: i32, ch: u8) {
        if x < 0 || y < 0 { return; }
        let mut console = crate::graphics::framebuffer::CONSOLE.lock();
        console.write_byte_at(x as usize, y as usize, ch);
    }

    fn set_color(&mut self, color: u8) {
        let mut console = crate::graphics::framebuffer::CONSOLE.lock();
        let rgb = color_index_to_rgb(color);
        console.set_fg_bg(rgb, 0x000000);
    }

    fn key(&mut self) -> i32 {
        if let Some(key) = input::try_read_key() {
            key_code(key)
        } else {
            0
        }
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

fn color_index_to_rgb(index: u8) -> u32 {
    const COLORS: [u32; 16] = [
        0x000000, // Black
        0x0000AA, // Blue
        0x00AA00, // Green
        0x00AAAA, // Cyan
        0xAA0000, // Red
        0xAA00AA, // Magenta
        0xAA5500, // Brown
        0xAAAAAA, // LightGray
        0x555555, // DarkGray
        0x5555FF, // LightBlue
        0x55FF55, // LightGreen
        0x55FFFF, // LightCyan
        0xFF5555, // LightRed
        0xFF55FF, // Pink
        0xFFFF55, // Yellow
        0xFFFFFF, // White
    ];
    COLORS[(index & 0x0F) as usize]
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
    let saved = {
        let console = crate::graphics::framebuffer::CONSOLE.lock();
        console.fg
    };
    {
        let mut host = KernelHost::new();
        super::run("color 12\nprint 6 * 7", &mut host).unwrap();
    }
    // dropping the host restored the color the script had changed
    assert_eq!(crate::graphics::framebuffer::CONSOLE.lock().fg, saved);
}
