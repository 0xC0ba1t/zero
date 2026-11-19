//! The host that runs scripts on the real screen and keyboard.
//!
//! Scripts that only `print` write to the shell console like any command.
//! The first time a script draws (`cls`, `text` or `plot`) it takes over the
//! whole screen as an 80x25 grid of cells, scaled to the display. The cells
//! are drawn as pixels (see `graphics::vga`): a block character fills its
//! whole cell, a shade is a dither pattern, box lines meet in the middle.
//! When the script ends the shell console comes back.

use super::host::Host;
use crate::graphics::console::CONSOLE;
use crate::graphics::display::Display;
use crate::graphics::font::Font;
use crate::graphics::framebuffer::{self, Framebuffer};
use crate::graphics::vga::{self, BACKGROUND, PALETTE};
use crate::input::{self, Key};
use crate::{fb_println, time};
use alloc::string::String;
use alloc::vec::Vec;

const COLS: usize = 80;
const ROWS: usize = 25;

/// A grid cell: character in the low byte, palette color in the high byte.
type Cell = u16;
const BLANK: Cell = b' ' as Cell | 7 << 8;

pub struct KernelHost {
    saved_attr: u8,
    /// The color scripts draw with (palette index).
    color: u8,
    graphics: bool,
    cells: Vec<Cell>,
    /// Lines printed while the grid was showing; they are echoed to the shell afterwards.
    printed: Vec<String>,
    // grid geometry in pixels
    cell_w: usize,
    cell_h: usize,
    origin_x: usize,
    origin_y: usize,
    font: Font,
}

impl KernelHost {
    pub fn new() -> KernelHost {
        input::take_interrupt();
        KernelHost {
            saved_attr: CONSOLE.lock().attr(),
            color: 7,
            graphics: false,
            cells: alloc::vec![BLANK; COLS * ROWS],
            printed: Vec::new(),
            cell_w: 1,
            cell_h: 1,
            origin_x: 0,
            origin_y: 0,
            font: Font::console(),
        }
    }

    /// Takes over the screen for the cell grid (once).
    fn enter_graphics(&mut self) {
        if self.graphics {
            return;
        }
        let mut geometry = None;
        framebuffer::with(|fb| {
            let (w, h) = (fb.width(), fb.height());
            geometry = Some((w / COLS, h / ROWS, (w % COLS) / 2, (h % ROWS) / 2));
            fb.clear_to(BACKGROUND);
            fb.flush();
        });
        if let Some((cw, ch, ox, oy)) = geometry {
            self.cell_w = cw.max(1);
            self.cell_h = ch.max(1);
            self.origin_x = ox;
            self.origin_y = oy;
            self.font = Font::fit(self.cell_w, self.cell_h);
            self.graphics = true;
        }
    }

    fn paint(&self, fb: &mut Framebuffer, cx: usize, cy: usize) {
        let cell = self.cells[cy * COLS + cx];
        let fg = PALETTE[(cell >> 8) as usize & 15];
        vga::draw_cell(
            fb,
            self.origin_x + cx * self.cell_w,
            self.origin_y + cy * self.cell_h,
            self.cell_w,
            self.cell_h,
            cell as u8,
            fg,
            BACKGROUND,
            &self.font,
        );
    }

    fn set_cell(&mut self, x: i32, y: i32, ch: u8) {
        if x < 0 || y < 0 || x as usize >= COLS || y as usize >= ROWS {
            return;
        }
        let (x, y) = (x as usize, y as usize);
        self.cells[y * COLS + x] = ch as Cell | (self.color as Cell) << 8;
        framebuffer::with(|fb| self.paint(fb, x, y));
    }

    fn flush(&self) {
        if self.graphics {
            framebuffer::with(|fb| fb.flush());
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
        Key::Home => 262,
        Key::End => 263,
        Key::Delete => 264,
    }
}

impl Drop for KernelHost {
    /// Puts the screen back the way the shell expects it.
    fn drop(&mut self) {
        {
            let mut console = CONSOLE.lock();
            console.set_attr(self.saved_attr);
            if self.graphics {
                console.restore();
            }
        }
        // whatever the script printed on its own screen goes to the shell
        for line in &self.printed {
            fb_println!("{}", line);
        }
        input::take_interrupt();
    }
}

impl Host for KernelHost {
    fn clear(&mut self) {
        self.enter_graphics();
        for cell in self.cells.iter_mut() {
            *cell = BLANK;
        }
        framebuffer::with(|fb| {
            fb.clear_to(BACKGROUND);
            fb.flush();
        });
    }

    fn print(&mut self, text: &str) {
        if !self.graphics {
            fb_println!("{}", text);
            return;
        }
        // scroll the grid up one row and write the text on the last one
        self.cells.copy_within(COLS.., 0);
        let last = (ROWS - 1) * COLS;
        for cell in &mut self.cells[last..] {
            *cell = BLANK;
        }
        for (i, b) in text.bytes().take(COLS).enumerate() {
            let ch = if b.is_ascii() { b } else { b'?' };
            self.cells[last + i] = ch as Cell | (self.color as Cell) << 8;
        }
        self.printed.push(String::from(text));
        let this = &*self;
        framebuffer::with(|fb| {
            for cy in 0..ROWS {
                for cx in 0..COLS {
                    this.paint(fb, cx, cy);
                }
            }
            fb.flush();
        });
    }

    fn text(&mut self, x: i32, y: i32, text: &str) {
        self.enter_graphics();
        for (i, b) in text.bytes().enumerate() {
            self.set_cell(x + i as i32, y, if b.is_ascii() { b } else { b'?' });
        }
    }

    fn plot(&mut self, x: i32, y: i32, ch: u8) {
        self.enter_graphics();
        self.set_cell(x, y, ch);
    }

    fn set_color(&mut self, color: u8) {
        self.color = color & 0x0F;
        CONSOLE.lock().set_fg(self.color);
    }

    fn key(&mut self) -> i32 {
        self.flush();
        match input::try_read_key() {
            Some(key) => key_code(key),
            None => 0,
        }
    }

    fn wait_key(&mut self) -> i32 {
        self.flush();
        key_code(input::read_key())
    }

    fn sleep(&mut self, ms: u32) {
        self.flush();
        let target = time::uptime_ms() + ms as u64;
        while time::uptime_ms() < target && !input::interrupt_requested() {
            x86_64::instructions::hlt();
        }
    }

    fn ticks(&mut self) -> u64 {
        self.flush();
        time::uptime_ms()
    }

    fn interrupted(&mut self) -> bool {
        self.flush();
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
    let saved = CONSOLE.lock().attr();
    {
        let mut host = KernelHost::new();
        super::run("color 12\nprint 6 * 7", &mut host).unwrap();
    }
    // dropping the host restored the color the script had changed
    assert_eq!(CONSOLE.lock().attr(), saved);
}
