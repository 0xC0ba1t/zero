//! Scripts that are built into the kernel.
//!
//! There is no file system yet, so programs live in the `programs/` directory
//! of the source tree and are embedded with `include_str!`. To add one, drop a
//! `.zs` file there and list it below.

/// A built-in script.
pub struct Entry {
    pub name: &'static str,
    pub about: &'static str,
    pub source: &'static str,
}

/// Every built-in script.
pub static PROGRAMS: &[Entry] = &[Entry {
    name: "hello",
    about: "say hello",
    source: include_str!("../../programs/hello.zs"),
}, Entry {
    name: "fizzbuzz",
    about: "fizzbuzz from 1 to 30",
    source: include_str!("../../programs/fizzbuzz.zs"),
}, Entry {
    name: "countdown",
    about: "count down from 5 with sleep",
    source: include_str!("../../programs/countdown.zs"),
}, Entry {
    name: "bounce",
    about: "a bouncing ball (q to stop)",
    source: include_str!("../../programs/bounce.zs"),
}, Entry {
    name: "pong",
    about: "pong against the computer (w/s or arrows, q quits)",
    source: include_str!("../../programs/pong.zs"),
}];

/// Looks up a built-in script by name.
pub fn find(name: &str) -> Option<&'static Entry> {
    PROGRAMS.iter().find(|entry| entry.name == name)
}
