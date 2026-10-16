#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(zero::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use bootloader::{BootInfo, entry_point};
use zero::println;
use core::panic::PanicInfo;

entry_point!(kernal_main);

fn kernal_main(boot_info: &'static BootInfo) -> ! {

    use zero::memory::{self, BootInfoFrameAllocator};
    use zero::allocator;
    use x86_64::VirtAddr;

    zero::init(); // init idt <Interrupt Descriptor Table>

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };

    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");
    *zero::fs::FILESYSTEM.lock() = Some(zero::fs::Filesystem::new());

    boot_screen();

    #[cfg(test)]
    test_main();

    zero::shell::run();
}

fn boot_screen() {
    use zero::vga_buffer::{self, Color};
    use alloc::format;
    vga_buffer::clear_screen();

    // Welcome message
    vga_buffer::set_color(Color::White, Color::Black);
    let version = env!("CARGO_PKG_VERSION");
    let welcome = format!("Welcome to Zero v{}", version);
    vga_buffer::write_at(0, 0, &welcome);
    vga_buffer::write_at(0, 1, "--------------------------------------------------------------------------------");

    // Border lines
    vga_buffer::write_at(0, 2, "--------------------------------------------------------------------------------");
    vga_buffer::write_at(0, 23, "--------------------------------------------------------------------------------");

    // Stars
    vga_buffer::set_color(Color::White, Color::Black);
    vga_buffer::put_at(5, 5, b'*');
    vga_buffer::put_at(40, 4, b'*');
    vga_buffer::put_at(70, 20, b'*');
    vga_buffer::put_at(30, 18, b'*');

    // Clouds (using block characters)
    vga_buffer::set_color(Color::DarkGray, Color::Black);
    // Cloud 1
    for x in 10..25 { vga_buffer::put_at(x, 6, 219); }
    for x in 12..22 { vga_buffer::put_at(x, 5, 219); }
    // Cloud 2
    for x in 50..65 { vga_buffer::put_at(x, 15, 219); }
    for x in 52..62 { vga_buffer::put_at(x, 14, 219); }

    // Moon
    vga_buffer::set_color(Color::White, Color::Black);
    for y in 4..9 {
        vga_buffer::put_at(65, y, 219);
        vga_buffer::put_at(66, y, 219);
    }
    vga_buffer::put_at(67, 5, 219); vga_buffer::put_at(67, 7, 219);

    // Footer
    vga_buffer::set_color(Color::White, Color::Black);
    vga_buffer::write_at(25, 24, "Press any key to enter");

    loop {
        match zero::input::read_key() {
            _ => break,
        }
    }
    vga_buffer::clear_screen();
}

/// This function is called on panic.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    zero::hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    zero::test_panic_handler(info)
}

#[test_case]
fn trivial_assertion() {
    assert_eq!(1, 1);
}
