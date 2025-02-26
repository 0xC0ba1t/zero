#![no_std] // no use of 'std' lib
#![no_main] // no 'main' function

use core::panic::PanicInfo;

#[panic_handler] // defines func as panic handler func
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[no_mangle] // disable name mangling
pub extern "C" fn _start() -> ! {
    // entry point func
    // linker looks for '_start' func by default
    loop {}
}
