#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(zero::test_runner)]
#![reexport_test_harness_modules = "test_main"]

extern crate alloc;

use bootloader_api::{BootInfo, entry_point, config::{BootloaderConfig, Mapping}};
use zero::fb_println;
use zero::serial_println;
use core::panic::PanicInfo;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    // The default stack size (80 KiB) is insufficient for lazy_static!
    // initializers like WRITER (which allocates ~48 KiB for a logical scrollback
    // buffer on the stack). Increase to 256 KiB to avoid stack overflow.
    config.kernel_stack_size = 256 * 1024;
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {

    use zero::memory::{self, BootInfoFrameAllocator};
    use zero::allocator;
    use x86_64::VirtAddr;

    serial_println!("[kernel] kernel_main reached");

    zero::init();
    serial_println!("[kernel] idt initialized");

    let phys_mem_offset_val = boot_info.physical_memory_offset.into_option().unwrap_or(0);
    serial_println!("[kernel] phys_mem_offset = 0x{:x}", phys_mem_offset_val);

    let phys_mem_offset = VirtAddr::new(phys_mem_offset_val);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    serial_println!("[kernel] mapper initialized");

    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_regions) };
    serial_println!("[kernel] frame allocator initialized");

    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");
    serial_println!("[kernel] heap initialized");

    // Initialize framebuffer from bootloader-provided info
    if let Some(mut fb) = boot_info.framebuffer.take() {
        let info = fb.info();
        let buffer = fb.into_buffer();
        *zero::graphics::framebuffer::FRAMEBUFFER.lock() = Some(
            zero::graphics::framebuffer::Framebuffer::new(info, buffer)
        );
        serial_println!("[kernel] framebuffer initialized: {}x{}", info.width, info.height);
    } else {
        serial_println!("[kernel] no framebuffer provided by bootloader");
    }

    // Initialize the framebuffer console
    zero::graphics::framebuffer::CONSOLE.lock().init();
    serial_println!("[kernel] framebuffer console initialized");

    *zero::fs::FILESYSTEM.lock() = Some(zero::fs::Filesystem::new());
    serial_println!("[kernel] filesystem initialized");

    // Unmask only keyboard IRQ (IRQ1, bit 1) for now.
    // Keep timer masked to avoid issues during boot screen.
    // 0xFD = 11111101 — unmask only IRQ1
    serial_println!("[kernel] unmasking keyboard IRQ");
    unsafe {
        zero::interrupts::PICS.lock().write_masks(0xFD, 0xFF);
    }
    serial_println!("[kernel] keyboard IRQ unmasked");

    // Render the full boot screen to the framebuffer
    boot_screen();
    serial_println!("[kernel] boot_screen done");

    #[cfg(test)]
    test_main();

    serial_println!("[kernel] starting shell");
    zero::shell::run();
}

fn boot_screen() {
    use zero::graphics::font;
    use zero::graphics::display::Display as _;
    use zero::graphics::framebuffer::{self, CONSOLE};
    use alloc::format;

    serial_println!("[kernel] boot_screen starting");

    // Clear the framebuffer
    CONSOLE.lock().clear();

    let white: u32 = 0xFFFFFF;
    let light_gray: u32 = 0xD3D3D3;
    let dark_gray: u32 = 0xA9A9A9;

    let cell_w = font::CHAR_WIDTH + 2;
    let cell_h = font::CHAR_HEIGHT + 2;

    // Draw directly to the framebuffer
    let mut fb_guard = framebuffer::FRAMEBUFFER.lock();
    let fb = match fb_guard.as_mut() {
        Some(fb) => fb,
        None => {
            serial_println!("[kernel] no framebuffer for boot screen");
            return;
        }
    };

    // Top and bottom separator lines (dotted)
    let dots = "................................................................................";
    font::draw_text(fb, 0, cell_h, dots, white, 0);
    font::draw_text(fb, 0, 2 * cell_h, dots, white, 0);
    font::draw_text(fb, 0, 23 * cell_h, dots, white, 0);

    // Welcome message
    let version = env!("CARGO_PKG_VERSION");
    let welcome = format!("Welcome to Zero v{}", version);
    font::draw_text(fb, 0, 0, &welcome, white, 0);

    // Stars
    font::draw_block_char(fb, 5, 5, b'*', white, 0);
    font::draw_block_char(fb, 40, 4, b'*', white, 0);
    font::draw_block_char(fb, 70, 20, b'*', white, 0);
    font::draw_block_char(fb, 30, 18, b'*', white, 0);

    // Cloud 1
    for x in 10..25 { font::draw_block_char(fb, x, 6, 219, dark_gray, 0); }
    for x in 12..22 { font::draw_block_char(fb, x, 5, 219, dark_gray, 0); }
    font::draw_block_char(fb, 25, 6, 176, light_gray, 0);
    font::draw_block_char(fb, 9, 6, 176, light_gray, 0);

    // Cloud 2
    for x in 50..65 { font::draw_block_char(fb, x, 15, 219, dark_gray, 0); }
    for x in 52..62 { font::draw_block_char(fb, x, 14, 219, dark_gray, 0); }
    font::draw_block_char(fb, 65, 15, 176, light_gray, 0);
    font::draw_block_char(fb, 49, 15, 176, light_gray, 0);

    // Moon (C-shape with shading)
    font::draw_block_char(fb, 68, 3, 219, white, 0);
    font::draw_block_char(fb, 69, 3, 219, white, 0);
    font::draw_block_char(fb, 70, 3, 178, light_gray, 0);
    font::draw_block_char(fb, 71, 3, 177, light_gray, 0);
    font::draw_block_char(fb, 66, 4, 219, white, 0);
    font::draw_block_char(fb, 67, 4, 219, white, 0);
    font::draw_block_char(fb, 72, 4, 176, light_gray, 0);
    for y in 5..9 {
        font::draw_block_char(fb, 65, y, 219, white, 0);
        font::draw_block_char(fb, 66, y, 219, white, 0);
    }
    font::draw_block_char(fb, 67, 5, 178, light_gray, 0);
    font::draw_block_char(fb, 68, 5, 177, light_gray, 0);
    font::draw_block_char(fb, 67, 8, 178, light_gray, 0);
    font::draw_block_char(fb, 68, 8, 177, light_gray, 0);
    font::draw_block_char(fb, 67, 9, 219, white, 0);
    font::draw_block_char(fb, 68, 9, 219, white, 0);
    font::draw_block_char(fb, 66, 10, 219, white, 0);
    font::draw_block_char(fb, 67, 10, 219, white, 0);
    font::draw_block_char(fb, 72, 10, 176, light_gray, 0);
    font::draw_block_char(fb, 68, 11, 219, white, 0);
    font::draw_block_char(fb, 69, 11, 219, white, 0);
    font::draw_block_char(fb, 70, 11, 178, light_gray, 0);
    font::draw_block_char(fb, 71, 11, 177, light_gray, 0);

    // Footer
    font::draw_text(fb, 25 * cell_w, 24 * cell_h, "Press any key to enter", white, 0);

    drop(fb_guard);

    serial_println!("[kernel] boot screen rendered to framebuffer");

    // Wait for any key press
    loop {
        match zero::input::read_key() {
            _ => break,
        }
    }

    // After key press, clear framebuffer and reset console
    serial_println!("[kernel] key pressed, transitioning to shell");
    CONSOLE.lock().clear();
}

/// This function is called on panic.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Disable interrupts immediately to prevent nested double faults
    // from timer interrupts firing during panic handling.
    x86_64::instructions::interrupts::disable();
    serial_println!("[kernel] PANIC: {}", info);
    // Try framebuffer too, in case it's set up
    fb_println!("PANIC: {}", info);
    zero::hlt_loop_noirq(); // disable IRQs + hlt to avoid NMI re-entry
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
