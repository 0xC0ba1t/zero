use crate::println;
use crate::gdt;
use crate::input::{self, Key};
use crate::time;
use crate::InterruptIndex;
use lazy_static::lazy_static;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};
use x86_64::structures::idt::PageFaultErrorCode;
use pic8259::ChainedPics;
use spin;
use crate::hlt_loop;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: spin::Mutex<ChainedPics> =
    spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

lazy_static! {
    /// The IDT lives in static memory so its address remains valid after
    /// `init_idt` returns and the CPU's IDTR keeps pointing at it.
    /// In UEFI mode the timer/keyboard IRQ entries are left unset because
    /// the legacy 8259 PIC is unavailable.
    static ref IDT: spin::Mutex<InterruptDescriptorTable> = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault.set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt.page_fault.set_handler_fn(page_fault_handler);
        spin::Mutex::new(idt)
    };
}

/// Initialise the IDT.
///
/// When `legacy_pic` is `true` the timer and keyboard IRQ handlers (vectors
/// 32 and 33) are registered and wired to the 8259 PIC.  In UEFI mode the
/// legacy PIC is unavailable, so those vectors are left unset to avoid
/// spurious or mis-routed interrupts causing double faults.
pub fn init_idt(legacy_pic: bool) {
    let mut idt = IDT.lock();

    if legacy_pic {
        idt[InterruptIndex::Timer.as_u8()]
            .set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_u8()]
            .set_handler_fn(keyboard_interrupt_handler);
    }

    idt.load();
}

/// handles breakpoint exceptions
extern "x86-interrupt" fn breakpoint_handler(
    stack_frame: InterruptStackFrame)
{
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

/// handles double fault exceptions
extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame, _error_code: u64) -> !
{
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

/// handles page fault exceptions
extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    println!("EXCEPTION: PAGE FAULT");
    println!("Accessed Address: {:?}", Cr2::read());
    println!("Error Code: {:?}", error_code);
    println!("{:#?}", stack_frame);
    hlt_loop();
}

extern "x86-interrupt" fn timer_interrupt_handler(
    _stack_frame: InterruptStackFrame)
{
    time::tick();

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(
    _stack_frame: InterruptStackFrame)
{
    use pc_keyboard::{layouts, DecodedKey, HandleControl, KeyCode, Keyboard, ScancodeSet1};
    use spin::Mutex;
    use x86_64::instructions::port::Port;

    lazy_static! {
        static ref KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> =
            Mutex::new(Keyboard::new(ScancodeSet1::new(),
                layouts::Us104Key, HandleControl::MapLettersToUnicode)
            );
    }

    let mut keyboard = KEYBOARD.lock();
    let mut port = Port::new(0x60);

    let scancode: u8 = unsafe { port.read() };
    if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
        if let Some(key) = keyboard.process_keyevent(key_event) {
            match key {
                DecodedKey::Unicode(character) => {
                    // ctrl+c also stops a running script
                    if character == '\u{3}' {
                        input::request_interrupt();
                    }
                    input::push_key(Key::Char(character));
                }
                DecodedKey::RawKey(KeyCode::ArrowUp) => input::push_key(Key::Up),
                DecodedKey::RawKey(KeyCode::ArrowDown) => input::push_key(Key::Down),
                DecodedKey::RawKey(KeyCode::ArrowLeft) => input::push_key(Key::Left),
                DecodedKey::RawKey(KeyCode::ArrowRight) => input::push_key(Key::Right),
                DecodedKey::RawKey(KeyCode::PageUp) => input::push_key(Key::PageUp),
                DecodedKey::RawKey(KeyCode::PageDown) => input::push_key(Key::PageDown),
                DecodedKey::RawKey(_) => {}
            }
        }
    }

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

#[test_case]
/// test breakpoint exceptions
fn test_breakpoint_exception() {
    // (manually) invoke a breakpoint exception
    x86_64::instructions::interrupts::int3();
}
