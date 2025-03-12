#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(zero::test_runner)]
#![reexport_test_harness_main = "test_main"]

use bootloader::{BootInfo, entry_point};
use x86_64::VirtAddr;
use zero::{memory::active_level_4_table, println};
use core::panic::PanicInfo;

entry_point!(kernal_main);

fn kernal_main(boot_info: &'static BootInfo) -> ! {
    println!("Hello World{}", "!");

    zero::init(); // init idt <Interrupt Descriptor Table>

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let l4_table = unsafe { active_level_4_table(phys_mem_offset) };

    for (i, entry) in l4_table.iter().enumerate() {
        if !entry.is_unused() {
            println!("L4 Entry {}: {:?}", i, entry);
        }
    }

    fn stack_overflow() {
        stack_overflow();
    }

    // uncomment lines below (manually) trigger a page fault

    // let ptr = 0xdeadbeef as *mut u8;
    // unsafe { *ptr = 42; }

    // uncomment line below to (manually) trigger a stack overflow
    // stack_overflow();

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
