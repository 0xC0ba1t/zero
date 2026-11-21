//! Threads.
//!
//! Every application (the shell, a game, Doom, the desktop itself) runs in
//! its own kernel thread with its own stack. Threads switch in two ways:
//!
//! * politely: blocking calls (waiting for a key, sleeping) call
//!   [`yield_now`] / [`yield_idle`], which hand the CPU to the next thread;
//! * by force: the timer interrupt calls [`timer_tick`], which takes the CPU
//!   away from a thread that has used up its time slice (preemption), so one
//!   busy program cannot freeze the others.
//!
//! The scheduler never allocates while interrupts are off (the heap lock may
//! be held by the thread that was interrupted), so the thread table has a
//! fixed capacity and dead threads are freed by [`reap`] from a normal
//! thread.

use crate::graphics::framebuffer::Framebuffer;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;
use x86_64::instructions::interrupts;

core::arch::global_asm!(
    // zero_switch(save: *mut u64, to: u64): pushes the callee-saved registers
    // and the flags on the current stack, stores the stack pointer in *save,
    // switches to the stack `to` and pops the same things from it.
    ".global zero_switch",
    "zero_switch:",
    "pushfq",
    "push rbp",
    "push rbx",
    "push r12",
    "push r13",
    "push r14",
    "push r15",
    "mov [rdi], rsp",
    "mov rsp, rsi",
    "pop r15",
    "pop r14",
    "pop r13",
    "pop r12",
    "pop rbx",
    "pop rbp",
    "popfq",
    "ret",
);

extern "C" {
    fn zero_switch(save: *mut u64, to: u64);
}

/// What a thread owns besides its stack: the window it draws on and the
/// queue its input arrives in.
pub struct Context {
    pub surface: Arc<Mutex<Framebuffer>>,
    pub events: Arc<Mutex<alloc::collections::VecDeque<crate::gui::Event>>>,
    /// The window has the keyboard.
    pub focused: AtomicBool,
    /// The user closed the window: the program should finish.
    pub closed: AtomicBool,
    /// The window is on screen (not minimized).
    pub visible: AtomicBool,
    /// The program wants raw mouse movement (a game that grabs the mouse).
    pub raw_mouse: AtomicBool,
    /// The window was resized since the program last looked.
    pub resized: AtomicBool,
    /// The window holds the shell (its text console re-lays itself out on resize).
    pub terminal: AtomicBool,
    /// Title shown by the desktop.
    pub title: Mutex<alloc::string::String>,
    /// Mouse pointer shape the app wants (see `gui::Cursor`).
    pub cursor: core::sync::atomic::AtomicU8,
    /// The app's menus for the menu bar: (name, item labels; "-" separates).
    pub menus: Mutex<Vec<(alloc::string::String, Vec<alloc::string::String>)>>,
    /// What the app was started with (a file to open).
    pub arg: Mutex<Option<alloc::string::String>>,
    /// The app asks for a new content size.
    pub want_size: Mutex<Option<(usize, usize)>>,
}

impl Context {
    pub fn new(surface: Framebuffer) -> Arc<Context> {
        Arc::new(Context {
            surface: Arc::new(Mutex::new(surface)),
            events: Arc::new(Mutex::new(alloc::collections::VecDeque::new())),
            focused: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            visible: AtomicBool::new(true),
            raw_mouse: AtomicBool::new(false),
            resized: AtomicBool::new(false),
            terminal: AtomicBool::new(false),
            title: Mutex::new(alloc::string::String::new()),
            cursor: core::sync::atomic::AtomicU8::new(0),
            menus: Mutex::new(Vec::new()),
            arg: Mutex::new(None),
            want_size: Mutex::new(None),
        })
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }
}

/// True if the calling thread has a window and the user closed it.
pub fn window_closed() -> bool {
    context().map(|c| c.is_closed()).unwrap_or(false)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Ready,
    Dead,
}

struct Thread {
    id: usize,
    name: &'static str,
    state: State,
    rsp: u64,
    #[allow(dead_code)]
    stack: Vec<u8>,
    entry: Option<Box<dyn FnOnce() + Send>>,
    context: Option<Arc<Context>>,
}

struct Sched {
    threads: Vec<Box<Thread>>,
    current: usize,
    idle_streak: usize,
    slice: u32,
    next_id: usize,
}

const MAX_THREADS: usize = 64;
/// Milliseconds a thread may run before the timer takes the CPU away.
const SLICE_MS: u32 = 8;

static mut SCHED: Option<Sched> = None;
static PREEMPT: AtomicBool = AtomicBool::new(false);
static SWITCHES: AtomicU64 = AtomicU64::new(0);

#[allow(static_mut_refs)]
fn sched() -> Option<&'static mut Sched> {
    unsafe { SCHED.as_mut() }
}

/// Makes the code that is running now into the first thread.
pub fn init() {
    let mut threads = Vec::with_capacity(MAX_THREADS);
    threads.push(Box::new(Thread {
        id: 0,
        name: "kernel",
        state: State::Ready,
        rsp: 0,
        stack: Vec::new(),
        entry: None,
        context: None,
    }));
    interrupts::without_interrupts(|| unsafe {
        SCHED = Some(Sched { threads, current: 0, idle_streak: 0, slice: 0, next_id: 1 });
    });
}

/// Lets the timer interrupt switch threads by force.
pub fn enable_preemption(on: bool) {
    PREEMPT.store(on, Ordering::SeqCst);
}

/// How many thread switches have happened (for `top`-style displays).
pub fn switches() -> u64 {
    SWITCHES.load(Ordering::Relaxed)
}

extern "C" fn thread_start() -> ! {
    // runs on the new thread's own stack, with interrupts on
    interrupts::enable();
    let entry = interrupts::without_interrupts(|| {
        let s = sched().unwrap();
        let cur = s.current;
        s.threads[cur].entry.take()
    });
    if let Some(f) = entry {
        f();
    }
    exit()
}

/// Starts a thread. `stack_kib` is the size of its stack.
pub fn spawn(name: &'static str, stack_kib: usize, f: impl FnOnce() + Send + 'static) -> usize {
    spawn_with(name, stack_kib, None, f)
}

/// Starts a thread that draws on a window and receives its input.
pub fn spawn_with(name: &'static str, stack_kib: usize, context: Option<Arc<Context>>, f: impl FnOnce() + Send + 'static) -> usize {
    let mut stack = alloc::vec![0u8; stack_kib * 1024];
    let top = (stack.as_mut_ptr() as usize + stack.len()) & !15;
    // what zero_switch will pop: r15 r14 r13 r12 rbx rbp, flags, then `ret` into thread_start
    let rsp = top - 8 * 8;
    unsafe {
        let p = rsp as *mut u64;
        for i in 0..6 {
            p.add(i).write(0);
        }
        p.add(6).write(0x202); // flags: interrupts enabled
        p.add(7).write(thread_start as usize as u64);
    }
    let mut thread = Box::new(Thread {
        id: 0,
        name,
        state: State::Ready,
        rsp: rsp as u64,
        stack,
        entry: Some(Box::new(f)),
        context,
    });
    interrupts::without_interrupts(|| {
        let s = sched().expect("scheduler not started");
        assert!(s.threads.len() < MAX_THREADS, "too many threads");
        thread.id = s.next_id;
        s.next_id += 1;
        let id = thread.id;
        s.threads.push(thread);
        id
    })
}

/// Picks the next ready thread and switches to it. Interrupts must be off.
fn switch_next() {
    let Some(s) = sched() else { return };
    let n = s.threads.len();
    let mut next = None;
    for i in 1..=n {
        let j = (s.current + i) % n;
        if s.threads[j].state == State::Ready {
            next = Some(j);
            break;
        }
    }
    let Some(j) = next else { return };
    if j == s.current {
        return;
    }
    let old = s.current;
    s.current = j;
    SWITCHES.fetch_add(1, Ordering::Relaxed);
    let save: *mut u64 = &mut s.threads[old].rsp;
    let to = s.threads[j].rsp;
    unsafe { zero_switch(save, to) };
}

/// Gives the CPU to the next thread; this thread stays ready.
pub fn yield_now() {
    let was = interrupts::are_enabled();
    interrupts::disable();
    if let Some(s) = sched() {
        s.idle_streak = 0;
    }
    switch_next();
    if was {
        interrupts::enable();
    }
}

/// Yields at most once per millisecond: cheap enough to call from a hot loop.
pub fn yield_if_due() {
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = crate::time::uptime_ms();
    if LAST.swap(now, Ordering::Relaxed) != now {
        yield_now();
    }
}

/// Like [`yield_now`], for a thread that is only waiting: if every thread is
/// waiting, the CPU sleeps until the next interrupt instead of spinning.
pub fn yield_idle() {
    let was = interrupts::are_enabled();
    interrupts::disable();
    let mut sleep = false;
    if let Some(s) = sched() {
        s.idle_streak += 1;
        let live = s.threads.iter().filter(|t| t.state == State::Ready).count();
        if s.idle_streak >= live {
            s.idle_streak = 0;
            sleep = true;
        }
    }
    if sleep {
        // enables interrupts and halts; the timer wakes us at the latest
        interrupts::enable_and_hlt();
        interrupts::disable();
    }
    switch_next();
    if was {
        interrupts::enable();
    }
}

/// Something useful happened: the system is not idle.
pub fn note_progress() {
    interrupts::without_interrupts(|| {
        if let Some(s) = sched() {
            s.idle_streak = 0;
        }
    });
}

/// Waits for `ms` milliseconds, letting the other threads run.
pub fn sleep_ms(ms: u64) {
    let end = crate::time::uptime_ms() + ms;
    while crate::time::uptime_ms() < end {
        yield_idle();
    }
}

/// Ends the calling thread.
pub fn exit() -> ! {
    interrupts::disable();
    if let Some(s) = sched() {
        let cur = s.current;
        s.threads[cur].state = State::Dead;
    }
    loop {
        switch_next();
        interrupts::enable_and_hlt();
        interrupts::disable();
    }
}

/// Called by the timer interrupt (interrupts are off): takes the CPU away
/// from a thread that has had its share.
pub fn timer_tick() {
    if !PREEMPT.load(Ordering::Relaxed) {
        return;
    }
    let Some(s) = sched() else { return };
    s.slice += 1;
    if s.slice >= SLICE_MS {
        s.slice = 0;
        s.idle_streak = 0;
        switch_next();
    }
}

/// Frees the stacks of finished threads. Call from a normal thread.
pub fn reap() {
    let dead: Vec<Box<Thread>> = interrupts::without_interrupts(|| {
        let Some(s) = sched() else { return Vec::new() };
        let mut out = Vec::new();
        let mut i = 0;
        while i < s.threads.len() {
            if s.threads[i].state == State::Dead && i != s.current {
                // no allocation: `swap_remove` only moves a pointer
                let t = s.threads.swap_remove(i);
                if s.current == s.threads.len() {
                    s.current = i;
                }
                out.push(t);
            } else {
                i += 1;
            }
        }
        out
    });
    drop(dead);
}

/// The window context of the calling thread, if it has one.
pub fn context() -> Option<Arc<Context>> {
    interrupts::without_interrupts(|| {
        let s = sched()?;
        s.threads[s.current].context.clone()
    })
}

/// Gives the calling thread a window context (or takes it away).
pub fn set_context(context: Option<Arc<Context>>) {
    interrupts::without_interrupts(|| {
        if let Some(s) = sched() {
            let cur = s.current;
            s.threads[cur].context = context;
        }
    });
}

/// How many threads are alive.
pub fn count() -> usize {
    interrupts::without_interrupts(|| sched().map(|s| s.threads.iter().filter(|t| t.state == State::Ready).count()).unwrap_or(0))
}

/// Names of the live threads, for a task list.
pub fn names() -> Vec<(usize, &'static str)> {
    let mut out = Vec::with_capacity(MAX_THREADS);
    interrupts::without_interrupts(|| {
        if let Some(s) = sched() {
            for t in s.threads.iter().filter(|t| t.state == State::Ready) {
                out.push((t.id, t.name));
            }
        }
    });
    out
}
