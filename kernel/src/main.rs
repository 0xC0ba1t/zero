#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(zero::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use bootloader_api::{BootInfo, entry_point, config::{BootloaderConfig, Mapping}};
use core::panic::PanicInfo;
use zero::emergency_println;
use zero::serial_println;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config.kernel_stack_size = 256 * 1024;
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    use x86_64::VirtAddr;
    use zero::allocator;
    use zero::graphics::{bootscreen, console::CONSOLE, framebuffer::{Framebuffer, FRAMEBUFFER}};
    use zero::memory::{self, BootInfoFrameAllocator};

    serial_println!("[kernel] kernel_main reached");

    zero::init();

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset.into_option().unwrap_or(0));
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_regions) };

    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");
    serial_println!("[kernel] heap initialized ({} MiB)", allocator::HEAP_SIZE / (1024 * 1024));

    // The screen: needs the heap (for the shadow buffer), so it comes after it.
    if let Some(fb) = boot_info.framebuffer.take() {
        let info = fb.info();
        let buffer = fb.into_buffer();
        serial_println!(
            "[kernel] framebuffer: {}x{} {:?} {} bytes/pixel",
            info.width, info.height, info.pixel_format, info.bytes_per_pixel
        );
        *FRAMEBUFFER.lock() = Some(Framebuffer::new(info, buffer));
    } else {
        serial_println!("[kernel] no framebuffer provided by bootloader");
    }
    CONSOLE.lock().init();

    *zero::fs::FILESYSTEM.lock() = Some(zero::fs::Filesystem::new());

    // Timer (IRQ0) and keyboard (IRQ1). The PIC lock is taken with interrupts
    // off: the timer handler takes it too and would otherwise deadlock.
    x86_64::instructions::interrupts::without_interrupts(|| unsafe {
        zero::interrupts::PICS.lock().write_masks(0xFC, 0xFF);
    });
    serial_println!("[kernel] interrupts unmasked");

    // Boot screen (animated) until a key is pressed, then the shell window.
    bootscreen::run();
    CONSOLE.lock().init();
    zero::graphics::ui::restore_shell_screen();

    #[cfg(test)]
    test_main();

    serial_println!("[kernel] starting shell");
    zero::shell::run();
}

/// This function is called on panic.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Disable interrupts first so a timer tick can't interrupt the report.
    x86_64::instructions::interrupts::disable();
    serial_println!("[kernel] PANIC: {}", info);
    // The panic may have happened while the console was locked; never wait for it.
    emergency_println!("\n\x1b[91mKERNEL PANIC\x1b[0m: {}", info);
    zero::hlt_loop_noirq();
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
