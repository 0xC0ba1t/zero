#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(zero::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use bootloader_api::{BootInfo, entry_point, config::{BootloaderConfig, Mapping}};
use zero::println;
use core::panic::PanicInfo;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {

    use zero::memory::{self, BootInfoFrameAllocator};
    use zero::allocator;
    use x86_64::VirtAddr;
    use zero::serial_println;

    serial_println!("Kernel entry point reached");

    zero::init(); // init idt <Interrupt Descriptor Table>
    serial_println!("IDT initialized");

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset.into_option().unwrap_or(0));
    serial_println!("Phys mem offset: {:?}", phys_mem_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    serial_println!("Mapper initialized");
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_regions) };
    serial_println!("Frame allocator initialized");

    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");
    serial_println!("Heap initialized");

    // Map the VGA text buffer at 0xb8000 so we can write to it
    {
        use x86_64::structures::paging::{Page, PhysFrame, PageTableFlags as Flags, Mapper, Size4KiB};
        let vga_page: Page<Size4KiB> = Page::containing_address(x86_64::VirtAddr::new(0xb8000));
        let vga_frame = PhysFrame::containing_address(x86_64::PhysAddr::new(0xb8000));
        let flags = Flags::PRESENT | Flags::WRITABLE | Flags::NO_CACHE;
        unsafe {
            mapper
                .map_to(vga_page, vga_frame, flags, &mut frame_allocator)
                .expect("failed to map VGA buffer")
                .flush();
        }
    }

    // Initialize framebuffer if available
    if let Some(fb) = boot_info.framebuffer.as_mut() {
        let info = fb.info();
        let frame_buffer = unsafe { fb.buffer_mut() };
        *zero::graphics::framebuffer::FRAMEBUFFER.lock() = Some(zero::graphics::framebuffer::Framebuffer::new(info, frame_buffer));
    }

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

    let dots = "................................................................................";
    vga_buffer::write_at(0, 1, dots);
    vga_buffer::write_at(0, 2, dots);
    vga_buffer::write_at(0, 23, dots);

    // Stars
    vga_buffer::set_color(Color::White, Color::Black);
    vga_buffer::put_at(5, 5, b'*');
    vga_buffer::put_at(40, 4, b'*');
    vga_buffer::put_at(70, 20, b'*');
    vga_buffer::put_at(30, 18, b'*');

    // Clouds (using block characters and shading)
    // Cloud 1
    vga_buffer::set_color(Color::DarkGray, Color::Black);
    for x in 10..25 { vga_buffer::put_at(x, 6, 219); }
    for x in 12..22 { vga_buffer::put_at(x, 5, 219); }
    vga_buffer::set_color(Color::LightGray, Color::Black);
    vga_buffer::put_at(25, 6, 176);
    vga_buffer::put_at(9, 6, 176);

    // Cloud 2
    vga_buffer::set_color(Color::DarkGray, Color::Black);
    for x in 50..65 { vga_buffer::put_at(x, 15, 219); }
    for x in 52..62 { vga_buffer::put_at(x, 14, 219); }
    vga_buffer::set_color(Color::LightGray, Color::Black);
    vga_buffer::put_at(65, 15, 176);
    vga_buffer::put_at(49, 15, 176);

    // Moon (C-shape with shading)
    vga_buffer::set_color(Color::White, Color::Black);
    // Top tip
    vga_buffer::put_at(68, 3, 219); vga_buffer::put_at(69, 3, 219); vga_buffer::put_at(70, 3, 178); vga_buffer::put_at(71, 3, 177);
    vga_buffer::put_at(66, 4, 219); vga_buffer::put_at(67, 4, 219); vga_buffer::put_at(72, 4, 176);
    // Middle body
    for y in 5..9 {
        vga_buffer::put_at(65, y, 219);
        vga_buffer::put_at(66, y, 219);
    }
    // Shading on body
    vga_buffer::set_color(Color::LightGray, Color::Black);
    vga_buffer::put_at(67, 5, 178); vga_buffer::put_at(68, 5, 177);
    vga_buffer::put_at(67, 8, 178); vga_buffer::put_at(68, 8, 177);
    vga_buffer::put_at(67, 9, 219); vga_buffer::put_at(68, 9, 219);
    vga_buffer::set_color(Color::White, Color::Black);
    // Bottom tip
    vga_buffer::put_at(66, 10, 219); vga_buffer::put_at(67, 10, 219); vga_buffer::put_at(72, 10, 176);
    vga_buffer::put_at(68, 11, 219); vga_buffer::put_at(69, 11, 219); vga_buffer::put_at(70, 11, 178); vga_buffer::put_at(71, 11, 177);

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
    use zero::serial_println;
    serial_println!("{}", info);
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
