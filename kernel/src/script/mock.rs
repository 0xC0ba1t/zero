//! A fake host that records everything a script does. Used by the tests.

use super::host::Host;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Default)]
pub struct MockHost {
    /// everything printed, one line per `print`
    pub output: String,
    pub plots: Vec<(i32, i32, u8)>,
    pub texts: Vec<(i32, i32, String)>,
    pub colors: Vec<u8>,
    /// keys returned by `key()` and `wait_key()`, oldest first
    pub keys: Vec<i32>,
    pub clears: usize,
    /// total milliseconds slept
    pub slept: u32,
    pub now: u64,
    pub polls: usize,
    /// report an interrupt after this many polls
    pub interrupt_after: Option<usize>,
}

impl MockHost {
    fn next_key(&mut self) -> i32 {
        if self.keys.is_empty() {
            0
        } else {
            self.keys.remove(0)
        }
    }
}

impl Host for MockHost {
    fn clear(&mut self) {
        self.clears += 1;
    }

    fn print(&mut self, text: &str) {
        self.output.push_str(text);
        self.output.push('\n');
    }

    fn text(&mut self, x: i32, y: i32, text: &str) {
        self.texts.push((x, y, String::from(text)));
    }

    fn plot(&mut self, x: i32, y: i32, ch: u8) {
        self.plots.push((x, y, ch));
    }

    fn set_color(&mut self, color: u8) {
        self.colors.push(color);
    }

    fn key(&mut self) -> i32 {
        self.next_key()
    }

    fn wait_key(&mut self) -> i32 {
        self.next_key()
    }

    fn sleep(&mut self, ms: u32) {
        self.slept += ms;
        self.now += ms as u64;
    }

    fn ticks(&mut self) -> u64 {
        self.now
    }

    fn interrupted(&mut self) -> bool {
        self.polls += 1;
        match self.interrupt_after {
            Some(limit) => self.polls > limit,
            None => false,
        }
    }
}
