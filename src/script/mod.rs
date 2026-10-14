//! zs, a tiny scripting language that runs inside the kernel.
//!
//! A script is compiled into a flat list of statements and then executed by a
//! small virtual machine. The machine never touches the hardware itself: all
//! input and output goes through the `host::Host` trait, which is what lets
//! the interpreter be tested with a fake host.
