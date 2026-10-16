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

    // Moon
    vga_buffer::set_color(Color::White, Color::Black);
    vga_buffer::write_at(64, 5, "  ___  ");
    vga_buffer::write_at(63, 6, " /   \\ ");
    vga_buffer::write_at(63, 7, " \\___/ ");

    // Footer
    vga_buffer::set_color(Color::White, Color::Black);
    vga_buffer::write_at(0, 24, "--------------------------------------------------------------------------------");
    vga_buffer::write_at(25, 23, "Press any key to enter the shell");

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
