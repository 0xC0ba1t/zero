//! More than one processor.
//!
//! [`start`] wakes the other cores (application processors) with the
//! INIT/SIPI sequence and a small real-mode trampoline that takes each one
//! to 64-bit mode. The extra cores then wait for work: [`parallel_for`] hands
//! a loop over `0..count` to all cores, each taking indexes until none are
//! left. The work must not allocate, lock or print: it runs on cores that
//! know nothing about the scheduler.

use crate::memory;
use alloc::alloc::{alloc_zeroed, Layout};
use alloc::boxed::Box;
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

const TRAMPOLINE: u64 = 0x8000;
const LAPIC_PHYS: u64 = 0xFEE0_0000;
const WAKE_VECTOR: u8 = 0xF0;
const MAX_CPUS: u32 = 16;

static LAPIC: AtomicU64 = AtomicU64::new(0);
static ONLINE: AtomicU32 = AtomicU32::new(0);
static BSP_CR3: AtomicU64 = AtomicU64::new(0);

// ---- the job being worked on ---------------------------------------------------
static JOB_LOCK: Mutex<()> = Mutex::new(());
static JOB_GEN: AtomicUsize = AtomicUsize::new(0);
static JOB_CALL: AtomicUsize = AtomicUsize::new(0);
static JOB_CTX: AtomicUsize = AtomicUsize::new(0);
static JOB_COUNT: AtomicUsize = AtomicUsize::new(0);
static JOB_NEXT: AtomicUsize = AtomicUsize::new(0);
static JOB_DONE: AtomicUsize = AtomicUsize::new(0);

// The trampoline: real mode, then protected mode, then long mode. It is copied
// to physical address 0x8000 and must not use anything outside itself.
core::arch::global_asm!(
    ".section .rodata",
    ".global zero_tramp_start",
    ".global zero_tramp_end",
    ".code16",
    "zero_tramp_start:",
    "cli",
    "cld",
    "xor ax, ax",
    "mov ds, ax",
    "mov es, ax",
    "mov ss, ax",
    // lgdt [0x8000 + gdt_ptr]
    ".byte 0x0F, 0x01, 0x16",
    ".word 0x8000 + (zero_tramp_gdt_ptr - zero_tramp_start)",
    "mov eax, cr0",
    "or eax, 1",
    "mov cr0, eax",
    // far jump to 32-bit code (selector 0x08)
    ".byte 0x66, 0xEA",
    ".long 0x8000 + (zero_tramp_pm - zero_tramp_start)",
    ".word 0x08",
    ".code32",
    "zero_tramp_pm:",
    "mov ax, 0x10",
    "mov ds, ax",
    "mov es, ax",
    "mov ss, ax",
    "mov eax, cr4",
    "or eax, 0x20",
    "mov cr4, eax",
    "mov eax, dword ptr [ZERO_TRAMP_CR3]",
    "mov cr3, eax",
    "mov ecx, 0xC0000080",
    "rdmsr",
    "or eax, 0x900",
    "wrmsr",
    "mov eax, cr0",
    "or eax, 0x80000001",
    "mov cr0, eax",
    // far jump to 64-bit code (selector 0x18)
    ".byte 0xEA",
    ".long 0x8000 + (zero_tramp_lm - zero_tramp_start)",
    ".word 0x18",
    ".code64",
    "zero_tramp_lm:",
    "mov rsp, qword ptr [ZERO_TRAMP_STACK]",
    "mov rax, qword ptr [ZERO_TRAMP_ENTRY]",
    "call rax",
    "2: hlt",
    "jmp 2b",
    ".align 8",
    "zero_tramp_gdt:",
    ".quad 0",
    ".quad 0x00CF9A000000FFFF",
    ".quad 0x00CF92000000FFFF",
    ".quad 0x00AF9A000000FFFF",
    "zero_tramp_gdt_ptr:",
    ".word 31",
    ".long 0x8000 + (zero_tramp_gdt - zero_tramp_start)",
    ".align 8",
    "zero_tramp_cr3:",
    ".quad 0",
    "zero_tramp_stack:",
    ".quad 0",
    "zero_tramp_entry:",
    ".quad 0",
    "zero_tramp_end:",
    ".set ZERO_TRAMP_CR3, 0x8000 + (zero_tramp_cr3 - zero_tramp_start)",
    ".set ZERO_TRAMP_STACK, 0x8000 + (zero_tramp_stack - zero_tramp_start)",
    ".set ZERO_TRAMP_ENTRY, 0x8000 + (zero_tramp_entry - zero_tramp_start)",
    ".code64",
    ".section .text",
);

extern "C" {
    static zero_tramp_start: u8;
    static zero_tramp_end: u8;
}

// ---- local APIC -----------------------------------------------------------------

fn lapic_read(reg: usize) -> u32 {
    let base = LAPIC.load(Ordering::Relaxed);
    unsafe { core::ptr::read_volatile((base + reg as u64) as *const u32) }
}

fn lapic_write(reg: usize, value: u32) {
    let base = LAPIC.load(Ordering::Relaxed);
    unsafe { core::ptr::write_volatile((base + reg as u64) as *mut u32, value) }
}

/// Tells the local APIC an interrupt is done (for the wake-up interrupt).
pub fn lapic_eoi() {
    lapic_write(0xB0, 0);
}

fn send_ipi(apic_id: u32, low: u32) {
    lapic_write(0x310, apic_id << 24);
    lapic_write(0x300, low);
    for _ in 0..100_000 {
        if lapic_read(0x300) & 0x1000 == 0 {
            break;
        }
    }
}

fn delay_ms(ms: u64) {
    let end = crate::time::uptime_ms() + ms;
    while crate::time::uptime_ms() < end {
        core::hint::spin_loop();
    }
}

/// How many processors are running kernel code.
pub fn cpu_count() -> usize {
    1 + ONLINE.load(Ordering::Relaxed) as usize
}

/// Wakes the other cores. Returns how many processors there are in total.
pub fn start() -> usize {
    if LAPIC.load(Ordering::Relaxed) != 0 {
        return cpu_count();
    }
    // where the local APIC registers are
    if !memory::page_is_free(TRAMPOLINE) {
        crate::serial_println!("[smp] page 0x8000 is not free memory: staying on one processor");
        return 1;
    }
    let base_msr = unsafe { x86_64::registers::model_specific::Msr::new(0x1B).read() };
    if base_msr & (1 << 11) == 0 {
        return 1;
    }
    let phys = base_msr & 0xFFFF_F000;
    let phys = if phys == 0 { LAPIC_PHYS } else { phys };
    let Some(virt) = memory::map_mmio(phys, 0x1000) else { return 1 };
    LAPIC.store(virt as u64, Ordering::SeqCst);

    // make sure the APIC is on without cutting off the old interrupt controller
    let svr = lapic_read(0xF0);
    if svr & 0x100 == 0 {
        lapic_write(0xF0, svr | 0x1FF);
        lapic_write(0x350, 0x700); // LINT0: external interrupts keep arriving
    }
    let bsp_id = lapic_read(0x20) >> 24;

    // page tables for the new core's first steps: the kernel's own, plus the
    // first 2 MiB mapped to themselves so the trampoline can run
    let offset = memory::PHYS_OFFSET.load(Ordering::Relaxed);
    let (cr3_frame, _) = x86_64::registers::control::Cr3::read();
    let cr3 = cr3_frame.start_address().as_u64();
    BSP_CR3.store(cr3, Ordering::SeqCst);
    let page = Layout::from_size_align(4096, 4096).unwrap();
    let (pml4, pdpt, pd) = unsafe { (alloc_zeroed(page) as *mut u64, alloc_zeroed(page) as *mut u64, alloc_zeroed(page) as *mut u64) };
    let Some(pml4_phys) = memory::virt_to_phys(pml4 as u64) else { return 1 };
    let (Some(pdpt_phys), Some(pd_phys)) = (memory::virt_to_phys(pdpt as u64), memory::virt_to_phys(pd as u64)) else { return 1 };
    if pml4_phys > u32::MAX as u64 {
        return 1;
    }
    unsafe {
        let src = (offset + cr3) as *const u64;
        for i in 0..512 {
            pml4.add(i).write(src.add(i).read());
        }
        let first = src.read();
        if first & 1 != 0 {
            let src_pdpt = (offset + (first & 0x000F_FFFF_FFFF_F000)) as *const u64;
            for i in 0..512 {
                pdpt.add(i).write(src_pdpt.add(i).read());
            }
        }
        pdpt.write(pd_phys | 3);
        pd.write(0x83); // 2 MiB, present, writable, huge
        pml4.write(pdpt_phys | 3);
    }

    // copy the trampoline into low memory
    let start_addr = unsafe { &zero_tramp_start as *const u8 as usize };
    let end_addr = unsafe { &zero_tramp_end as *const u8 as usize };
    let size = end_addr - start_addr;
    let dest = (offset + TRAMPOLINE) as *mut u8;
    unsafe { core::ptr::copy_nonoverlapping(start_addr as *const u8, dest, size) };
    let data_off = size - 24;
    let data = unsafe { dest.add(data_off) as *mut u64 };

    for apic_id in 0..MAX_CPUS {
        if apic_id == bsp_id {
            continue;
        }
        let stack = Box::leak(alloc::vec![0u8; 128 * 1024].into_boxed_slice());
        let top = (stack.as_ptr() as u64 + stack.len() as u64) & !15;
        unsafe {
            data.write_volatile(pml4_phys);
            data.add(1).write_volatile(top - 8);
            data.add(2).write_volatile(ap_main as usize as u64);
        }
        let before = ONLINE.load(Ordering::SeqCst);
        send_ipi(apic_id, 0x0000_4500); // INIT
        delay_ms(10);
        send_ipi(apic_id, 0x0000_4600 | (TRAMPOLINE >> 12) as u32); // start-up
        delay_ms(2);
        send_ipi(apic_id, 0x0000_4600 | (TRAMPOLINE >> 12) as u32);
        let end = crate::time::uptime_ms() + 100;
        while ONLINE.load(Ordering::SeqCst) == before && crate::time::uptime_ms() < end {
            core::hint::spin_loop();
        }
        if ONLINE.load(Ordering::SeqCst) == before {
            break; // no such processor
        }
        // the next processor must not read this one's stack pointer too early
        delay_ms(2);
    }
    cpu_count()
}

/// What a new core runs once the trampoline has put it in 64-bit mode.
extern "C" fn ap_main() -> ! {
    // the kernel's own page tables, descriptor tables and interrupt table
    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) BSP_CR3.load(Ordering::SeqCst), options(nostack));
    }
    crate::gdt::init_secondary();
    crate::interrupts::load_idt_secondary();
    // its own local APIC, to receive the wake-up interrupt
    lapic_write(0xF0, lapic_read(0xF0) | 0x1FF);
    ONLINE.fetch_add(1, Ordering::SeqCst);

    let mut seen = JOB_GEN.load(Ordering::SeqCst);
    loop {
        x86_64::instructions::interrupts::disable();
        let gen = JOB_GEN.load(Ordering::SeqCst);
        if gen != seen {
            seen = gen;
            x86_64::instructions::interrupts::enable();
            work();
        } else {
            // sleep until the wake-up interrupt
            x86_64::instructions::interrupts::enable_and_hlt();
        }
    }
}

/// Takes loop indexes from the current job until none are left.
fn work() {
    let call = JOB_CALL.load(Ordering::SeqCst);
    let ctx = JOB_CTX.load(Ordering::SeqCst);
    let count = JOB_COUNT.load(Ordering::SeqCst);
    if call == 0 {
        return;
    }
    let f: fn(usize, usize) = unsafe { core::mem::transmute(call) };
    loop {
        let i = JOB_NEXT.fetch_add(1, Ordering::SeqCst);
        if i >= count {
            break;
        }
        f(ctx, i);
        JOB_DONE.fetch_add(1, Ordering::SeqCst);
    }
}

fn call_closure(ctx: usize, i: usize) {
    let f = unsafe { &*(ctx as *const &(dyn Fn(usize) + Sync)) };
    f(i);
}

/// Runs `f(0)`, `f(1)`, ... `f(count - 1)` spread over all cores and
/// returns when all are done. `f` must not allocate, take locks or print.
pub fn parallel_for(count: usize, f: &(dyn Fn(usize) + Sync)) {
    if count == 0 {
        return;
    }
    let guard = if cpu_count() > 1 && count > 1 { JOB_LOCK.try_lock() } else { None };
    let Some(_guard) = guard else {
        for i in 0..count {
            f(i);
        }
        return;
    };
    let ctx = &f as *const &(dyn Fn(usize) + Sync) as usize;
    JOB_CALL.store(call_closure as usize, Ordering::SeqCst);
    JOB_CTX.store(ctx, Ordering::SeqCst);
    JOB_COUNT.store(count, Ordering::SeqCst);
    JOB_NEXT.store(0, Ordering::SeqCst);
    JOB_DONE.store(0, Ordering::SeqCst);
    JOB_GEN.fetch_add(1, Ordering::SeqCst);
    // wake every other core: "all excluding self", fixed delivery
    lapic_write(0x300, 0x000C_4000 | WAKE_VECTOR as u32);
    work();
    while JOB_DONE.load(Ordering::SeqCst) < count {
        core::hint::spin_loop();
    }
    JOB_CALL.store(0, Ordering::SeqCst);
}

pub const WAKE: u8 = WAKE_VECTOR;
