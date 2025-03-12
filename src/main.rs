#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(zero::test_runner)]
#![reexport_test_harness_main = "test_main"]

use bootloader::{BootInfo, entry_point};
use zero::println;
use core::panic::PanicInfo;

entry_point!(kernal_main);

fn kernal_main(boot_info: &'static BootInfo) -> ! {
    println!("Hello World{}", "!");

    zero::init(); // init idt <Interrupt Descriptor Table>

    fn stack_overflow() {
        stack_overflow();
    }

    // uncomment lines below (manually) trigger a page fault

    // let ptr = 0xdeadbeef as *mut u8;
    // unsafe { *ptr = 42; }

    // uncomment line below to (manually) trigger a stack overflow
    // stack_overflow();

    /// (debug) show PhysAddr of (currently) active lvl 4 page
    use x86_64::registers::control::Cr3;

    let (level_4_page_table, _) = Cr3::read();
    println!("Level 4 page table at: {:?}", level_4_page_table.start_address());

    #[cfg(test)]
    test_main();

    println!("It did not crash!");
    zero::hlt_loop();
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
