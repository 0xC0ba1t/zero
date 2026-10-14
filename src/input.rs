//! Keyboard input queue.
//!
//! The keyboard interrupt handler pushes decoded keys into a fixed-size
//! ring buffer and the shell pops them off again. Nothing in here allocates,
//! so it is safe to use from inside an interrupt handler.
