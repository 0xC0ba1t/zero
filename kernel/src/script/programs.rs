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
        about: "pong against the computer (w/s or arrows, q quits)",
        source: include_str!("../../../programs/pong.zs"),
    },
    Entry {
        name: "tictactoe",
        about: "tic tac toe against the computer (w/s/a/d or arrows, enter to mark, q quits)",
        source: include_str!("../../../programs/tictactoe.zs"),
    },
    Entry {
        name: "snake",
        about: "classic snake arcade game (w/a/s/d or arrows, q quits)",
        source: include_str!("../../../programs/snake.zs"),
    },
    Entry {
        name: "invaders",
        about: "space invaders defender game (a/d or arrows, space to shoot, q quits)",
        source: include_str!("../../../programs/space_invaders.zs"),
    },
    Entry {
        name: "calculator",
        about: "interactive grid calculator (w/a/s/d or arrows, enter to press)",
        source: include_str!("../../../programs/calculator.zs"),
    },
    Entry {
        name: "connect4",
        about: "connect four against the computer (a/d to choose column, enter to drop)",
        source: include_str!("../../../programs/connect_four.zs"),
    },
];

/// Looks up a built-in script by name.
pub fn find(name: &str) -> Option<&'static Entry> {
    PROGRAMS.iter().find(|entry| entry.name == name)
}
