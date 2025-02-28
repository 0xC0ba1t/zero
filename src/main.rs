#![no_std] // no use of 'std' lib
#![no_main] // no 'main' function

use core::panic::PanicInfo;
mod vga_buffer;

#[panic_handler] // defines func as panic handler func
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info); // output panic message
    loop {}
}

#[no_mangle] // disable name mangling
pub extern "C" fn _start() -> ! { // entry point func
    println!("Hello World{}", "!"); // hello world!
    panic!("test panic");

    loop {}
}
