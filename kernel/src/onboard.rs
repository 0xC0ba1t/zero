//! The welcome tour: Bit introduces itself, asks your name, shows you around
//! and lets you steer it. It runs the first time the system boots from a disk
//! (and any time with the `onboard` command).

use crate::graphics::buddy::{self, Activity, Context, Mood};
use crate::graphics::display::Display;
use crate::graphics::font::Font;
use crate::graphics::framebuffer::{self, Framebuffer};
use crate::graphics::ui;
use crate::input::{self, Key};
use crate::settings;
use alloc::string::String;
use noto_sans_mono_bitmap::RasterHeight;

const BG: u32 = 0x21252B;
const CARD: u32 = 0x282C34;

/// One card of the tour: a title, some lines, and what Bit does meanwhile.
struct Card {
    title: &'static str,
    lines: &'static [&'static str],
    bit: Activity,
    mood: Mood,
    say: &'static str,
}

const TOUR: [Card; 6] = [
    Card {
        title: "The shell",
        lines: &[
            "Type commands at the prompt; `help` lists them all.",
            "Tab completes commands, arguments and file names.",
            "Up and down walk through your history,",
            "page up and page down scroll the output.",
        ],
        bit: Activity::TypeLaptop,
        mood: Mood::Happy,
        say: "i'll type along with you!",
    },
    Card {
        title: "Files that stay",
        lines: &[
            "Everything you create is saved to the disk:",
            "  write note.txt hello      mkdir projects",
            "  edit note.txt             cat note.txt",
            "It is still there after a reboot.",
        ],
        bit: Activity::ReadBook,
        mood: Mood::Curious,
        say: "i never forget a file",
    },
    Card {
        title: "Games",
        lines: &[
            "`programs` lists the built-in games: pong, snake,",
            "invaders, tic-tac-toe, connect four, a calculator.",
            "Every game asks for a difficulty first.",
            "And yes, there is `doom`.",
        ],
        bit: Activity::PlayGame,
        mood: Mood::Excited,
        say: "headset on. let's go",
    },
    Card {
        title: "3D and screen",
        lines: &[
            "`3d` runs the demos (torus, terrain, a starfield...).",
            "`res` changes the resolution, `screen` shows it,",
            "`bench` and `zsbench` measure how fast things run.",
        ],
        bit: Activity::Stargaze,
        mood: Mood::Excited,
        say: "so many pixels!",
    },
    Card {
        title: "Programming",
        lines: &[
            "zs is the built-in language: `exec \"print 6 * 7\"`.",
            "Look at a game's source with `src pong`.",
            "Scripts can draw, read keys and use arrays.",
        ],
        bit: Activity::Debug,
        mood: Mood::Curious,
        say: "found a bug! ...no, a feature",
    },
    Card {
        title: "Me",
        lines: &[
            "I live in the status bar. I watch what you do,",
            "type along, play along, nap when it is quiet.",
            "`buddy` shows what I'm up to and lets you ask for tricks.",
        ],
        bit: Activity::Wave,
        mood: Mood::Happy,
        say: "that's me!",
    },
];

/// Word-wraps `text` into `max_w` pixels and draws it; returns the next y.
fn paragraph(fb: &mut Framebuffer, font: &Font, x: usize, mut y: usize, max_w: usize, text: &str, fg: u32, bg: u32) -> usize {
    let per_line = (max_w / font.width()).max(8);
    let mut line = String::new();
    for word in text.split(' ') {
        if !line.is_empty() && line.len() + 1 + word.len() > per_line {
            font.draw_text(fb, x, y, &line, fg, bg);
            y += font.height() + 8;
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        font.draw_text(fb, x, y, &line, fg, bg);
        y += font.height() + 8;
    }
    y
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Welcome,
    Name,
    Chatty,
    Tour(usize),
    Steer,
    Done,
}

struct State {
    step: Step,
    name: String,
    chatty: usize,
    steer_moves: u32,
}

/// Runs the tour. Returns when it is finished (or skipped with escape).
pub fn run() {
    buddy::set_context(Context::Onboarding);
    buddy::greet();
    let mut st = State { step: Step::Welcome, name: String::new(), chatty: 1, steer_moves: 0 };
    enter_step(&st);
    let mut last = crate::time::uptime_ms();
    'tour: loop {
        while let Some(key) = input::try_read_key() {
            if key == Key::Char('\u{1b}') {
                break 'tour;
            }
            match (st.step, key) {
                (Step::Welcome, Key::Char('\n')) | (Step::Welcome, Key::Right) => advance(&mut st),
                (Step::Name, Key::Char('\n')) => advance(&mut st),
                (Step::Name, Key::Char('\u{8}')) => {
                    st.name.pop();
                }
                (Step::Name, Key::Char(c)) if c.is_ascii() && !c.is_ascii_control() && st.name.len() < 20 => st.name.push(c),
                (Step::Chatty, Key::Left) => {
                    st.chatty = st.chatty.saturating_sub(1);
                    react_chatty(&st);
                }
                (Step::Chatty, Key::Right) => {
                    st.chatty = (st.chatty + 1).min(2);
                    react_chatty(&st);
                }
                (Step::Chatty, Key::Char('\n')) => advance(&mut st),
                (Step::Tour(_), Key::Char('\n')) | (Step::Tour(_), Key::Right) => advance(&mut st),
                (Step::Tour(_), Key::Left) => back(&mut st),
                (Step::Steer, Key::Left) => {
                    buddy::nudge(-90);
                    st.steer_moves += 1;
                }
                (Step::Steer, Key::Right) => {
                    buddy::nudge(90);
                    st.steer_moves += 1;
                }
                (Step::Steer, Key::Up) => {
                    buddy::perform("hop", Some(Mood::Excited));
                }
                (Step::Steer, Key::Char('\n')) => advance(&mut st),
                (Step::Done, Key::Char('\n')) => break 'tour,
                _ => {}
            }
        }

        framebuffer::with(|fb| draw(fb, &st));

        // about 30 frames a second
        while crate::time::uptime_ms() < last + 33 {
            x86_64::instructions::hlt();
        }
        last = crate::time::uptime_ms();
    }
    // remember the answers and that the tour has been seen
    let name: String = if st.name.trim().is_empty() { String::from("friend") } else { String::from(st.name.trim()) };
    settings::set("name", &name);
    settings::set("chatty", &alloc::format!("{}", st.chatty));
    settings::set("onboarded", "1");
    buddy::set_name(&name);
    buddy::set_chatty(st.chatty as u8);
    buddy::set_context(Context::Shell);
    buddy::say(&alloc::format!("welcome, {}!", name));
}

fn react_chatty(st: &State) {
    buddy::set_chatty(st.chatty as u8);
    match st.chatty {
        0 => {
            buddy::perform("whistle", Some(Mood::Neutral));
            buddy::say("...");
        }
        1 => {
            buddy::perform("nod", Some(Mood::Happy));
            buddy::say("a little chat now and then");
        }
        _ => {
            buddy::perform("cheer", Some(Mood::Excited));
            buddy::say("yay, i love talking!");
        }
    }
}

fn enter_step(st: &State) {
    match st.step {
        Step::Welcome => {
            buddy::script(&[(Activity::Wave, Mood::Happy, 3500), (Activity::Hop, Mood::Excited, 1200), (Activity::Idle, Mood::Happy, 2000)]);
            buddy::say("hi! i'm Bit. welcome to Zero!");
        }
        Step::Name => {
            buddy::script(&[(Activity::Think, Mood::Curious, 3000), (Activity::LookAround, Mood::Curious, 3000)]);
            buddy::say("what should i call you?");
        }
        Step::Chatty => {
            buddy::script(&[(Activity::Idle, Mood::Curious, 3000)]);
            buddy::say("how talkative should i be?");
        }
        Step::Tour(i) => {
            let card = &TOUR[i];
            buddy::script(&[(card.bit, card.mood, 9000)]);
            buddy::say(card.say);
        }
        Step::Steer => {
            buddy::script(&[(Activity::Present, Mood::Excited, 2000)]);
            buddy::say("steer me with the arrow keys!");
        }
        Step::Done => {
            buddy::script(&[(Activity::Celebrate, Mood::Excited, 6000)]);
            let name = if st.name.trim().is_empty() { "friend" } else { st.name.trim() };
            buddy::say(&alloc::format!("all set, {}!", name));
        }
    }
}

fn advance(st: &mut State) {
    st.step = match st.step {
        Step::Welcome => Step::Name,
        Step::Name => {
            buddy::set_name(if st.name.trim().is_empty() { "friend" } else { st.name.trim() });
            Step::Chatty
        }
        Step::Chatty => Step::Tour(0),
        Step::Tour(i) if i + 1 < TOUR.len() => Step::Tour(i + 1),
        Step::Tour(_) => Step::Steer,
        Step::Steer => Step::Done,
        Step::Done => Step::Done,
    };
    enter_step(st);
}

fn back(st: &mut State) {
    if let Step::Tour(i) = st.step {
        st.step = if i == 0 { Step::Chatty } else { Step::Tour(i - 1) };
        enter_step(st);
    }
}

fn draw(fb: &mut Framebuffer, st: &State) {
    let (w, h) = (fb.width(), fb.height());
    let u = (w / 150).min(h / 90).max(1);
    fb.clear_to(BG);

    let title_font = Font::regular(RasterHeight::Size32);
    let body_font = Font::regular(RasterHeight::Size24);
    let small = Font::regular(RasterHeight::Size20);

    // the card
    let card_w = (110 * u).min(w.saturating_sub(4 * u));
    let card_h = (66 * u).min(h.saturating_sub(4 * u));
    let cx = (w - card_w) / 2;
    let cy = (h - card_h) / 2;
    ui::fill_round(fb, cx - 1, cy - 1, card_w + 2, card_h + 2, 10, ui::BORDER);
    ui::fill_round(fb, cx, cy, card_w, card_h, 10, CARD);

    let pad = 4 * u;
    let tx = cx + pad;
    let mut y = cy + pad;

    // progress dots
    let steps = 4 + TOUR.len();
    let current = match st.step {
        Step::Welcome => 0,
        Step::Name => 1,
        Step::Chatty => 2,
        Step::Tour(i) => 3 + i,
        Step::Steer => 3 + TOUR.len(),
        Step::Done => steps,
    };
    let dot = (u / 2).max(4);
    for i in 0..=steps {
        let color = if i == current {
            ui::ACCENT
        } else if i < current {
            ui::TEXT_MUTED
        } else {
            ui::BORDER
        };
        ui::fill_round(fb, cx + card_w - pad - (steps + 1 - i) * (dot * 2), cy + pad, dot, dot, dot / 2, color);
    }

    let width = card_w - 2 * pad;
    let hint: &str;
    match st.step {
        Step::Welcome => {
            title_font.draw_text(fb, tx, y, "Welcome to Zero", ui::ACCENT, CARD);
            y += title_font.height() + 3 * u;
            y = paragraph(fb, &body_font, tx, y, width, "A small operating system with a persistent disk, games, a 3D toolkit, and me.", ui::TEXT, CARD);
            paragraph(fb, &body_font, tx, y + u, width, "This takes about a minute. Press escape any time to skip it.", ui::TEXT_MUTED, CARD);
            hint = "enter: continue";
        }
        Step::Name => {
            title_font.draw_text(fb, tx, y, "What should I call you?", ui::ACCENT, CARD);
            y += title_font.height() + 4 * u;
            let box_w = 40 * u;
            ui::fill_round(fb, tx, y, box_w, body_font.height() + 3 * u, 6, BG);
            let cursor = if (crate::time::uptime_ms() / 500) % 2 == 0 { "_" } else { " " };
            let shown = alloc::format!("{}{}", st.name, cursor);
            body_font.draw_text(fb, tx + u, y + 3 * u / 2, &shown, ui::TEXT, BG);
            y += body_font.height() + 5 * u;
            paragraph(fb, &small, tx, y, width, "Your name is only used for greetings and is saved on the disk.", ui::TEXT_MUTED, CARD);
            hint = "type a name, then enter";
        }
        Step::Chatty => {
            title_font.draw_text(fb, tx, y, "How talkative should I be?", ui::ACCENT, CARD);
            y += title_font.height() + 4 * u;
            for (i, label) in ["quiet", "normal", "chatty"].iter().enumerate() {
                let bx = tx + i * 24 * u;
                let on = i == st.chatty;
                let (fill, ink) = if on { (ui::ACCENT, BG) } else { (BG, ui::TEXT) };
                ui::fill_round(fb, bx, y, 21 * u, body_font.height() + 4 * u, 6, fill);
                let tw = body_font.text_width(label);
                body_font.draw_text(fb, bx + (21 * u - tw) / 2, y + 2 * u, label, ink, fill);
            }
            y += body_font.height() + 7 * u;
            paragraph(fb, &small, tx, y, width, "I only say things in the status bar. You can change this later with `buddy chatty 0`, `1` or `2`.", ui::TEXT_MUTED, CARD);
            hint = "left/right: choose, enter: continue";
        }
        Step::Tour(i) => {
            let card = &TOUR[i];
            title_font.draw_text(fb, tx, y, card.title, ui::ACCENT, CARD);
            y += title_font.height() + 3 * u;
            for line in card.lines {
                y = paragraph(fb, &body_font, tx, y, width, line, ui::TEXT, CARD);
            }
            hint = "enter or right: next, left: back";
        }
        Step::Steer => {
            title_font.draw_text(fb, tx, y, "Your turn", ui::ACCENT, CARD);
            y += title_font.height() + 3 * u;
            y = paragraph(fb, &body_font, tx, y, width, "Steer me along the floor with the left and right arrow keys. Up makes me hop.", ui::TEXT, CARD);
            if st.steer_moves >= 4 {
                paragraph(fb, &body_font, tx, y + u, width, "Nicely done!", ui::GREEN, CARD);
            }
            hint = "arrows: steer, enter: finish";
        }
        Step::Done => {
            title_font.draw_text(fb, tx, y, "All set!", ui::ACCENT, CARD);
            y += title_font.height() + 3 * u;
            let name = if st.name.trim().is_empty() { "friend" } else { st.name.trim() };
            y = paragraph(fb, &body_font, tx, y, width, &alloc::format!("Welcome aboard, {}. Your files and settings are saved on the disk.", name), ui::TEXT, CARD);
            paragraph(fb, &body_font, tx, y + u, width, "Type `help` to begin, `programs` for games, or just say hi to me in the status bar.", ui::TEXT_MUTED, CARD);
            hint = "enter: start";
        }
    }
    small.draw_text(fb, tx, cy + card_h - pad - small.height(), hint, ui::TEXT_FAINT, CARD);

    // Bit stands on the card's lower edge, with what it is saying
    let scale = (u * 8 / 10).max(2);
    let lane_x = cx + pad;
    let lane_w = card_w - 2 * pad;
    let floor = cy + card_h - pad - small.height() - 2 * u;
    buddy::draw(fb, lane_x, lane_w, floor, scale);
    if let Some(text) = buddy::thought() {
        let bit_x = buddy::screen_x(lane_x, lane_w, scale);
        let bw = small.text_width(&text) + 3 * u;
        let bh = small.height() + 2 * u;
        let by = floor.saturating_sub(buddy::SPRITE.1 * scale + bh + u);
        let bx = (bit_x + buddy::SPRITE.0 * scale / 2).saturating_sub(bw / 2).clamp(cx + pad, (cx + card_w).saturating_sub(pad + bw));
        ui::fill_round(fb, bx, by, bw, bh, 8, ui::BG_PANEL);
        small.draw_text(fb, bx + 3 * u / 2, by + u, &text, ui::TEXT, ui::BG_PANEL);
    }
    fb.flush();
}

/// Whether the tour has been seen on this disk.
pub fn seen() -> bool {
    settings::get("onboarded").is_some()
}
