//! Restarting and powering off the machine.

use crate::hlt_loop;
use x86_64::instructions::interrupts;
use x86_64::instructions::port::Port;

/// Resets the machine by pulsing the CPU reset line through the keyboard controller.
pub fn reboot() -> ! {
    interrupts::disable();
    unsafe {
        let mut command_port: Port<u8> = Port::new(0x64);
        // wait until the controller can accept a command (input buffer empty)
        while command_port.read() & 0x02 != 0 {}
        command_port.write(0xFE);
    }
    hlt_loop()
}

/// Powers the machine off.
///
/// This only works on virtual machines (QEMU and Bochs); on real hardware the
/// writes are ignored and the CPU just halts.
pub fn shutdown() -> ! {
    unsafe {
        // ACPI PM1a control port as used by QEMU (SLP_EN | SLP_TYP)
        let mut qemu: Port<u16> = Port::new(0x604);
        qemu.write(0x2000);
        // older QEMU versions and Bochs
        let mut bochs: Port<u16> = Port::new(0xB004);
        bochs.write(0x2000);
    }
    hlt_loop()
}
