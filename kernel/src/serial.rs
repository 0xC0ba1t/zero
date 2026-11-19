use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;
use x86_64::instructions::port::Port;

lazy_static! {
    pub static ref SERIAL1: Mutex<SerialPort> = {
        let mut serial_port = unsafe { SerialPort::new(0x3F8) };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

#[doc(hidden)]
pub fn _print(args: ::core::fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        SERIAL1
            .lock()
            .write_fmt(args)
            .expect("Printing to serial failed");
    });
}

/// Write a raw byte slice directly to the UART without acquiring the SERIAL1
/// mutex. This is safe to call from interrupt/NMI handlers where the main
/// thread may already hold the SERIAL1 mutex lock, which would deadlock a
/// spinlock-based lock.
pub fn write_raw(bytes: &[u8]) {
    unsafe {
        let mut lsr = Port::<u8>::new(0x3F8 + 5);
        let mut thr = Port::<u8>::new(0x3F8);
        for &byte in bytes {
            // Wait for transmitter hold register empty
            while (lsr.read() & 0x20) == 0 {}
            thr.write(byte);
        }
    }
}

/// Prints to the host through the serial interface.
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*));
    };
}

/// Prints to the host through the serial interface, appending a newline.
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(
        concat!($fmt, "\n"), $($arg)*));
}
