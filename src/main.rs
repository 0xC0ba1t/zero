#![no_std] // no use of 'std' lib
#![no_main] // no 'main' function

use core::panic::PanicInfo;

#[panic_handler] // defines func as panic handler func
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

static HELLO: &[u8] = b"Hello World!";

#[no_mangle] // disable name mangling
pub extern "C" fn _start() -> ! { // entry point func
    let vga_buffer = 0xb8000 as *mut u8;

    for (i,&byte) in HELLO.iter().enumerate() {
        unsafe {
            *vga_buffer.offset(i as isize * 2) = byte;
            *vga_buffer.offset(i as isize * 2 + 1) = 0xb;
        }
    }

    loop {}
}
