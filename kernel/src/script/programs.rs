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
pub static PROGRAMS: &[Entry] = &[
    Entry {
        name: "hello",
        about: "say hello",
        source: include_str!("../../../programs/hello.zs"),
    },
    Entry {
        name: "fizzbuzz",
        about: "fizzbuzz from 1 to 30",
        source: include_str!("../../../programs/fizzbuzz.zs"),
    },
    Entry {
        name: "countdown",
        about: "count down from 5 with sleep",
        source: include_str!("../../../programs/countdown.zs"),
    },
    Entry {
        name: "bounce",
        about: "a bouncing ball (q to stop)",
        source: include_str!("../../../programs/bounce.zs"),
    },
    Entry {
        name: "pong",
        about: "pong with smooth paddles and a difficulty setting",
        source: include_str!("../../../programs/pong.zs"),
    },
    Entry {
        name: "tictactoe",
        about: "tic tac toe; the hard computer plays perfectly",
        source: include_str!("../../../programs/tictactoe.zs"),
    },
    Entry {
        name: "snake",
        about: "snake with three difficulty levels",
        source: include_str!("../../../programs/snake.zs"),
    },
    Entry {
        name: "invaders",
        about: "space invaders: bunkers, a saucer, waves, 3 levels",
        source: include_str!("../../../programs/space_invaders.zs"),
    },
    Entry {
        name: "calculator",
        about: "a calculator you can type into or point and press",
        source: include_str!("../../../programs/calculator.zs"),
    },
    Entry {
        name: "connect4",
        about: "connect four against a computer that looks ahead",
        source: include_str!("../../../programs/connect_four.zs"),
    },
];

/// Looks up a built-in script by name.
pub fn find(name: &str) -> Option<&'static Entry> {
    PROGRAMS.iter().find(|entry| entry.name == name)
}
