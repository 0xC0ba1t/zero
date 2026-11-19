//! The small slice of the C library that Doom needs, written in Rust.
//!
//! The kernel has no libc, and Doom (the C code in `third_party/doomgeneric`)
//! was written for one. This module provides the functions it calls: memory
//! allocation out of a private arena (so everything is released when Doom
//! exits), string and character functions, `printf` and friends, and `FILE`
//! I/O that reads the game data from the disk partition and keeps save games
//! in the kernel's file system.
//!
//! Floating point is deliberately absent: the kernel has no FPU state, so
//! `printf("%f")` prints nothing. Doom is fixed-point and never needs it.

#![allow(non_camel_case_types, non_upper_case_globals, clippy::missing_safety_doc)]

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::ffi::{c_char, c_int, c_long, c_void, VaList};
use core::ptr::{null_mut, NonNull};
use linked_list_allocator::Heap;
use spin::Mutex;

pub type size_t = usize;

// ---------------------------------------------------------------- memory

/// Where `malloc` gets its memory. Set up by [`init_arena`] when Doom starts
/// and dropped (all at once) when it ends.
static ARENA: Mutex<Option<(Heap, Box<[u8]>)>> = Mutex::new(None);

const HEADER: usize = 16;

/// Reserves the engine's memory: `bytes` if the kernel heap can spare it,
/// otherwise as much smaller as still works (big screens use a lot of heap).
pub fn init_arena(bytes: usize) {
    let mut size = bytes;
    let mut memory = loop {
        let mut v: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
        if v.try_reserve_exact(size).is_ok() {
            v.resize(size, 0);
            break v.into_boxed_slice();
        }
        if size <= 10 * 1024 * 1024 {
            panic!("not enough memory for Doom");
        }
        size -= 2 * 1024 * 1024;
    };
    let mut heap = Heap::empty();
    unsafe { heap.init(memory.as_mut_ptr() as usize, size) };
    *ARENA.lock() = Some((heap, memory));
}

pub fn drop_arena() {
    *ARENA.lock() = None;
}

#[no_mangle]
pub unsafe extern "C" fn malloc(size: size_t) -> *mut c_void {
    let mut guard = ARENA.lock();
    let Some((heap, _)) = guard.as_mut() else { return null_mut() };
    let total = size.max(1) + HEADER;
    let Ok(layout) = Layout::from_size_align(total, 16) else { return null_mut() };
    match heap.allocate_first_fit(layout) {
        Ok(block) => {
            let base = block.as_ptr();
            (base as *mut usize).write(total);
            base.add(HEADER) as *mut c_void
        }
        Err(()) => null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    let mut guard = ARENA.lock();
    let Some((heap, _)) = guard.as_mut() else { return };
    let base = (ptr as *mut u8).sub(HEADER);
    let total = (base as *const usize).read();
    if let (Ok(layout), Some(base)) = (Layout::from_size_align(total, 16), NonNull::new(base)) {
        heap.deallocate(base, layout);
    }
}

#[no_mangle]
pub unsafe extern "C" fn calloc(count: size_t, size: size_t) -> *mut c_void {
    let bytes = count.saturating_mul(size);
    let ptr = malloc(bytes);
    if !ptr.is_null() {
        core::ptr::write_bytes(ptr as *mut u8, 0, bytes);
    }
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, size: size_t) -> *mut c_void {
    if ptr.is_null() {
        return malloc(size);
    }
    if size == 0 {
        free(ptr);
        return null_mut();
    }
    let old = ((ptr as *const u8).sub(HEADER) as *const usize).read() - HEADER;
    let new = malloc(size);
    if !new.is_null() {
        core::ptr::copy_nonoverlapping(ptr as *const u8, new as *mut u8, old.min(size));
        free(ptr);
    }
    new
}

// ---------------------------------------------------------------- strings

#[no_mangle]
pub unsafe extern "C" fn strlen(s: *const c_char) -> size_t {
    let mut n = 0;
    while *s.add(n) != 0 {
        n += 1;
    }
    n
}

#[no_mangle]
pub unsafe extern "C" fn strnlen(s: *const c_char, max: size_t) -> size_t {
    let mut n = 0;
    while n < max && *s.add(n) != 0 {
        n += 1;
    }
    n
}

#[no_mangle]
pub unsafe extern "C" fn strcmp(a: *const c_char, b: *const c_char) -> c_int {
    let mut i = 0;
    loop {
        let (x, y) = (*a.add(i) as u8, *b.add(i) as u8);
        if x != y || x == 0 {
            return x as c_int - y as c_int;
        }
        i += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn strncmp(a: *const c_char, b: *const c_char, n: size_t) -> c_int {
    for i in 0..n {
        let (x, y) = (*a.add(i) as u8, *b.add(i) as u8);
        if x != y || x == 0 {
            return x as c_int - y as c_int;
        }
    }
    0
}

fn lower(c: u8) -> u8 {
    if c.is_ascii_uppercase() {
        c + 32
    } else {
        c
    }
}

#[no_mangle]
pub unsafe extern "C" fn strcasecmp(a: *const c_char, b: *const c_char) -> c_int {
    let mut i = 0;
    loop {
        let (x, y) = (lower(*a.add(i) as u8), lower(*b.add(i) as u8));
        if x != y || x == 0 {
            return x as c_int - y as c_int;
        }
        i += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn strncasecmp(a: *const c_char, b: *const c_char, n: size_t) -> c_int {
    for i in 0..n {
        let (x, y) = (lower(*a.add(i) as u8), lower(*b.add(i) as u8));
        if x != y || x == 0 {
            return x as c_int - y as c_int;
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn strcpy(dst: *mut c_char, src: *const c_char) -> *mut c_char {
    let mut i = 0;
    loop {
        let c = *src.add(i);
        *dst.add(i) = c;
        if c == 0 {
            return dst;
        }
        i += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn strncpy(dst: *mut c_char, src: *const c_char, n: size_t) -> *mut c_char {
    let mut i = 0;
    while i < n && *src.add(i) != 0 {
        *dst.add(i) = *src.add(i);
        i += 1;
    }
    while i < n {
        *dst.add(i) = 0;
        i += 1;
    }
    dst
}

#[no_mangle]
pub unsafe extern "C" fn strcat(dst: *mut c_char, src: *const c_char) -> *mut c_char {
    strcpy(dst.add(strlen(dst)), src);
    dst
}

#[no_mangle]
pub unsafe extern "C" fn strncat(dst: *mut c_char, src: *const c_char, n: size_t) -> *mut c_char {
    let end = dst.add(strlen(dst));
    let mut i = 0;
    while i < n && *src.add(i) != 0 {
        *end.add(i) = *src.add(i);
        i += 1;
    }
    *end.add(i) = 0;
    dst
}

#[no_mangle]
pub unsafe extern "C" fn strchr(s: *const c_char, c: c_int) -> *mut c_char {
    let mut p = s;
    loop {
        if *p as u8 == c as u8 {
            return p as *mut c_char;
        }
        if *p == 0 {
            return null_mut();
        }
        p = p.add(1);
    }
}

#[no_mangle]
pub unsafe extern "C" fn strrchr(s: *const c_char, c: c_int) -> *mut c_char {
    let mut found = null_mut();
    let mut p = s;
    loop {
        if *p as u8 == c as u8 {
            found = p as *mut c_char;
        }
        if *p == 0 {
            return found;
        }
        p = p.add(1);
    }
}

#[no_mangle]
pub unsafe extern "C" fn strstr(hay: *const c_char, needle: *const c_char) -> *mut c_char {
    let n = strlen(needle);
    if n == 0 {
        return hay as *mut c_char;
    }
    let mut p = hay;
    while *p != 0 {
        if strncmp(p, needle, n) == 0 {
            return p as *mut c_char;
        }
        p = p.add(1);
    }
    null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn strdup(s: *const c_char) -> *mut c_char {
    let n = strlen(s) + 1;
    let copy = malloc(n) as *mut c_char;
    if !copy.is_null() {
        core::ptr::copy_nonoverlapping(s, copy, n);
    }
    copy
}

#[no_mangle]
pub unsafe extern "C" fn memchr(s: *const c_void, c: c_int, n: size_t) -> *mut c_void {
    let s = s as *const u8;
    for i in 0..n {
        if *s.add(i) == c as u8 {
            return s.add(i) as *mut c_void;
        }
    }
    null_mut()
}

// ---------------------------------------------------------------- ctype and numbers

#[no_mangle]
pub extern "C" fn toupper(c: c_int) -> c_int {
    if (b'a' as c_int..=b'z' as c_int).contains(&c) {
        c - 32
    } else {
        c
    }
}

#[no_mangle]
pub extern "C" fn tolower(c: c_int) -> c_int {
    if (b'A' as c_int..=b'Z' as c_int).contains(&c) {
        c + 32
    } else {
        c
    }
}

macro_rules! ctype {
    ($($name:ident => $test:expr),* $(,)?) => {
        $(#[no_mangle] pub extern "C" fn $name(c: c_int) -> c_int {
            if (0..256).contains(&c) { let b = c as u8; ($test(b)) as c_int } else { 0 }
        })*
    };
}

ctype! {
    isalpha => |b: u8| b.is_ascii_alphabetic(),
    isdigit => |b: u8| b.is_ascii_digit(),
    isalnum => |b: u8| b.is_ascii_alphanumeric(),
    isspace => |b: u8| b == b' ' || (9..=13).contains(&b),
    isupper => |b: u8| b.is_ascii_uppercase(),
    islower => |b: u8| b.is_ascii_lowercase(),
    isprint => |b: u8| (32..127).contains(&b),
    isxdigit => |b: u8| b.is_ascii_hexdigit(),
    ispunct => |b: u8| b.is_ascii_punctuation(),
}

#[no_mangle]
pub extern "C" fn abs(x: c_int) -> c_int {
    x.wrapping_abs()
}

#[no_mangle]
pub extern "C" fn labs(x: c_long) -> c_long {
    x.wrapping_abs()
}

/// Parses an integer the way `strtol` does.
unsafe fn parse_int(s: *const c_char, end: *mut *mut c_char, mut base: c_int) -> i64 {
    let mut p = s;
    while isspace(*p as c_int) != 0 {
        p = p.add(1);
    }
    let mut negative = false;
    if *p as u8 == b'-' || *p as u8 == b'+' {
        negative = *p as u8 == b'-';
        p = p.add(1);
    }
    if (base == 0 || base == 16) && *p as u8 == b'0' && (*p.add(1) as u8 == b'x' || *p.add(1) as u8 == b'X') {
        p = p.add(2);
        base = 16;
    } else if base == 0 && *p as u8 == b'0' {
        base = 8;
    } else if base == 0 {
        base = 10;
    }
    let mut value: i64 = 0;
    loop {
        let c = *p as u8;
        let digit = match c {
            b'0'..=b'9' => (c - b'0') as c_int,
            b'a'..=b'z' => (c - b'a') as c_int + 10,
            b'A'..=b'Z' => (c - b'A') as c_int + 10,
            _ => break,
        };
        if digit >= base {
            break;
        }
        value = value.wrapping_mul(base as i64).wrapping_add(digit as i64);
        p = p.add(1);
    }
    if !end.is_null() {
        *end = p as *mut c_char;
    }
    if negative {
        -value
    } else {
        value
    }
}

#[no_mangle]
pub unsafe extern "C" fn atoi(s: *const c_char) -> c_int {
    parse_int(s, null_mut(), 10) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn atol(s: *const c_char) -> c_long {
    parse_int(s, null_mut(), 10) as c_long
}

#[no_mangle]
pub unsafe extern "C" fn strtol(s: *const c_char, end: *mut *mut c_char, base: c_int) -> c_long {
    parse_int(s, end, base) as c_long
}

#[no_mangle]
pub unsafe extern "C" fn strtoul(s: *const c_char, end: *mut *mut c_char, base: c_int) -> u64 {
    parse_int(s, end, base) as u64
}

/// `qsort` by insertion into a temporary copy (Doom sorts small arrays).
#[no_mangle]
pub unsafe extern "C" fn qsort(base: *mut c_void, n: size_t, size: size_t, cmp: extern "C" fn(*const c_void, *const c_void) -> c_int) {
    let base = base as *mut u8;
    let mut tmp: Vec<u8> = alloc::vec![0; size];
    for i in 1..n {
        core::ptr::copy_nonoverlapping(base.add(i * size), tmp.as_mut_ptr(), size);
        let mut j = i;
        while j > 0 && cmp(base.add((j - 1) * size) as *const c_void, tmp.as_ptr() as *const c_void) > 0 {
            core::ptr::copy_nonoverlapping(base.add((j - 1) * size), base.add(j * size), size);
            j -= 1;
        }
        core::ptr::copy_nonoverlapping(tmp.as_ptr(), base.add(j * size), size);
    }
}

#[no_mangle]
pub extern "C" fn rand() -> c_int {
    static SEED: Mutex<u32> = Mutex::new(0x1234_5678);
    let mut seed = SEED.lock();
    *seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
    ((*seed >> 16) & 0x7FFF) as c_int
}

#[no_mangle]
pub extern "C" fn srand(_seed: u32) {}

static mut ERRNO: c_int = 0;

#[no_mangle]
pub unsafe extern "C" fn __errno_location() -> *mut c_int {
    core::ptr::addr_of_mut!(ERRNO)
}

#[no_mangle]
pub unsafe extern "C" fn getenv(_name: *const c_char) -> *mut c_char {
    null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn __assert_fail(expr: *const c_char, file: *const c_char, line: c_int, _func: *const c_char) -> ! {
    crate::serial_println!("[doom] assertion failed: {} ({}:{})", cstr(expr), cstr(file), line);
    super::exit_doom(-2)
}

// ---------------------------------------------------------------- printf

/// Formats like C's `printf`, sending every byte to `out`. Integers, strings,
/// characters, pointers and `%%` are supported, with flags, width and
/// precision. (No floating point: see the module comment.)
unsafe fn format(out: &mut dyn FnMut(u8), fmt: *const c_char, args: &mut VaList) {
    let mut p = fmt as *const u8;
    while *p != 0 {
        if *p != b'%' {
            out(*p);
            p = p.add(1);
            continue;
        }
        p = p.add(1);
        // flags
        let (mut left, mut zero, mut plus, mut space, mut alt) = (false, false, false, false, false);
        loop {
            match *p {
                b'-' => left = true,
                b'0' => zero = true,
                b'+' => plus = true,
                b' ' => space = true,
                b'#' => alt = true,
                _ => break,
            }
            p = p.add(1);
        }
        // width
        let mut width: usize = 0;
        if *p == b'*' {
            let w = args.next_arg::<c_int>();
            if w < 0 {
                left = true;
            }
            width = w.unsigned_abs() as usize;
            p = p.add(1);
        } else {
            while (*p).is_ascii_digit() {
                width = width * 10 + (*p - b'0') as usize;
                p = p.add(1);
            }
        }
        // precision
        let mut precision: Option<usize> = None;
        if *p == b'.' {
            p = p.add(1);
            let mut n = 0usize;
            if *p == b'*' {
                n = args.next_arg::<c_int>().max(0) as usize;
                p = p.add(1);
            } else {
                while (*p).is_ascii_digit() {
                    n = n * 10 + (*p - b'0') as usize;
                    p = p.add(1);
                }
            }
            precision = Some(n);
        }
        // length
        let mut long = 0;
        loop {
            match *p {
                b'l' | b'z' | b't' | b'j' | b'q' => long += 1,
                b'h' => long -= 1,
                _ => break,
            }
            p = p.add(1);
        }
        let conv = *p;
        if conv == 0 {
            break;
        }
        p = p.add(1);

        let mut body: Vec<u8> = Vec::new();
        let mut sign: Option<u8> = None;
        let mut prefix: &[u8] = b"";
        let mut numeric = false;
        match conv {
            b'%' => body.push(b'%'),
            b'c' => body.push(args.next_arg::<c_int>() as u8),
            b's' => {
                let s = args.next_arg::<*const c_char>();
                if s.is_null() {
                    body.extend_from_slice(b"(null)");
                } else {
                    let mut n = 0;
                    while *s.add(n) != 0 && precision.map_or(true, |max| n < max) {
                        body.push(*s.add(n) as u8);
                        n += 1;
                    }
                }
            }
            b'd' | b'i' => {
                let v: i64 = if long > 0 { args.next_arg::<i64>() } else { args.next_arg::<c_int>() as i64 };
                let v = match long {
                    -1 => v as i16 as i64,
                    -2 => v as i8 as i64,
                    _ => v,
                };
                numeric = true;
                sign = if v < 0 {
                    Some(b'-')
                } else if plus {
                    Some(b'+')
                } else if space {
                    Some(b' ')
                } else {
                    None
                };
                digits(&mut body, v.unsigned_abs(), 10, false);
            }
            b'u' | b'x' | b'X' | b'o' | b'p' => {
                let v: u64 = if conv == b'p' || long > 0 { args.next_arg::<u64>() } else { args.next_arg::<u32>() as u64 };
                let v = match long {
                    -1 => v & 0xFFFF,
                    -2 => v & 0xFF,
                    _ => v,
                };
                numeric = true;
                let base = match conv {
                    b'o' => 8,
                    b'u' => 10,
                    _ => 16,
                };
                if conv == b'p' {
                    prefix = b"0x";
                } else if alt && v != 0 {
                    prefix = match conv {
                        b'x' => b"0x",
                        b'X' => b"0X",
                        b'o' => b"0",
                        _ => b"",
                    };
                }
                digits(&mut body, v, base, conv == b'X');
            }
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => {
                // no floating point in the kernel: consume nothing, print nothing
                body.push(b'0');
            }
            other => {
                body.push(b'%');
                body.push(other);
            }
        }
        // precision of numbers is a minimum number of digits
        if numeric {
            if let Some(min) = precision {
                while body.len() < min {
                    body.insert(0, b'0');
                }
                zero = false;
            }
        }
        let used = body.len() + prefix.len() + sign.is_some() as usize;
        let pad = width.saturating_sub(used);
        if !left && !(zero && numeric) {
            for _ in 0..pad {
                out(b' ');
            }
        }
        if let Some(sign) = sign {
            out(sign);
        }
        for &b in prefix {
            out(b);
        }
        if !left && zero && numeric {
            for _ in 0..pad {
                out(b'0');
            }
        }
        for &b in &body {
            out(b);
        }
        if left {
            for _ in 0..pad {
                out(b' ');
            }
        }
    }
}

fn digits(out: &mut Vec<u8>, mut v: u64, base: u64, upper: bool) {
    if v == 0 {
        out.push(b'0');
        return;
    }
    let start = out.len();
    while v > 0 {
        let d = (v % base) as u8;
        out.push(if d < 10 {
            b'0' + d
        } else if upper {
            b'A' + d - 10
        } else {
            b'a' + d - 10
        });
        v /= base;
    }
    out[start..].reverse();
}

unsafe fn cstr(s: *const c_char) -> String {
    if s.is_null() {
        return String::new();
    }
    let n = strlen(s);
    String::from_utf8_lossy(core::slice::from_raw_parts(s as *const u8, n)).into_owned()
}

/// Writes formatted output into a buffer of `size` bytes (always terminated);
/// returns the length the full result would have had.
unsafe fn format_into(buf: *mut c_char, size: size_t, fmt: *const c_char, args: &mut VaList) -> c_int {
    let mut written = 0usize;
    {
        let mut put = |b: u8| {
            if size > 0 && written + 1 < size {
                *buf.add(written) = b as c_char;
            }
            written += 1;
        };
        format(&mut put, fmt, args);
    }
    if size > 0 {
        *buf.add(written.min(size - 1)) = 0;
    }
    written as c_int
}

#[no_mangle]
pub unsafe extern "C" fn snprintf(buf: *mut c_char, size: size_t, fmt: *const c_char, mut args: ...) -> c_int {
    format_into(buf, size, fmt, &mut args)
}

#[no_mangle]
pub unsafe extern "C" fn vsnprintf(buf: *mut c_char, size: size_t, fmt: *const c_char, mut args: VaList) -> c_int {
    format_into(buf, size, fmt, &mut args)
}

#[no_mangle]
pub unsafe extern "C" fn sprintf(buf: *mut c_char, fmt: *const c_char, mut args: ...) -> c_int {
    format_into(buf, usize::MAX / 2, fmt, &mut args)
}

#[no_mangle]
pub unsafe extern "C" fn vsprintf(buf: *mut c_char, fmt: *const c_char, mut args: VaList) -> c_int {
    format_into(buf, usize::MAX / 2, fmt, &mut args)
}

// ---------------------------------------------------------------- files

/// A `FILE`: what it reads from or writes to.
pub struct File {
    kind: Kind,
    pos: usize,
    eof: bool,
}

enum Kind {
    /// stdout / stderr
    Console { error: bool },
    /// A file read from memory (the game data or a save game).
    Read(Vec<u8>),
    /// A file being written; saved to the file system when closed.
    Write { path: String, data: Vec<u8> },
}

/// The messages Doom wrote to stderr, shown after it exits.
pub static ERROR_TEXT: Mutex<String> = Mutex::new(String::new());

static STDOUT: Mutex<()> = Mutex::new(());
static mut STDOUT_FILE: File = File { kind: Kind::Console { error: false }, pos: 0, eof: false };
static mut STDERR_FILE: File = File { kind: Kind::Console { error: true }, pos: 0, eof: false };

#[no_mangle]
pub static mut stdout: *mut File = unsafe { core::ptr::addr_of_mut!(STDOUT_FILE) };
#[no_mangle]
pub static mut stderr: *mut File = unsafe { core::ptr::addr_of_mut!(STDERR_FILE) };
#[no_mangle]
pub static mut stdin: *mut File = null_mut();

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Reads a whole file for `fopen`: the game data from the partition, or a
/// save game from the file system. Settings files are never found, so Doom
/// always starts from its defaults.
fn read_whole(path: &str) -> Option<Vec<u8>> {
    let name = base_name(path);
    if name.eq_ignore_ascii_case("doom1.wad") || name.eq_ignore_ascii_case("doom.wad") {
        return super::wad_bytes();
    }
    if name.ends_with(".cfg") {
        return None;
    }
    let mut guard = crate::fs::FILESYSTEM.lock();
    let fs = guard.as_mut()?;
    fs.read_file(&alloc::format!("/doom/{}", name)).ok()
}

#[no_mangle]
pub unsafe extern "C" fn fopen(path: *const c_char, mode: *const c_char) -> *mut File {
    let path = cstr(path);
    let mode = cstr(mode);
    let kind = if mode.starts_with('r') {
        match read_whole(&path) {
            Some(data) => Kind::Read(data),
            None => return null_mut(),
        }
    } else {
        Kind::Write { path: String::from(base_name(&path)), data: Vec::new() }
    };
    Box::into_raw(Box::new(File { kind, pos: 0, eof: false }))
}

unsafe fn flush(file: &mut File) {
    if let Kind::Write { path, data } = &file.kind {
        if path.ends_with(".cfg") {
            return;
        }
        let mut guard = crate::fs::FILESYSTEM.lock();
        if let Some(fs) = guard.as_mut() {
            let _ = fs.create_dir("/doom");
            let _ = fs.write_file(&alloc::format!("/doom/{}", path), data.clone());
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn fclose(file: *mut File) -> c_int {
    if file.is_null() {
        return -1;
    }
    flush(&mut *file);
    if file != stdout && file != stderr {
        drop(Box::from_raw(file));
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn fflush(file: *mut File) -> c_int {
    if !file.is_null() {
        flush(&mut *file);
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn fread(buf: *mut c_void, size: size_t, count: size_t, file: *mut File) -> size_t {
    let file = &mut *file;
    let Kind::Read(data) = &file.kind else { return 0 };
    let want = size.saturating_mul(count);
    if size == 0 {
        return 0;
    }
    let available = data.len().saturating_sub(file.pos);
    let n = want.min(available);
    core::ptr::copy_nonoverlapping(data.as_ptr().add(file.pos), buf as *mut u8, n);
    file.pos += n;
    if n < want {
        file.eof = true;
    }
    n / size
}

/// Appends bytes to a file (or sends them to the serial port / error text).
unsafe fn put_bytes(file: &mut File, bytes: &[u8]) {
    match &mut file.kind {
        Kind::Write { data, .. } => data.extend_from_slice(bytes),
        Kind::Console { error } => {
            let _guard = STDOUT.lock();
            let text = String::from_utf8_lossy(bytes);
            crate::serial_print!("{}", text);
            if *error {
                let mut shown = ERROR_TEXT.lock();
                if shown.len() < 2000 {
                    shown.push_str(&text);
                }
            }
        }
        Kind::Read(_) => {}
    }
}

#[no_mangle]
pub unsafe extern "C" fn fwrite(buf: *const c_void, size: size_t, count: size_t, file: *mut File) -> size_t {
    let bytes = core::slice::from_raw_parts(buf as *const u8, size.saturating_mul(count));
    put_bytes(&mut *file, bytes);
    count
}

#[no_mangle]
pub unsafe extern "C" fn fseek(file: *mut File, offset: c_long, whence: c_int) -> c_int {
    let file = &mut *file;
    let len = match &file.kind {
        Kind::Read(data) => data.len() as i64,
        Kind::Write { data, .. } => data.len() as i64,
        Kind::Console { .. } => 0,
    };
    let base = match whence {
        0 => 0,
        1 => file.pos as i64,
        _ => len,
    };
    let target = base + offset as i64;
    if target < 0 {
        return -1;
    }
    file.pos = target as usize;
    file.eof = false;
    0
}

#[no_mangle]
pub unsafe extern "C" fn ftell(file: *mut File) -> c_long {
    let file = &*file;
    match &file.kind {
        Kind::Write { data, .. } => data.len() as c_long,
        _ => file.pos as c_long,
    }
}

#[no_mangle]
pub unsafe extern "C" fn rewind(file: *mut File) {
    fseek(file, 0, 0);
}

#[no_mangle]
pub unsafe extern "C" fn feof(file: *mut File) -> c_int {
    (*file).eof as c_int
}

#[no_mangle]
pub unsafe extern "C" fn ferror(_file: *mut File) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn fgetc(file: *mut File) -> c_int {
    let mut byte = 0u8;
    if fread(&mut byte as *mut u8 as *mut c_void, 1, 1, file) == 1 {
        byte as c_int
    } else {
        -1
    }
}

#[no_mangle]
pub unsafe extern "C" fn getc(file: *mut File) -> c_int {
    fgetc(file)
}

#[no_mangle]
pub unsafe extern "C" fn fputc(c: c_int, file: *mut File) -> c_int {
    put_bytes(&mut *file, &[c as u8]);
    c
}

#[no_mangle]
pub unsafe extern "C" fn putc(c: c_int, file: *mut File) -> c_int {
    fputc(c, file)
}

#[no_mangle]
pub unsafe extern "C" fn putchar(c: c_int) -> c_int {
    fputc(c, stdout)
}

#[no_mangle]
pub unsafe extern "C" fn fputs(s: *const c_char, file: *mut File) -> c_int {
    let n = strlen(s);
    put_bytes(&mut *file, core::slice::from_raw_parts(s as *const u8, n));
    0
}

#[no_mangle]
pub unsafe extern "C" fn puts(s: *const c_char) -> c_int {
    fputs(s, stdout);
    fputc(b'\n' as c_int, stdout);
    0
}

unsafe fn print_to(file: *mut File, fmt: *const c_char, args: &mut VaList) -> c_int {
    let mut bytes: Vec<u8> = Vec::new();
    {
        let mut put = |b: u8| bytes.push(b);
        format(&mut put, fmt, args);
    }
    put_bytes(&mut *file, &bytes);
    bytes.len() as c_int
}

#[no_mangle]
pub unsafe extern "C" fn printf(fmt: *const c_char, mut args: ...) -> c_int {
    print_to(stdout, fmt, &mut args)
}

#[no_mangle]
pub unsafe extern "C" fn vprintf(fmt: *const c_char, mut args: VaList) -> c_int {
    print_to(stdout, fmt, &mut args)
}

#[no_mangle]
pub unsafe extern "C" fn fprintf(file: *mut File, fmt: *const c_char, mut args: ...) -> c_int {
    print_to(file, fmt, &mut args)
}

#[no_mangle]
pub unsafe extern "C" fn vfprintf(file: *mut File, fmt: *const c_char, mut args: VaList) -> c_int {
    print_to(file, fmt, &mut args)
}

/// A very small `sscanf`: `%d`, `%i`, `%x`, `%u` and `%s`, enough for the few
/// places Doom parses numbers.
#[no_mangle]
pub unsafe extern "C" fn sscanf(input: *const c_char, fmt: *const c_char, mut args: ...) -> c_int {
    let (mut s, mut f) = (input as *const u8, fmt as *const u8);
    let mut done = 0;
    while *f != 0 {
        if *f == b'%' {
            f = f.add(1);
            while (*f).is_ascii_digit() {
                f = f.add(1);
            }
            while *s == b' ' || *s == b'\t' || *s == b'\n' {
                s = s.add(1);
            }
            match *f {
                b'd' | b'i' | b'u' | b'x' => {
                    let base = if *f == b'x' { 16 } else if *f == b'i' { 0 } else { 10 };
                    let mut end: *mut c_char = null_mut();
                    let v = parse_int(s as *const c_char, &mut end, base);
                    if end as *const u8 == s {
                        return done;
                    }
                    *args.next_arg::<*mut c_int>() = v as c_int;
                    s = end as *const u8;
                    done += 1;
                }
                b's' => {
                    let out = args.next_arg::<*mut c_char>();
                    let mut n = 0;
                    while *s != 0 && *s != b' ' && *s != b'\n' && *s != b'\t' {
                        *out.add(n) = *s as c_char;
                        n += 1;
                        s = s.add(1);
                    }
                    *out.add(n) = 0;
                    done += 1;
                }
                _ => return done,
            }
            f = f.add(1);
        } else if *f == *s {
            f = f.add(1);
            s = s.add(1);
        } else if (*f).is_ascii_whitespace() {
            f = f.add(1);
        } else {
            return done;
        }
    }
    done
}

#[no_mangle]
pub unsafe extern "C" fn remove(path: *const c_char) -> c_int {
    let name = alloc::format!("/doom/{}", base_name(&cstr(path)));
    let mut guard = crate::fs::FILESYSTEM.lock();
    match guard.as_mut() {
        Some(fs) => {
            if fs.delete(&name).is_ok() {
                0
            } else {
                -1
            }
        }
        None => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn rename(from: *const c_char, to: *const c_char) -> c_int {
    let (from, to) = (alloc::format!("/doom/{}", base_name(&cstr(from))), alloc::format!("/doom/{}", base_name(&cstr(to))));
    let mut guard = crate::fs::FILESYSTEM.lock();
    let Some(fs) = guard.as_mut() else { return -1 };
    match fs.read_file(&from) {
        Ok(data) => {
            let _ = fs.write_file(&to, data);
            let _ = fs.delete(&from);
            0
        }
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn mkdir(_path: *const c_char, _mode: u32) -> c_int {
    0
}

/// `stat` says whether a file exists (the size is filled in for the game data).
#[no_mangle]
pub unsafe extern "C" fn stat(path: *const c_char, buf: *mut c_void) -> c_int {
    let path = cstr(path);
    let size = match read_whole(&path) {
        Some(data) => data.len(),
        None => return -1,
    };
    // struct stat on x86_64 is 144 bytes: st_mode at 24, st_size at 48
    core::ptr::write_bytes(buf as *mut u8, 0, 144);
    (buf as *mut u8).add(24).cast::<u32>().write(0o100644);
    (buf as *mut u8).add(48).cast::<i64>().write(size as i64);
    0
}

#[no_mangle]
pub unsafe extern "C" fn access(path: *const c_char, _mode: c_int) -> c_int {
    if read_whole(&cstr(path)).is_some() {
        0
    } else {
        -1
    }
}

#[no_mangle]
pub unsafe extern "C" fn exit(code: c_int) -> ! {
    super::exit_doom(code)
}

#[no_mangle]
pub unsafe extern "C" fn abort() -> ! {
    super::exit_doom(-3)
}

#[no_mangle]
pub unsafe extern "C" fn atexit(_f: extern "C" fn()) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn system(_cmd: *const c_char) -> c_int {
    -1
}

#[no_mangle]
pub unsafe extern "C" fn time(out: *mut i64) -> i64 {
    let seconds = crate::time::uptime_secs() as i64;
    if !out.is_null() {
        *out = seconds;
    }
    seconds
}

#[no_mangle]
pub unsafe extern "C" fn usleep(us: u32) -> c_int {
    super::sleep_ms(us / 1000);
    0
}
