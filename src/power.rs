//! Restarting and powering off the machine.

use crate::hlt_loop;
use x86_64::instructions::interrupts;
use x86_64::instructions::port::Port;

/// Resets the machine by pulsing the CPU reset line through the keyboard controller.
pub fn reboot() -> ! {
    interrupts::disable();
    unsafe {
        let mut command_port: Port<u8> = Port::new(0x64);
        command_port.write(0xFE);
    }
    hlt_loop()
}
