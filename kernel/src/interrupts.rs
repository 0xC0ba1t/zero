use crate::emergency_println;
use crate::gdt;
use crate::input::{self, Key};
use crate::time;
use crate::InterruptIndex;
use lazy_static::lazy_static;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};
use x86_64::structures::idt::PageFaultErrorCode;
use pic8259::ChainedPics;
use spin;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: spin::Mutex<ChainedPics> =
    spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

/// Handler for spurious / unhandled interrupts (e.g. NMI on vector 2).
/// Logs and halts so we can see what fired.
extern "x86-interrupt" fn spurious_handler(_stack_frame: InterruptStackFrame) {
    // Use a raw UART write instead of serial_println! to avoid potential
    // deadlock if the main thread is holding the SERIAL1 mutex when an
    // NMI fires. The x86_64-interrupt ABI saves a new interrupt stack,
    // but the spinlock inside serial_println! would still deadlock.
    crate::serial::write_raw(b"[kernel] spurious interrupt\n");
    crate::hlt_loop_noirq();
}

/// Wakes a processor that is waiting for work (see `smp`).
extern "x86-interrupt" fn wake_handler(_stack_frame: InterruptStackFrame) {
    crate::smp::lapic_eoi();
}

/// The local APIC's spurious interrupt: nothing to do, and no end-of-interrupt.
extern "x86-interrupt" fn lapic_spurious_handler(_stack_frame: InterruptStackFrame) {}

/// Loads the interrupt table on another processor.
pub fn load_idt_secondary() {
    IDT.load();
}

lazy_static! {
    /// The IDT lives in static memory so its address remains valid after
    /// `init_idt` returns and the CPU's IDTR keeps pointing at it.
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        // Set a default handler for all "safe" vectors so unhandled
        // interrupts/exceptions (e.g. NMI on vector 2) don't trigger a
        // double fault. The x86_64 crate panics if set_handler_fn is
        // called on error-code exceptions (8, 10-14, 17, 21, 29, 30, 31),
        // reserved vectors (9, 15, 22-28), or diverging entries (18).
        // Those get specific typed handlers below or use CPU defaults.
        const SAFE_EXCEPTIONS: [u8; 10] = [0, 1, 2, 4, 5, 6, 7, 16, 19, 20];
        // Set handlers for safe exception vectors
        for &vector in &SAFE_EXCEPTIONS {
            idt[vector].set_handler_fn(spurious_handler);
        }
        // Set handlers for all hardware interrupt vectors (32-255)
        for vector in 32..=255u8 {
            idt[vector].set_handler_fn(spurious_handler);
        }

        idt[crate::smp::WAKE].set_handler_fn(wake_handler);
        idt[0xFF].set_handler_fn(lapic_spurious_handler);

        // Override with specific handlers
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault.set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt.page_fault.set_handler_fn(page_fault_handler);

        // Register timer and keyboard IRQ handlers. These are only invoked
        // when the legacy 8259 PIC is initialized (legacy BIOS mode). In UEFI
        // mode no interrupts fire on these vectors, so the handlers are dormant.
        idt[InterruptIndex::Timer.as_u8()]
            .set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_u8()]
            .set_handler_fn(keyboard_interrupt_handler);
        idt[PIC_2_OFFSET + 4].set_handler_fn(mouse_interrupt_handler);

        idt
    };
}

/// Initialise the IDT.
pub fn init_idt() {
    IDT.load();
}

/// handles breakpoint exceptions
extern "x86-interrupt" fn breakpoint_handler(
    stack_frame: InterruptStackFrame)
{
    emergency_println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
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

    emergency_println!("EXCEPTION: PAGE FAULT");
    emergency_println!("Accessed Address: {:?}", Cr2::read());
    emergency_println!("Error Code: {:?}", error_code);
    emergency_println!("{:#?}", stack_frame);
    crate::hlt_loop_noirq();
}

extern "x86-interrupt" fn timer_interrupt_handler(
    _stack_frame: InterruptStackFrame)
{
    time::tick();

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }

    // keep Doom's sound card fed even while the game is busy drawing a frame
    // (a big screen takes longer than the audio buffered ahead)
    static TICKS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
    if TICKS.fetch_add(1, core::sync::atomic::Ordering::Relaxed) % 4 == 0 {
        crate::doom::audio_tick();
    }
    // time slices for the threads (may switch to another thread)
    crate::sched::timer_tick();
}

extern "x86-interrupt" fn keyboard_interrupt_handler(
    _stack_frame: InterruptStackFrame)
{
    use pc_keyboard::{layouts, DecodedKey, HandleControl, KeyCode, KeyState, Keyboard, ScancodeSet1};
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
        input::set_held(key_event.code as usize, key_event.state != KeyState::Up);
        input::doom_key_event(key_event.code, key_event.state == KeyState::Down);
        if let Some(key) = keyboard.process_keyevent(key_event) {
            match key {
                DecodedKey::Unicode(character) => {
                    // ctrl+c also stops a running script
                    if character == '\u{3}' {
                        input::request_interrupt();
                    }
                    crate::graphics::buddy::note_key();
                    input::push_key(Key::Char(character));
                }
                DecodedKey::RawKey(KeyCode::ArrowUp) => input::push_key(Key::Up),
                DecodedKey::RawKey(KeyCode::ArrowDown) => input::push_key(Key::Down),
                DecodedKey::RawKey(KeyCode::ArrowLeft) => input::push_key(Key::Left),
                DecodedKey::RawKey(KeyCode::ArrowRight) => input::push_key(Key::Right),
                DecodedKey::RawKey(KeyCode::PageUp) => input::push_key(Key::PageUp),
                DecodedKey::RawKey(KeyCode::PageDown) => input::push_key(Key::PageDown),
                DecodedKey::RawKey(KeyCode::Home) => input::push_key(Key::Home),
                DecodedKey::RawKey(KeyCode::End) => input::push_key(Key::End),
                DecodedKey::RawKey(KeyCode::Delete) => input::push_key(Key::Delete),
                DecodedKey::RawKey(code) => {
                    use KeyCode as K;
                    let f = match code {
                        K::F1 => 1, K::F2 => 2, K::F3 => 3, K::F4 => 4, K::F5 => 5, K::F6 => 6,
                        K::F7 => 7, K::F8 => 8, K::F9 => 9, K::F10 => 10, K::F11 => 11, K::F12 => 12,
                        _ => 0,
                    };
                    if f > 0 {
                        input::push_key(Key::F(f));
                    }
                }
                DecodedKey::RawKey(_) => {}
            }
        }
    }

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

extern "x86-interrupt" fn mouse_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;

    let byte: u8 = unsafe { Port::new(0x60).read() };
    crate::mouse::handle_byte(byte);
    unsafe {
        PICS.lock().notify_end_of_interrupt(PIC_2_OFFSET + 4);
    }
}

#[test_case]
/// test breakpoint exceptions
fn test_breakpoint_exception() {
    // (manually) invoke a breakpoint exception
    x86_64::instructions::interrupts::int3();
}
