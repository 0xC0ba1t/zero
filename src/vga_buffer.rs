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
        color_code: ColorCode::new(Color::Yellow, Color::Black),
        buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
    });
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
    /// All colors together with the names used to refer to them.
    pub const NAMES: [(&'static str, Color); 16] = [
        ("black", Color::Black),
        ("blue", Color::Blue),
        ("green", Color::Green),
        ("cyan", Color::Cyan),
        ("red", Color::Red),
        ("magenta", Color::Magenta),
        ("brown", Color::Brown),
        ("lightgray", Color::LightGray),
        ("darkgray", Color::DarkGray),
        ("lightblue", Color::LightBlue),
        ("lightgreen", Color::LightGreen),
        ("lightcyan", Color::LightCyan),
        ("lightred", Color::LightRed),
        ("pink", Color::Pink),
        ("yellow", Color::Yellow),
        ("white", Color::White),
    ];

    /// Looks up a color by name, ignoring case.
    pub fn from_name(name: &str) -> Option<Color> {
        Color::NAMES
            .iter()
            .find(|entry| entry.0.eq_ignore_ascii_case(name))
            .map(|entry| entry.1)
    }

    /// Returns the color with the given VGA palette index (only the low 4 bits count).
    pub fn from_index(index: u8) -> Color {
        Color::NAMES[(index & 0x0F) as usize].1
    }
}

/// A combination of a foreground and a background color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    /// Create a new `ColorCode` with the given foreground and background colors.
    fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

/// A screen character in the VGA text buffer, consisting of an ASCII character and a `ColorCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

/// The height of the text buffer (normally 25 lines).
const BUFFER_HEIGHT: usize = 25;
/// The width of the text buffer (normally 80 columns).
const BUFFER_WIDTH: usize = 80;

/// A structure representing the VGA text buffer.
#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

/// A writer type that allows writing ASCII bytes and strings to an underlying `Buffer`.
///
/// Wraps lines at `BUFFER_WIDTH`. Supports newline characters and implements the
/// `core::fmt::Write` trait.
pub struct Writer {
    column_position: usize,
    color_code: ColorCode,
    buffer: &'static mut Buffer,
}

impl Writer {
    /// Writes an ASCII byte to the buffer.
    ///
    /// Wraps lines at `BUFFER_WIDTH`. Supports the `\n` newline character.
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position;

                let color_code = self.color_code;
                self.buffer.chars[row][col].write(ScreenChar {
                    ascii_character: byte,
                    color_code,
                });
                self.column_position += 1;
            }
        }
    }

    /// Writes a raw byte at any position using the current color.
    ///
    /// Positions outside the screen are ignored. Unlike `write_byte` this does
    /// not move the cursor, which makes it suitable for drawing.
    pub fn put_at(&mut self, column: usize, row: usize, byte: u8) {
        if column >= BUFFER_WIDTH || row >= BUFFER_HEIGHT {
            return;
        }
        let color_code = self.color_code;
        self.buffer.chars[row][column].write(ScreenChar {
            ascii_character: byte,
            color_code,
        });
    }

    /// Erases the character left of the cursor on the current row.
    ///
    /// Does nothing at the start of a row, so the cursor never jumps back to
    /// the previous line.
    pub fn backspace(&mut self) {
        if self.column_position == 0 {
            return;
        }
        self.column_position -= 1;
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        self.buffer.chars[BUFFER_HEIGHT - 1][self.column_position].write(blank);
    }

    /// Writes the given ASCII string to the buffer.
    ///
    /// Wraps lines at `BUFFER_WIDTH`. Supports the `\n` newline character. Does **not**
    /// support strings with non-ASCII characters, since they can't be printed in the VGA text
    /// mode.
    fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                // printable ASCII byte or newline
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                // backspace
                0x08 => self.backspace(),
                // not part of printable ASCII range
                _ => self.write_byte(0xfe),
            }
        }
        self.update_cursor();
    }

    /// Changes the colors used for everything written from now on.
    pub fn set_color(&mut self, foreground: Color, background: Color) {
        self.color_code = ColorCode::new(foreground, background);
    }

    /// Blanks the whole screen and moves the cursor to the start of the last row.
    pub fn clear_screen(&mut self) {
        for row in 0..BUFFER_HEIGHT {
            self.clear_row(row);
        }
        self.column_position = 0;
        self.update_cursor();
    }

    /// Shifts all lines one line up and clears the last row.
    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer.chars[row][col].read();
                self.buffer.chars[row - 1][col].write(character);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    /// Moves the blinking hardware cursor to the writer's position.
    fn update_cursor(&self) {
        use x86_64::instructions::port::Port;

        let column = core::cmp::min(self.column_position, BUFFER_WIDTH - 1);
        let position = (BUFFER_HEIGHT - 1) * BUFFER_WIDTH + column;

        unsafe {
            let mut index: Port<u8> = Port::new(0x3D4);
            let mut data: Port<u8> = Port::new(0x3D5);
            index.write(0x0F); // cursor location, low byte
            data.write((position & 0xFF) as u8);
            index.write(0x0E); // cursor location, high byte
            data.write(((position >> 8) & 0xFF) as u8);
        }
    }

    /// Clears a row by overwriting it with blank characters.
    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

/// Like the `print!` macro in the standard library, but prints to the VGA text buffer.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*)));
}

/// Like the `println!` macro in the standard library, but prints to the VGA text buffer.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

/// Prints the given formatted string to the VGA text buffer through the global `WRITER` instance.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args).unwrap();
    });
}

/// Clears the VGA text buffer through the global `WRITER` instance.
pub fn clear_screen() {
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        WRITER.lock().clear_screen();
    });
}

/// Sets the text colors of the global `WRITER` instance.
pub fn set_color(foreground: Color, background: Color) {
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        WRITER.lock().set_color(foreground, background);
    });
}

/// Draws a raw byte at `(column, row)` through the global `WRITER`.
/// `(0, 0)` is the top left corner of the screen.
pub fn put_at(column: i32, row: i32, byte: u8) {
    use x86_64::instructions::interrupts;

    if column < 0 || row < 0 {
        return;
    }
    interrupts::without_interrupts(|| {
        WRITER.lock().put_at(column as usize, row as usize, byte);
    });
}

/// Draws `text` starting at `(column, row)`. Characters outside the printable
/// ASCII range show up as a filled square.
pub fn write_at(column: i32, row: i32, text: &str) {
    for (offset, byte) in text.bytes().enumerate() {
        let byte = match byte {
            0x20..=0x7e => byte,
            _ => 0xfe,
        };
        put_at(column.saturating_add(offset as i32), row, byte);
    }
}

/// Shows or hides the blinking hardware cursor.
pub fn set_cursor_visible(visible: bool) {
    use x86_64::instructions::interrupts;
    use x86_64::instructions::port::Port;

    interrupts::without_interrupts(|| unsafe {
        let mut index: Port<u8> = Port::new(0x3D4);
        let mut data: Port<u8> = Port::new(0x3D5);
        if visible {
            index.write(0x0A); // cursor start scanline
            data.write(14);
            index.write(0x0B); // cursor end scanline
            data.write(15);
        } else {
            index.write(0x0A);
            data.write(0x20); // bit 5 turns the cursor off
        }
    });
}

/// Returns the current text color as a raw VGA attribute byte.
pub fn color_code() -> u8 {
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| WRITER.lock().color_code.0)
}

/// Restores a color previously returned by `color_code`.
pub fn set_color_code(code: u8) {
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        WRITER.lock().color_code = ColorCode(code);
    });
}

#[test_case]
fn test_println_simple() {
    println!("test_println_simple output");
}

#[test_case]
fn test_println_many() {
    for _ in 0..200 {
        println!("test_println_many output");
    }
}

#[test_case]
fn test_println_output() {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    let s = "Some test string that fits on a single line";
    interrupts::without_interrupts(|| {
        let mut writer = WRITER.lock();
        writeln!(writer, "\n{}", s).expect("writeln failed");
        for (i, c) in s.chars().enumerate() {
            let screen_char = writer.buffer.chars[BUFFER_HEIGHT - 2][i].read();
            assert_eq!(char::from(screen_char.ascii_character), c);
        }
    });
}

#[test_case]
fn test_backspace_erases_last_char() {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        let mut writer = WRITER.lock();
        writeln!(writer, "\nabc\u{8}").expect("writeln failed");
        let row = BUFFER_HEIGHT - 2;
        assert_eq!(char::from(writer.buffer.chars[row][0].read().ascii_character), 'a');
        assert_eq!(char::from(writer.buffer.chars[row][1].read().ascii_character), 'b');
        assert_eq!(char::from(writer.buffer.chars[row][2].read().ascii_character), ' ');
    });
}

#[test_case]
fn test_color_from_name() {
    assert_eq!(Color::from_name("red"), Some(Color::Red));
    assert_eq!(Color::from_name("LightBlue"), Some(Color::LightBlue));
    assert_eq!(Color::from_name("chartreuse"), None);
}

#[test_case]
fn test_put_at_draws_without_moving_the_cursor() {
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        let mut writer = WRITER.lock();
        let column = writer.column_position;
        writer.put_at(5, 3, b'Q');
        assert_eq!(char::from(writer.buffer.chars[3][5].read().ascii_character), 'Q');
        assert_eq!(writer.column_position, column);
        // off-screen positions are ignored instead of panicking
        writer.put_at(BUFFER_WIDTH, 0, b'X');
        writer.put_at(0, BUFFER_HEIGHT, b'X');
    });
}

#[test_case]
fn test_write_at_draws_a_string() {
    use x86_64::instructions::interrupts;

    write_at(10, 4, "hi");
    interrupts::without_interrupts(|| {
        let writer = WRITER.lock();
        assert_eq!(char::from(writer.buffer.chars[4][10].read().ascii_character), 'h');
        assert_eq!(char::from(writer.buffer.chars[4][11].read().ascii_character), 'i');
    });
}

#[test_case]
fn test_color_from_index() {
    assert_eq!(Color::from_index(0), Color::Black);
    assert_eq!(Color::from_index(14), Color::Yellow);
    assert_eq!(Color::from_index(15), Color::White);
    // only the low four bits are used
    assert_eq!(Color::from_index(16), Color::Black);
}
