use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use volatile::Volatile;

lazy_static! {
    /// A global `Writer` instance that can be used for printing to the VGA text buffer.
    ///
    /// Used by the `print!` and `println!` macros.
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        column_position: 0,
        row_position: 0,
        scroll_offset: 0,
        color_code: ColorCode::new(Color::Yellow, Color::Black),
        logical_buffer: LogicalBuffer { chars: [[ScreenChar { ascii_character: b' ', color_code: ColorCode::new(Color::Yellow, Color::Black) }; BUFFER_WIDTH]; SCROLLBACK_HEIGHT] },
        physical_buffer: None,
        suspended_render: false,
    });
}

/// Initialize the physical VGA buffer pointer.
/// This must be called after the VGA text buffer has been mapped in the page table.
pub fn init_physical_buffer() {
    use x86_64::instructions::interrupts;
    crate::serial_println!("[kernel] inside init_physical_buffer");
    interrupts::without_interrupts(|| {
        crate::serial_println!("[kernel] inside without_interrupts");
        let mut guard = WRITER.lock();
        crate::serial_println!("[kernel] WRITER locked");
        // Set the physical buffer pointer to the VGA text buffer at 0xb8000.
        // This must be done with interrupts disabled and the WRITER mutex held
        // to prevent the timer interrupt handler from calling into the Writer
        // while it's in an inconsistent state.
        guard.physical_buffer = Some(unsafe { &mut *(0xb8000 as *mut PhysicalBuffer) });
        crate::serial_println!("[kernel] physical_buffer set");
    });
    crate::serial_println!("[kernel] init_physical_buffer returning");
}

/// The standard color palette in VGA text mode.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

impl Color {
    pub const NAMES: [(&'static str, Color); 16] = [
        ("black", Color::Black), ("blue", Color::Blue), ("green", Color::Green), ("cyan", Color::Cyan),
        ("red", Color::Red), ("magenta", Color::Magenta), ("brown", Color::Brown), ("lightgray", Color::LightGray),
        ("darkgray", Color::DarkGray), ("lightblue", Color::LightBlue), ("lightgreen", Color::LightGreen), ("lightcyan", Color::LightCyan),
        ("lightred", Color::LightRed), ("pink", Color::Pink), ("yellow", Color::Yellow), ("white", Color::White),
    ];

    pub fn from_name(name: &str) -> Option<Color> {
        Color::NAMES.iter().find(|entry| entry.0.eq_ignore_ascii_case(name)).map(|entry| entry.1)
    }
    pub fn from_index(index: u8) -> Color {
        Color::NAMES[(index & 0x0F) as usize].1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;
const SCROLLBACK_HEIGHT: usize = 100;

#[repr(transparent)]
struct PhysicalBuffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

#[repr(transparent)]
struct LogicalBuffer {
    chars: [[ScreenChar; BUFFER_WIDTH]; SCROLLBACK_HEIGHT],
}

pub struct Writer {
    column_position: usize,
    row_position: usize,
    scroll_offset: usize,
    color_code: ColorCode,
    logical_buffer: LogicalBuffer,
    physical_buffer: Option<&'static mut PhysicalBuffer>,
    suspended_render: bool,
}

impl Writer {
    pub fn suspend_render(&mut self) {
        self.suspended_render = true;
    }

    pub fn resume_render(&mut self) {
        self.suspended_render = false;
        self.render();
    }
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = self.row_position;
                let col = self.column_position;
                self.logical_buffer.chars[row][col] = ScreenChar {
                    ascii_character: byte,
                    color_code: self.color_code,
                };
                self.column_position += 1;
                if self.row_position >= self.scroll_offset && self.row_position < self.scroll_offset + BUFFER_HEIGHT {
                    self.render_row(self.row_position - self.scroll_offset);
                }
            }
        }
    }

    pub fn put_at(&mut self, column: usize, row: usize, byte: u8) {
        if column >= BUFFER_WIDTH || row >= BUFFER_HEIGHT {
            return;
        }
        self.logical_buffer.chars[row][column] = ScreenChar {
            ascii_character: byte,
            color_code: self.color_code,
        };
        self.render_row(row);
    }

    pub fn backspace(&mut self) {
        if self.column_position == 0 {
            if self.row_position > 0 {
                self.row_position -= 1;
                self.column_position = BUFFER_WIDTH - 1;
            }
            return;
        }
        self.column_position -= 1;
        self.logical_buffer.chars[self.row_position][self.column_position] = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        if self.row_position >= self.scroll_offset && self.row_position < self.scroll_offset + BUFFER_HEIGHT {
            self.render_row(self.row_position - self.scroll_offset);
        }
    }

    fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                0x08 => self.backspace(),
                _ => self.write_byte(0xfe),
            }
        }
        self.render();
        self.update_cursor();
    }

    pub fn set_color(&mut self, foreground: Color, background: Color) {
        self.color_code = ColorCode::new(foreground, background);
    }

    pub fn clear_screen(&mut self) {
        for row in 0..SCROLLBACK_HEIGHT {
            self.clear_row_logical(row);
        }
        self.row_position = 0;
        self.column_position = 0;
        self.scroll_offset = 0;
        self.render();
    }

    fn render_row(&mut self, row: usize) {
        let Some(ref mut physical_buffer) = self.physical_buffer else {
            return;
        };
        let logical_row = self.scroll_offset + row;
        if logical_row < SCROLLBACK_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                physical_buffer.chars[row][col].write(self.logical_buffer.chars[logical_row][col]);
            }
        } else {
            for col in 0..BUFFER_WIDTH {
                physical_buffer.chars[row][col].write(ScreenChar {
                    ascii_character: b' ',
                    color_code: self.color_code,
                });
            }
        }
    }

    pub fn render(&mut self) {
        for row in 0..BUFFER_HEIGHT {
            self.render_row(row);
        }
    }

    fn new_line(&mut self) {
        if self.row_position < SCROLLBACK_HEIGHT - 1 {
            self.row_position += 1;
        } else {
            for row in 1..SCROLLBACK_HEIGHT {
                self.logical_buffer.chars[row - 1] = self.logical_buffer.chars[row];
            }
            self.clear_row_logical(SCROLLBACK_HEIGHT - 1);
        }
        self.column_position = 0;
        if self.row_position >= BUFFER_HEIGHT && self.scroll_offset < self.row_position - BUFFER_HEIGHT + 1 {
            self.scroll_offset = self.row_position - BUFFER_HEIGHT + 1;
        }
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
            self.render();
        }
    }

    pub fn scroll_down(&mut self) {
        if self.scroll_offset < SCROLLBACK_HEIGHT - BUFFER_HEIGHT {
            self.scroll_offset += 1;
            self.render();
        }
    }

    fn update_cursor(&self) {
        use x86_64::instructions::port::Port;
        if self.row_position < self.scroll_offset || self.row_position >= self.scroll_offset + BUFFER_HEIGHT {
            return;
        }
        let column = core::cmp::min(self.column_position, BUFFER_WIDTH - 1);
        let row_in_view = self.row_position - self.scroll_offset;
        let position = row_in_view * BUFFER_WIDTH + column;
        unsafe {
            let mut index: Port<u8> = Port::new(0x3D4);
            let mut data: Port<u8> = Port::new(0x3D5);
            index.write(0x0F); data.write((position & 0xFF) as u8);
            index.write(0x0E); data.write(((position >> 8) & 0xFF) as u8);
        }
    }

    fn clear_row_logical(&mut self, row: usize) {
        let blank = ScreenChar { ascii_character: b' ', color_code: self.color_code };
        for col in 0..BUFFER_WIDTH {
            self.logical_buffer.chars[row][col] = blank;
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| { WRITER.lock().write_fmt(args).unwrap(); });
}

pub fn clear_screen() {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| { WRITER.lock().clear_screen(); });
}

pub fn set_color(foreground: Color, background: Color) {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| { WRITER.lock().set_color(foreground, background); });
}

pub fn put_at(column: i32, row: i32, byte: u8) {
    use x86_64::instructions::interrupts;
    if column < 0 || row < 0 { return; }
    interrupts::without_interrupts(|| { WRITER.lock().put_at(column as usize, row as usize, byte); });
}

pub fn write_at(column: i32, row: i32, text: &str) {
    for (offset, byte) in text.bytes().enumerate() {
        let byte = match byte { 0x20..=0x7e => byte, _ => 0xfe, };
        put_at(column.saturating_add(offset as i32), row, byte);
    }
}

pub fn set_cursor_visible(visible: bool) {
    use x86_64::instructions::interrupts;
    use x86_64::instructions::port::Port;
    interrupts::without_interrupts(|| unsafe {
        let mut index: Port<u8> = Port::new(0x3D4);
        let mut data: Port<u8> = Port::new(0x3D5);
        if visible {
            index.write(0x0A); data.write(14);
            index.write(0x0B); data.write(15);
        } else {
            index.write(0x0A); data.write(0x20);
        }
    });
}

pub fn color_code() -> u8 {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| WRITER.lock().color_code.0)
}

pub fn set_color_code(code: u8) {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| { WRITER.lock().color_code = ColorCode(code); });
}

pub fn scroll_up() {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| { WRITER.lock().scroll_up(); });
}

pub fn scroll_down() {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| { WRITER.lock().scroll_down(); });
}
