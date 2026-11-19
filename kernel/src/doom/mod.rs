//! DOOM, running inside the kernel.
//!
//! The game itself is id Software's original source (the doomgeneric port of
//! it, GPLv2, in `third_party/doomgeneric`), compiled to a static library by
//! `kernel/build.rs` and linked in. Everything that connects it to this
//! kernel is Rust and lives here:
//!
//! * `libc.rs`: the pieces of the C library the engine calls (memory, strings,
//!   `printf`, files);
//! * the platform functions doomgeneric asks every port to provide: draw a
//!   frame, read the keyboard, tell the time, sleep;
//! * a `setjmp`/`longjmp` pair, so the engine's `exit()` (quit from its menu,
//!   or a fatal error) returns to the shell instead of killing the kernel.
//!
//! The game data (`doom1.wad`, the shareware episode) is read from the
//! persistent partition; save games go to `/doom` in the file system and so
//! survive reboots. The engine keeps its state in C globals, so it can be
//! started once per boot.
//!
//! The engine is only built if Zig is available (see `kernel/build.rs`);
//! without it [`run`] says so.

pub mod libc;
#[cfg(has_doom)]
pub mod audio;

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

/// The engine is not re-entrant: its globals keep their values after a run.
static STARTED: AtomicBool = AtomicBool::new(false);

/// True while one of Doom's menus is open.
pub fn menu_open() -> bool {
    #[cfg(has_doom)]
    unsafe {
        return core::ptr::addr_of!(menuactive).read_volatile() != 0;
    }
    #[allow(unreachable_code)]
    false
}

/// Whether this kernel was built with the Doom engine.
pub fn available() -> bool {
    cfg!(has_doom)
}

/// The game data from the partition (read once when the engine opens it).
pub(crate) fn wad_bytes() -> Option<Vec<u8>> {
    let asset = crate::partition::asset("doom1.wad")?;
    let mut data = alloc::vec![0u8; asset.size as usize];
    let mut done = 0;
    while done < data.len() {
        // read in chunks so a slow disk does not look like a hang
        let end = (done + 256 * 1024).min(data.len());
        let n = asset.read_at(done as u64, &mut data[done..end]);
        if n == 0 {
            return None;
        }
        done += n;
    }
    Some(data)
}

pub(crate) fn sleep_ms(ms: u32) {
    let end = crate::time::uptime_ms() + ms as u64;
    while crate::time::uptime_ms() < end {
        keep_audio_fed();
        x86_64::instructions::hlt();
    }
}

/// Called from the timer interrupt: tops up the sound card (does nothing
/// unless Doom has the card running, and never waits for a lock).
pub fn audio_tick() {
    keep_audio_fed();
}

/// Tops up the sound card's buffers (a no-op without the engine or a card).
fn keep_audio_fed() {
    #[cfg(has_doom)]
    audio::pump();
}

// ---------------------------------------------------------------- exit

/// Saved registers for [`zero_setjmp`] / [`zero_longjmp`]: rbx, rbp, r12-r15,
/// then the stack pointer and the return address.
static mut JUMP: [u64; 8] = [0; 8];
static mut EXIT_CODE: i32 = 0;

core::arch::global_asm!(
    // zero_guarded_call(buf, f): saves the callee-saved registers and the stack
    // pointer in `buf`, then calls `f`. Returns 0 if `f` returns, or 1 if
    // somebody calls zero_longjmp(buf) while `f` is running.
    ".global zero_guarded_call",
    "zero_guarded_call:",
    "mov [rdi + 0], rbx",
    "mov [rdi + 8], rbp",
    "mov [rdi + 16], r12",
    "mov [rdi + 24], r13",
    "mov [rdi + 32], r14",
    "mov [rdi + 40], r15",
    "mov [rdi + 48], rsp",
    "sub rsp, 8",
    "call rsi",
    "add rsp, 8",
    "xor eax, eax",
    "ret",
    ".global zero_longjmp",
    "zero_longjmp:",
    "mov rbx, [rdi + 0]",
    "mov rbp, [rdi + 8]",
    "mov r12, [rdi + 16]",
    "mov r13, [rdi + 24]",
    "mov r14, [rdi + 32]",
    "mov r15, [rdi + 40]",
    "mov rsp, [rdi + 48]",
    "mov eax, 1",
    "ret",
);

extern "C" {
    fn zero_guarded_call(buf: *mut u64, f: extern "C" fn()) -> i32;
    fn zero_longjmp(buf: *mut u64, value: i32) -> !;
}

/// Leaves the engine from wherever it is (called by `exit()` and `abort()`).
pub(crate) unsafe fn exit_doom(code: i32) -> ! {
    EXIT_CODE = code;
    zero_longjmp(core::ptr::addr_of_mut!(JUMP) as *mut u64, 1)
}

// ---------------------------------------------------------------- the engine

#[cfg(has_doom)]
extern "C" {
    static menuactive: i32;
    fn doomgeneric_Create(argc: i32, argv: *mut *mut core::ffi::c_char);
    fn doomgeneric_Tick();
    static mut DG_ScreenBuffer: *mut u32;
}

#[cfg(has_doom)]
static mut ARGV: *mut *mut core::ffi::c_char = core::ptr::null_mut();

/// The engine's main loop; never returns (it leaves through `exit()`).
#[cfg(has_doom)]
extern "C" fn engine_main() {
    unsafe {
        doomgeneric_Create(3, ARGV);
        loop {
            doomgeneric_Tick();
        }
    }
}

/// Plays Doom until the player quits from its menu (or it fails).
#[cfg(has_doom)]
pub fn run() -> Result<(), String> {
    use crate::graphics::buddy::{self, Context};

    if STARTED.swap(true, Ordering::SeqCst) {
        return Err(String::from("Doom keeps its state in global variables, so it can only start once per boot: restart to play again"));
    }
    if crate::partition::asset("doom1.wad").is_none() {
        STARTED.store(false, Ordering::SeqCst);
        return Err(String::from("doom1.wad is not on this disk (it lives in the data partition: run `disk`)"));
    }

    libc::init_arena(24 * 1024 * 1024);
    let mouse = crate::mouse::enable();
    let sound = audio::init();
    crate::serial_println!("[doom] mouse: {}, sound card: {}", if mouse { "yes" } else { "no" }, if sound { "AC97" } else { "none" });
    libc::ERROR_TEXT.lock().clear();
    crate::input::set_doom_active(true);
    buddy::set_context(Context::Doom);

    // `-iwad` names the game data
    static ARG0: &[u8] = b"doom\0";
    static ARG1: &[u8] = b"-iwad\0";
    static ARG2: &[u8] = b"doom1.wad\0";
    static ARG3: &[u8] = b"-nosound\0";
    let mut argv: [*mut core::ffi::c_char; 4] = [ARG0.as_ptr() as *mut _, ARG1.as_ptr() as *mut _, ARG2.as_ptr() as *mut _, core::ptr::null_mut()];

    unsafe {
        ARGV = argv.as_mut_ptr();
        // returns when the engine calls exit()
        zero_guarded_call(core::ptr::addr_of_mut!(JUMP) as *mut u64, engine_main);
    }

    // back from exit()
    audio::shutdown();
    crate::mouse::disable();
    crate::input::set_doom_active(false);
    libc::drop_arena();
    unsafe {
        DG_ScreenBuffer = core::ptr::null_mut();
    }
    buddy::set_context(Context::Shell);
    while crate::input::try_read_key().is_some() {}
    crate::graphics::ui::restore_shell_screen();
    let code = unsafe { EXIT_CODE };
    let errors = libc::ERROR_TEXT.lock().clone();
    if code != 0 && !errors.trim().is_empty() {
        return Err(alloc::format!("Doom stopped ({}): {}", code, errors.trim()));
    }
    Ok(())
}

#[cfg(not(has_doom))]
pub fn run() -> Result<(), String> {
    let _ = &STARTED;
    Err(String::from(
        "this kernel was built without the Doom engine: install Zig (https://ziglang.org/download) and rebuild, see kernel/build.rs",
    ))
}

// ---------------------------------------------------------------- platform functions

/// Destination rectangle and column map for the last frame size, so they are
/// not recomputed every frame.
struct Blit {
    screen: (usize, usize),
    rect: (usize, usize, usize, usize),
    columns: Vec<u32>,
}

static BLIT: spin::Mutex<Option<Blit>> = spin::Mutex::new(None);

#[no_mangle]
pub extern "C" fn DG_Init() {}

#[no_mangle]
pub extern "C" fn DG_SleepMs(ms: u32) {
    sleep_ms(ms);
}

#[no_mangle]
pub extern "C" fn DG_GetTicksMs() -> u32 {
    crate::time::uptime_ms() as u32
}

#[no_mangle]
pub extern "C" fn DG_SetWindowTitle(_title: *const core::ffi::c_char) {}

/// Pops the next key event: returns 1 and fills `pressed` / `key`, or 0.
#[no_mangle]
pub unsafe extern "C" fn DG_GetKey(pressed: *mut i32, key: *mut u8) -> i32 {
    match crate::input::doom_pop() {
        Some((down, code)) => {
            *pressed = down as i32;
            *key = code;
            1
        }
        None => 0,
    }
}

/// Polls the mouse: movement since the last call (up is positive) and buttons.
#[no_mangle]
pub unsafe extern "C" fn DG_GetMouse(dx: *mut i32, dy: *mut i32, buttons: *mut i32) -> i32 {
    match crate::mouse::take() {
        Some((x, y, b)) => {
            *dx = x;
            *dy = y;
            *buttons = b as i32;
            1
        }
        None => 0,
    }
}

/// Shows the 640x400 frame the engine just rendered: stretched to the 4:3
/// shape the original game had on a monitor, centred.
#[no_mangle]
pub extern "C" fn DG_DrawFrame() {
    keep_audio_fed();
    #[cfg(has_doom)]
    unsafe {
        use crate::graphics::display::Display;
        let buffer = DG_ScreenBuffer;
        if buffer.is_null() {
            return;
        }
        let pixels = core::slice::from_raw_parts(buffer, 640 * 400);
        crate::graphics::framebuffer::with(|fb| {
            let (w, h) = (fb.width(), fb.height());
            let mut cache = BLIT.lock();
            let stale = match cache.as_ref() {
                Some(b) => b.screen != (w, h),
                None => true,
            };
            if stale {
                let dw = w.min(h * 4 / 3);
                let dh = dw * 3 / 4;
                let (dx, dy) = ((w - dw) / 2, (h - dh) / 2);
                let columns = (0..dw).map(|x| (x * 640 / dw) as u32).collect();
                fb.clear_to(0);
                *cache = Some(Blit { screen: (w, h), rect: (dx, dy, dw, dh), columns });
            }
            if let Some(b) = cache.as_ref() {
                fb.draw_pixels_scaled(pixels, 640, 400, b.rect.0, b.rect.1, b.rect.2, b.rect.3, &b.columns);
            }
            fb.flush();
        });
    }
}
