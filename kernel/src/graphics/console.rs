//! The text console the shell lives on.
//!
//! The console owns a grid of character cells plus a long scrollback. A write
//! updates the cells and paints only what changed; scrolling moves pixels
//! with a single memory copy instead of repainting the screen, and the whole
//! batch is pushed to the display once at the end. Looking at older output
//! (page up / page down) repaints from the stored cells.
//!
//! Colors can be changed from anywhere with ANSI escapes, e.g.
//! `fb_print!("\x1b[96mhello\x1b[0m")`.

use crate::graphics::display::Display;
use crate::graphics::font::Font;
use crate::graphics::framebuffer::{self, Framebuffer, FRAMEBUFFER};
use crate::graphics::ui;
use crate::graphics::vga::{self, BACKGROUND, PALETTE};
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::fmt;
use spin::Mutex;

pub static CONSOLE: Mutex<FbConsole> = Mutex::new(FbConsole::new());

/// One character cell: the character in the low byte, the colors in the high
/// byte (foreground in the low nibble, background in the high nibble).
type Cell = u16;

/// Light gray on black.
pub const DEFAULT_ATTR: u8 = 0x07;
const BLANK: Cell = b' ' as u16 | (DEFAULT_ATTR as u16) << 8;

/// How many lines of history are kept.
const MAX_LINES: usize = 2000;

/// ANSI color numbers (0 to 7) to VGA palette indexes.
const ANSI_TO_VGA: [u8; 8] = [0, 4, 2, 6, 1, 5, 3, 7];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Esc {
    None,
    /// Saw ESC.
    Escape,
    /// Inside `ESC [` collecting numbers.
    Csi,
}

pub struct FbConsole {
    initialized: bool,
    font: Font,
    cell_w: usize,
    cell_h: usize,
    /// Left padding in pixels, top of the console window, its height, and where the first row starts.
    pad_x: usize,
    area_y: usize,
    area_h: usize,
    oy: usize,
    cols: usize,
    rows: usize,
    /// All lines, oldest first. The last `rows` of them are the live screen.
    lines: VecDeque<Vec<Cell>>,
    /// Cursor position within the live screen.
    col: usize,
    row: usize,
    attr: u8,
    /// The color `ESC[0m` resets to (changed by the `color` command).
    default_attr: u8,
    /// How many lines the view is scrolled back from the live screen.
    view_back: usize,
    /// Where the cursor is currently painted, if anywhere.
    cursor_drawn: Option<(usize, usize)>,
    show_cursor: bool,
    esc: Esc,
    params: [u16; 4],
    nparams: usize,
    /// Rendered glyphs (character and colors to ready-to-copy pixels), made on first use.
    tiles: RefCell<Vec<Option<Vec<u8>>>>,
    /// When the screen last scrolled (milliseconds), to animate only slow output.
    last_scroll: u64,
    /// Rows the cursor moved because the window was resized (see `take_row_shift`).
    shift: isize,
}

impl FbConsole {
    pub const fn new() -> FbConsole {
        FbConsole {
            initialized: false,
            font: Font::console(),
            cell_w: 1,
            cell_h: 1,
            pad_x: 0,
            area_y: 0,
            area_h: 0,
            oy: 0,
            cols: 0,
            rows: 0,
            lines: VecDeque::new(),
            col: 0,
            row: 0,
            attr: DEFAULT_ATTR,
            default_attr: DEFAULT_ATTR,
            view_back: 0,
            cursor_drawn: None,
            show_cursor: true,
            esc: Esc::None,
            params: [0; 4],
            nparams: 0,
            tiles: RefCell::new(Vec::new()),
            last_scroll: 0,
            shift: 0,
        }
    }

    // ----- public API (locks the framebuffer itself) -----

    /// Sizes the console to the framebuffer and clears the screen.
    /// Works out the cell grid for the current surface size.
    fn fit_to(&mut self, fb: &Framebuffer) {
        // the console lives in the window between the title bar and the status bar
        let lay = ui::layout(fb.width(), fb.height());
        self.font = Font::console_for_width(fb.width());
        self.cell_w = self.font.width();
        self.cell_h = self.font.height() + 2;
        self.pad_x = 14.min(fb.width() / 20);
        self.area_y = lay.body_y();
        self.area_h = lay.body_h();
        self.oy = self.area_y + 6;
        self.cols = (fb.width().saturating_sub(2 * self.pad_x) / self.cell_w).max(1);
        self.rows = (self.area_h.saturating_sub(6) / self.cell_h).max(1);
    }

    /// The window changed size: lay the text out again, keeping the history.
    pub fn relayout(&mut self) {
        if !self.initialized {
            return self.init();
        }
        framebuffer::with(|fb| {
            let old_rows = self.rows;
            self.fit_to(fb);
            for line in self.lines.iter_mut() {
                line.resize(self.cols, BLANK);
            }
            let rows = self.rows;
            if rows < old_rows {
                // lose the empty rows below the cursor first, then scroll the top into the history
                let delta = old_rows - rows;
                let below = old_rows - 1 - self.row.min(old_rows - 1);
                let from_bottom = delta.min(below);
                for _ in 0..from_bottom {
                    self.lines.pop_back();
                }
                let from_top = delta - from_bottom;
                self.row = self.row.saturating_sub(from_top);
                self.shift -= from_top as isize;
            }
            while self.lines.len() < rows {
                let line = self.blank_line();
                self.lines.push_back(line);
            }
            self.row = self.row.min(rows - 1);
            self.col = self.col.min(self.cols.saturating_sub(1));
            self.view_back = 0;
            self.cursor_drawn = None;
            self.tiles.borrow_mut().clear();
            self.clear_area(fb);
            self.redraw(fb);
            self.finish(fb);
        });
    }

    /// How far the cursor row moved because of resizes since the last call.
    pub fn take_row_shift(&mut self) -> isize {
        core::mem::take(&mut self.shift)
    }

    pub fn init(&mut self) {
        framebuffer::with(|fb| {
            self.fit_to(fb);
            self.initialized = true;
            self.tiles.borrow_mut().clear();
            self.clear_fb(fb);
            self.finish(fb);
        });
    }

    pub fn clear(&mut self) {
        if !self.initialized {
            return;
        }
        framebuffer::with(|fb| {
            self.clear_fb(fb);
            self.finish(fb);
        });
    }

    /// Repaints the whole console (e.g. after a script took over the screen).
    pub fn restore(&mut self) {
        if !self.initialized {
            return;
        }
        framebuffer::with(|fb| {
            self.cursor_drawn = None;
            self.redraw(fb);
            self.finish(fb);
        });
    }

    pub fn write_str(&mut self, s: &str) {
        if !self.initialized {
            return;
        }
        framebuffer::with(|fb| {
            self.begin(fb);
            let mut out = Out { con: self, fb };
            let _ = fmt::Write::write_str(&mut out, s);
            let Out { con, fb } = out;
            con.finish(fb);
        });
    }

    /// Size of the live screen in characters.
    pub fn size(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    /// Where the next character will go.
    pub fn pos(&self) -> (usize, usize) {
        (self.col, self.row)
    }

    pub fn attr(&self) -> u8 {
        self.attr
    }

    pub fn set_attr(&mut self, attr: u8) {
        self.attr = attr;
    }

    /// Makes `index` the color text goes back to after `ESC[0m`, and uses it now.
    pub fn set_default_fg(&mut self, index: u8) {
        self.default_attr = (self.default_attr & 0xF0) | (index & 0x0F);
        self.attr = (self.attr & 0xF0) | (index & 0x0F);
    }

    /// Sets the foreground color to a palette index, keeping the background.
    pub fn set_fg(&mut self, index: u8) {
        self.attr = (self.attr & 0xF0) | (index & 0x0F);
    }

    /// Scrolls the view into the history: positive goes back in time.
    pub fn scroll_view(&mut self, delta: isize) {
        if !self.initialized {
            return;
        }
        let max_back = self.lines.len().saturating_sub(self.rows);
        let wanted = (self.view_back as isize + delta).clamp(0, max_back as isize) as usize;
        if wanted == self.view_back {
            return;
        }
        framebuffer::with(|fb| {
            self.erase_cursor(fb);
            let diff = wanted as isize - self.view_back as isize;
            if diff.abs() <= 60 {
                self.animate_view(fb, diff);
            }
            self.view_back = wanted;
            self.redraw(fb);
            self.finish(fb);
        });
    }

    /// Rewrites an input line that starts at `(col, row)`: draws `text`, wipes
    /// `erase_extra` leftover cells after it and puts the cursor at character
    /// `cursor` of the text. Everything is painted with one flush.
    pub fn redraw_input(&mut self, col: usize, row: usize, text: &str, erase_extra: usize, cursor: usize) {
        if !self.initialized {
            return;
        }
        framebuffer::with(|fb| {
            self.begin(fb);
            self.col = col;
            self.row = row.min(self.rows - 1);
            {
                let mut out = Out { con: self, fb };
                let _ = fmt::Write::write_str(&mut out, text);
                for _ in 0..erase_extra {
                    out.put(b' ');
                }
            }
            self.col = (col + cursor).min(self.cols - 1);
            self.finish(fb);
        });
    }

    // ----- everything below works on an already locked framebuffer -----

    /// Called before output: takes the cursor off the screen and returns to the live view.
    fn begin(&mut self, fb: &mut Framebuffer) {
        self.erase_cursor(fb);
        if self.view_back != 0 {
            self.view_back = 0;
            self.redraw(fb);
        }
    }

    /// Called after output: draws the cursor and pushes the pixels to the screen.
    fn finish(&mut self, fb: &mut Framebuffer) {
        if self.show_cursor && self.view_back == 0 && self.cursor_drawn.is_none() {
            let col = self.col.min(self.cols - 1);
            self.draw_cell_at(fb, col, self.row, true);
            self.cursor_drawn = Some((col, self.row));
        }
        fb.flush();
    }

    fn erase_cursor(&mut self, fb: &mut Framebuffer) {
        if let Some((col, row)) = self.cursor_drawn.take() {
            self.draw_cell_at(fb, col, row, false);
        }
    }

    fn base(&self) -> usize {
        self.lines.len() - self.rows
    }

    fn blank_line(&self) -> Vec<Cell> {
        alloc::vec![BLANK; self.cols]
    }

    fn clear_fb(&mut self, fb: &mut Framebuffer) {
        self.lines.clear();
        for _ in 0..self.rows {
            let line = self.blank_line();
            self.lines.push_back(line);
        }
        self.col = 0;
        self.row = 0;
        self.view_back = 0;
        self.cursor_drawn = None;
        self.esc = Esc::None;
        self.clear_area(fb);
    }

    /// Paints the console's whole window area with the background color.
    fn clear_area(&self, fb: &mut Framebuffer) {
        let w = fb.width();
        fb.fill_rect(0, self.area_y, w, self.area_h, BACKGROUND);
    }

    /// Paints one cell of the live screen.
    fn draw_cell_at(&self, fb: &mut Framebuffer, col: usize, row: usize, inverse: bool) {
        let cell = self.lines[self.base() + row][col];
        self.paint(fb, col, row, cell, inverse);
    }

    fn paint(&self, fb: &mut Framebuffer, col: usize, row: usize, cell: Cell, inverse: bool) {
        let x = self.pad_x + col * self.cell_w;
        let y = self.oy + row * self.cell_h;
        self.paint_at(fb, x, y, cell, inverse);
    }

    /// Paints a whole stored line with its top at pixel row `y`.
    fn paint_line_at(&self, fb: &mut Framebuffer, index: usize, y: usize) {
        for (c, &cell) in self.lines[index].iter().enumerate() {
            if cell != BLANK {
                self.paint_at(fb, self.pad_x + c * self.cell_w, y, cell, false);
            }
        }
    }

    fn paint_at(&self, fb: &mut Framebuffer, x: usize, y: usize, cell: Cell, inverse: bool) {
        let ch = cell as u8;
        let attr = (cell >> 8) as u8;
        let (fg, bg) = (attr & 0x0F, attr >> 4);
        let (mut fg, mut bg) = (PALETTE[fg as usize], PALETTE[bg as usize]);
        if inverse {
            // the cursor: a block in the accent color
            fg = BACKGROUND;
            bg = ui::ACCENT;
        }
        match ch {
            0 | b' ' => fb.fill_rect(x, y, self.cell_w, self.cell_h, bg),
            0x21..=0x7E if !inverse => {
                // a finished glyph for this character and color, copied row by row
                let key = (ch as usize - 0x21) * 256 + attr as usize;
                let mut tiles = self.tiles.borrow_mut();
                if tiles.is_empty() {
                    tiles.resize(94 * 256, None);
                }
                if tiles[key].is_none() {
                    tiles[key] = Some(fb.make_tile(self.cell_w, self.cell_h, 1, self.font.raster(ch as char), fg, bg));
                }
                if let Some(tile) = tiles[key].as_ref() {
                    fb.blit_tile(x, y, self.cell_w, self.cell_h, tile);
                }
            }
            0x21..=0x7E => {
                fb.fill_rect(x, y, self.cell_w, 1, bg);
                fb.fill_rect(x, y + self.cell_h - 1, self.cell_w, 1, bg);
                self.font.draw_char(fb, x, y + 1, ch as char, fg, bg);
            }
            _ => vga::draw_cell(fb, x, y, self.cell_w, self.cell_h, ch, fg, bg, &self.font),
        }
    }

    /// Slides the view by `diff` lines (positive: towards older output), one
    /// line at a time in a few pixel steps. The line coming into view is painted
    /// clipped to the console window, so it appears to slide in from the edge.
    fn animate_view(&mut self, fb: &mut Framebuffer, diff: isize) {
        let region = self.rows * self.cell_h;
        let width = fb.width();
        fb.set_clip(0, self.oy, width, region);
        let step = (self.cell_h + 2) / 3;
        let mut back = self.view_back;
        for _ in 0..diff.unsigned_abs() {
            let first = self.lines.len() - self.rows - back;
            let mut moved = 0;
            while moved < self.cell_h {
                let d = step.min(self.cell_h - moved);
                moved += d;
                if diff > 0 {
                    fb.scroll_rect_down(self.oy, region, d, BACKGROUND);
                    if self.oy + moved >= self.cell_h {
                        self.paint_line_at(fb, first - 1, self.oy + moved - self.cell_h);
                    }
                } else {
                    fb.scroll_rect_up(self.oy, region, d, BACKGROUND);
                    self.paint_line_at(fb, first + self.rows, self.oy + region - moved);
                }
                fb.flush();
                pause_ms(2);
            }
            back = if diff > 0 { back + 1 } else { back - 1 };
        }
        fb.clear_clip();
    }

    /// Repaints the whole screen from the stored cells.
    fn redraw(&mut self, fb: &mut Framebuffer) {
        let first = self.lines.len() - self.rows - self.view_back;
        self.clear_area(fb);
        for r in 0..self.rows {
            let line = &self.lines[first + r];
            let last = match line.iter().rposition(|&c| c != BLANK) {
                Some(last) => last,
                None => continue,
            };
            for (c, &cell) in line.iter().enumerate().take(last + 1) {
                if cell != BLANK {
                    self.paint(fb, c, r, cell, false);
                }
            }
        }
        self.cursor_drawn = None;
    }

    fn scroll(&mut self, fb: &mut Framebuffer) {
        let line = self.blank_line();
        self.lines.push_back(line);
        if self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
        // Output that trickles in (a line after pressing enter) scrolls smoothly,
        // in a few steps; a burst of lines just jumps so it stays fast.
        let now = crate::time::uptime_ms();
        let region = self.rows * self.cell_h;
        if now.saturating_sub(self.last_scroll) > 150 {
            let step = (self.cell_h + 3) / 4;
            let mut moved = 0;
            while moved < self.cell_h {
                let d = step.min(self.cell_h - moved);
                fb.scroll_rect_up(self.oy, region, d, BACKGROUND);
                fb.flush();
                moved += d;
                pause_ms(3);
            }
        } else {
            fb.scroll_rect_up(self.oy, region, self.cell_h, BACKGROUND);
        }
        self.last_scroll = crate::time::uptime_ms();
    }

    fn newline(&mut self, fb: &mut Framebuffer) {
        self.col = 0;
        if self.row + 1 < self.rows {
            self.row += 1;
        } else {
            self.scroll(fb);
        }
    }

    /// Writes one character cell at the cursor and advances.
    fn put_cell(&mut self, fb: &mut Framebuffer, ch: u8) {
        if self.col >= self.cols {
            self.newline(fb);
        }
        let cell = ch as Cell | (self.attr as Cell) << 8;
        let (col, row) = (self.col, self.row);
        let base = self.base();
        self.lines[base + row][col] = cell;
        self.paint(fb, col, row, cell, false);
        self.col += 1;
    }

    /// Handles one byte of output: control characters, escapes and printable characters.
    fn put_byte(&mut self, fb: &mut Framebuffer, b: u8) {
        match self.esc {
            Esc::Escape => {
                if b == b'[' {
                    self.esc = Esc::Csi;
                    self.params = [0; 4];
                    self.nparams = 0;
                } else {
                    self.esc = Esc::None;
                }
                return;
            }
            Esc::Csi => {
                match b {
                    b'0'..=b'9' => {
                        if self.nparams == 0 {
                            self.nparams = 1;
                        }
                        let p = &mut self.params[self.nparams - 1];
                        *p = p.saturating_mul(10).saturating_add((b - b'0') as u16);
                    }
                    b';' => {
                        if self.nparams == 0 {
                            self.nparams = 1;
                        }
                        if self.nparams < self.params.len() {
                            self.nparams += 1;
                        }
                    }
                    _ => {
                        self.esc = Esc::None;
                        self.csi(fb, b);
                    }
                }
                return;
            }
            Esc::None => {}
        }

        match b {
            0x1B => self.esc = Esc::Escape,
            b'\n' => self.newline(fb),
            b'\r' => self.col = 0,
            b'\t' => {
                let stop = (self.col + 8) & !7;
                while self.col < stop.min(self.cols) {
                    self.put_cell(fb, b' ');
                }
            }
            0x08 | 0x7F => {
                if self.col > 0 {
                    self.col = self.col.min(self.cols) - 1;
                    let (col, row) = (self.col, self.row);
                    let base = self.base();
                    self.lines[base + row][col] = BLANK;
                    self.paint(fb, col, row, BLANK, false);
                }
            }
            _ => self.put_cell(fb, b),
        }
    }

    /// Executes a finished `ESC [ ... <final>` sequence.
    fn csi(&mut self, fb: &mut Framebuffer, final_byte: u8) {
        match final_byte {
            b'm' => {
                if self.nparams == 0 {
                    self.attr = self.default_attr;
                }
                let mut i = 0;
                while i < self.nparams {
                    let code = self.params[i];
                    i += 1;
                    match code {
                        0 => self.attr = self.default_attr,
                        1 => self.attr |= 0x08, // bold: bright foreground
                        22 => self.attr &= !0x08,
                        30..=37 => {
                            self.attr = (self.attr & 0xF0) | ANSI_TO_VGA[(code - 30) as usize]
                        }
                        // 38;5;N and 48;5;N pick VGA palette entry N (0 to 15) directly
                        38 | 48 => {
                            if i + 1 < self.nparams && self.params[i] == 5 {
                                let n = (self.params[i + 1] & 0x0F) as u8;
                                if code == 38 {
                                    self.attr = (self.attr & 0xF0) | n;
                                } else {
                                    self.attr = (self.attr & 0x0F) | n << 4;
                                }
                            }
                            i += 2;
                        }
                        39 => self.attr = (self.attr & 0xF0) | (self.default_attr & 0x0F),
                        40..=47 => {
                            self.attr = (self.attr & 0x0F) | ANSI_TO_VGA[(code - 40) as usize] << 4
                        }
                        49 => self.attr &= 0x0F,
                        90..=97 => {
                            self.attr =
                                (self.attr & 0xF0) | (ANSI_TO_VGA[(code - 90) as usize] | 0x08)
                        }
                        _ => {}
                    }
                }
            }
            b'H' => {
                // ESC [ row ; col H, 1-based, both optional
                let row = if self.nparams >= 1 { self.params[0].max(1) as usize } else { 1 };
                let col = if self.nparams >= 2 { self.params[1].max(1) as usize } else { 1 };
                self.row = (row - 1).min(self.rows - 1);
                self.col = (col - 1).min(self.cols - 1);
            }
            b'J' => {
                if self.params[0] == 2 || self.nparams == 0 {
                    self.clear_fb(fb);
                }
            }
            b'K' => {
                // erase to the end of the line
                let (col, row) = (self.col.min(self.cols), self.row);
                let base = self.base();
                for c in col..self.cols {
                    self.lines[base + row][c] = BLANK;
                    self.paint(fb, c, row, BLANK, false);
                }
            }
            _ => {}
        }
    }
}

/// Adapter that lets `write!` print through a locked console and framebuffer.
struct Out<'a> {
    con: &'a mut FbConsole,
    fb: &'a mut Framebuffer,
}

impl Out<'_> {
    fn put(&mut self, b: u8) {
        self.con.put_byte(self.fb, b);
    }
}

impl fmt::Write for Out<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for ch in s.chars() {
            self.put(if ch.is_ascii() { ch as u8 } else { b'?' });
        }
        Ok(())
    }
}

/// Prints through the console. Takes both locks once for the whole message
/// and flushes once at the end.
#[doc(hidden)]
pub fn _fb_print(args: fmt::Arguments) {
    use fmt::Write;
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut con = CONSOLE.lock();
        if !con.initialized {
            return;
        }
        framebuffer::with(|fb| {
            con.begin(fb);
            let mut out = Out { con: &mut con, fb };
            let _ = out.write_fmt(args);
            let Out { con, fb } = out;
            con.finish(fb);
        });
    });
}

/// Like [`_fb_print`] but gives up instead of waiting for a lock. Used by
/// panic and exception handlers, which may have interrupted code that holds
/// the console.
#[doc(hidden)]
pub fn _fb_emergency_print(args: fmt::Arguments) {
    use fmt::Write;
    let (Some(mut con), Some(mut fbg)) = (CONSOLE.try_lock(), FRAMEBUFFER.try_lock()) else {
        return;
    };
    if !con.initialized {
        return;
    }
    if let Some(fb) = fbg.as_mut() {
        con.begin(fb);
        let mut out = Out { con: &mut con, fb };
        let _ = out.write_fmt(args);
        let Out { con, fb } = out;
        con.finish(fb);
    }
}

/// Print to the console.
#[macro_export]
macro_rules! fb_print {
    ($($arg:tt)*) => {
        $crate::graphics::console::_fb_print(format_args!($($arg)*))
    };
}

/// Print to the console, appending a newline.
#[macro_export]
macro_rules! fb_println {
    () => ($crate::fb_print!("\n"));
    ($fmt:expr) => ($crate::fb_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::fb_print!(
        concat!($fmt, "\n"), $($arg)*));
}

/// Print to the console from a panic or exception handler (never blocks).
#[macro_export]
macro_rules! emergency_println {
    ($fmt:expr) => ($crate::graphics::console::_fb_emergency_print(
        format_args!(concat!($fmt, "\n"))));
    ($fmt:expr, $($arg:tt)*) => ($crate::graphics::console::_fb_emergency_print(
        format_args!(concat!($fmt, "\n"), $($arg)*)));
}

/// Waits a few milliseconds without relying on interrupts (printing runs
/// with them off), using the calibrated timestamp counter.
fn pause_ms(ms: u64) {
    let end = crate::time::uptime_ms() + ms;
    while crate::time::uptime_ms() < end {
        core::hint::spin_loop();
    }
}
