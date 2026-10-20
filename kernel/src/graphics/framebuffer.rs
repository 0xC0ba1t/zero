use crate::graphics::display::Display;
use bootloader_api::info::FrameBufferInfo;
use spin::Mutex;

pub static FRAMEBUFFER: Mutex<Option<Framebuffer>> = Mutex::new(None);

pub struct Framebuffer {
    pub info: FrameBufferInfo,
    pub buffer: &'static mut [u8],
}

impl Framebuffer {
    pub fn new(info: FrameBufferInfo, buffer: &'static mut [u8]) -> Self {
        Self { info, buffer }
    }
}

impl Display for Framebuffer {
    fn put_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x < self.info.width && y < self.info.height {
            let pixel_offset = y * self.info.stride + x;
            let bytes_per_pixel = self.info.bytes_per_pixel;
            let byte_offset = pixel_offset * bytes_per_pixel;
            if byte_offset + bytes_per_pixel <= self.buffer.len() {
                let color_bytes = color.to_le_bytes();
                self.buffer[byte_offset..byte_offset + bytes_per_pixel]
                    .copy_from_slice(&color_bytes[..bytes_per_pixel]);
            }
        }
    }

    fn clear(&mut self) {
        self.buffer.fill(0);
    }

    fn width(&self) -> usize {
        self.info.width
    }

    fn height(&self) -> usize {
        self.info.height
    }
}

/// A simple text console that renders to the framebuffer using an 8x8 font.
pub struct FbConsole {
    /// Current cursor position in character cells.
    col: usize,
    row: usize,
    /// Foreground and background colors (u32 framebuffer format).
    pub fg: u32,
    pub bg: u32,
    /// Whether the console has been initialized.
    initialized: bool,
    /// Number of visible text columns.
    cols: usize,
    /// Number of visible text rows.
    rows: usize,
    /// Scrollback: each entry is one row of characters.
    scrollback: alloc::vec::Vec<alloc::vec::Vec<char>>,
}

lazy_static::lazy_static! {
    pub static ref CONSOLE: Mutex<FbConsole> = Mutex::new(FbConsole {
        col: 0,
        row: 0,
        fg: 0xFFFFFF,
        bg: 0x000000,
        initialized: false,
        cols: 0,
        rows: 0,
        scrollback: alloc::vec::Vec::new(),
    });
}

const CHAR_W: usize = crate::graphics::font::CHAR_WIDTH + 2;
const CHAR_H: usize = crate::graphics::font::CHAR_HEIGHT + 2;

impl FbConsole {
    pub fn init(&mut self) {
        let cols;
        let rows;
        {
            let mut fb_guard = FRAMEBUFFER.lock();
            if let Some(ref mut fb) = *fb_guard {
                fb.clear();
                cols = fb.width() / CHAR_W;
                rows = fb.height() / CHAR_H;
            } else {
                cols = 80;
                rows = 30;
            }
        }
        self.cols = cols;
        self.rows = rows;
        self.col = 0;
        self.row = 0;
        self.scrollback.clear();
        self.initialized = true;
    }

    pub fn clear(&mut self) {
        let (cols, rows) = {
            let mut fb_guard = FRAMEBUFFER.lock();
            if let Some(ref mut fb) = *fb_guard {
                fb.clear();
                (fb.width() / CHAR_W, fb.height() / CHAR_H)
            } else {
                (80, 30)
            }
        };
        self.col = 0;
        self.row = 0;
        self.scrollback.clear();
        if self.cols == 0 {
            self.cols = cols;
            self.rows = rows;
        }
    }

    pub fn set_fg_bg(&mut self, fg: u32, bg: u32) {
        self.fg = fg;
        self.bg = bg;
    }

    pub fn color_from_name(name: &str) -> Option<u32> {
        let lower = name.to_lowercase();
        let c = match lower.as_str() {
            "black" => 0x000000,
            "blue" => 0x0000AA,
            "green" => 0x00AA00,
            "cyan" => 0x00AAAA,
            "red" => 0xAA0000,
            "magenta" => 0xAA00AA,
            "brown" => 0xAA5500,
            "lightgray" => 0xAAAAAA,
            "darkgray" => 0x555555,
            "lightblue" => 0x5555FF,
            "lightgreen" => 0x55FF55,
            "lightcyan" => 0x55FFFF,
            "lightred" => 0xFF5555,
            "pink" => 0xFF55FF,
            "yellow" => 0xFFFF55,
            "white" => 0xFFFFFF,
            _ => return None,
        };
        Some(c)
    }

    pub fn scroll_up(&mut self) {
        // Scroll the visible window up by one row in the scrolback buffer
        if self.scrollback.len() > self.rows {
            self.scrollback.drain(0..1);
        }
        self.redraw();
    }

    pub fn scroll_down(&mut self) {
        // For simplicity, just redraw current view
        self.redraw();
    }

    /// Write a text string at a specific (col, row) position in the text grid.
    pub fn write_at(&mut self, col: usize, row: usize, text: &str) {
        if !self.initialized {
            return;
        }
        let mut fb_guard = FRAMEBUFFER.lock();
        if let Some(ref mut fb) = *fb_guard {
            for (i, byte) in text.bytes().enumerate() {
                let c = col + i;
                if c >= self.cols {
                    break;
                }
                let px = c * CHAR_W;
                let py = row * CHAR_H;
                crate::graphics::font::draw_char(fb, px, py, byte, self.fg, self.bg);
            }
        }
    }

    /// Redraw the entire visible region from scrollback.
    fn redraw(&mut self) {
        let mut fb_guard = FRAMEBUFFER.lock();
        if let Some(ref mut fb) = *fb_guard {
            fb.clear();
        }
        let start = if self.scrollback.len() > self.rows {
            self.scrollback.len() - self.rows
        } else {
            0
        };
        for (screen_row, entry) in self.scrollback.iter().enumerate().skip(start) {
            if screen_row - start >= self.rows {
                break;
            }
            for (col, ch) in entry.iter().enumerate() {
                if col >= self.cols {
                    break;
                }
                let px = col * CHAR_W;
                let py = (screen_row - start) * CHAR_H;
                if let Some(ref mut fb) = *fb_guard {
                    crate::graphics::font::draw_char(fb, px, py, *ch as u8, self.fg, self.bg);
                }
            }
        }
    }

    pub fn write_byte(&mut self, byte: u8) {
        if !self.initialized {
            return;
        }

        match byte {
            b'\n' => {
                self.col = 0;
                self.row += 1;
            }
            b'\r' => {
                self.col = 0;
            }
            b'\t' => {
                self.col = (self.col + 8) & !7;
            }
            byte => {
                if self.col >= self.cols {
                    self.col = 0;
                    self.row += 1;
                }
                if self.row >= self.rows {
                    // Scroll: drop oldest scrollback row
                    self.scrollback.drain(0..1);
                    self.row = self.rows - 1;
                }
                // Ensure scrollback has enough rows
                while self.scrollback.len() <= self.row {
                    let mut line = alloc::vec::Vec::new();
                    for _ in 0..self.cols {
                        line.push(' ');
                    }
                    self.scrollback.push(line);
                }
                // Update scrollback: write the character at (col, row)
                let col = self.col;
                let row = self.row;
                {
                    let row_vec = &mut self.scrollback[row];
                    if col < row_vec.len() {
                        let bytes = row_vec.as_mut_slice();
                        bytes[col] = byte as char;
                    }
                }
                // Draw to framebuffer
                let px = col * CHAR_W;
                let py = row * CHAR_H;
                let fg = self.fg;
                let bg = self.bg;
                let mut fb_guard = FRAMEBUFFER.lock();
                if let Some(ref mut fb) = *fb_guard {
                    crate::graphics::font::draw_char(fb, px, py, byte, fg, bg);
                }
                self.col += 1;
            }
        }
    }

    pub fn write_str(&mut self, s: &str) {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
    }
}

impl core::fmt::Write for FbConsole {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write_str(s);
        Ok(())
    }
}

/// Print to the framebuffer console.
#[macro_export]
macro_rules! fb_print {
    ($($arg:tt)*) => {
        $crate::graphics::framebuffer::_fb_print(format_args!($($arg)*))
    };
}

/// Print to the framebuffer console, appending a newline.
#[macro_export]
macro_rules! fb_println {
    () => ($crate::fb_print!("\n"));
    ($fmt:expr) => ($crate::fb_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::fb_print!(
        concat!($fmt, "\n"), $($arg)*));
}

#[doc(hidden)]
pub fn _fb_print(args: core::fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        CONSOLE.lock().write_fmt(args).unwrap();
    });
}
