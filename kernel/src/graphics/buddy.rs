//! Bit: the little creature that lives in the status bar, on the boot screen
//! and in the welcome tour.
//!
//! There is exactly one Bit; what makes it feel alive is what it does.
//!
//! * **Behaviors.** 64 activities (typing on a laptop, playing a game with a
//!   headset on, reading, napping, juggling, stargazing...) times 6 moods
//!   times 3 intensities make 1152 distinct behaviors; the mood and intensity
//!   change how an activity looks (tired eyes, a lively bounce, a grumpy
//!   frown...).
//! * **A little inner life.** Bit has energy, a mood, boredom and curiosity
//!   that change over time and with what happens.
//! * **Awareness.** The system tells Bit what is going on: which screen is
//!   showing, which command was just run and whether it worked, how fast you
//!   are typing, what a game is up to. Bit chooses what to do next from all
//!   of that, avoiding what it did recently, and sometimes says what it is
//!   thinking.
//!
//! None of this is a language model; it is a scoring engine with a lot of
//! hand-written taste. It is meant to feel like company.

use crate::graphics::display::Display;
use crate::graphics::framebuffer::Framebuffer;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

// ---------------------------------------------------------------- the canvas

/// Size of the drawing canvas in buddy pixels, the column of the body's
/// centre and the rows the body and legs sit on.
const CW: usize = 26;
const CH: usize = 22;
const CX: i32 = 13;
const BODY_TOP: i32 = 11;
const BODY_BOTTOM: i32 = 18;
const LEG_Y: i32 = 19;

// palette indexes on the canvas
const BODY: u8 = 1;
const SHADE: u8 = 2;
const LIGHT: u8 = 3;
const DARK: u8 = 4;
const BLUSH: u8 = 5;
const YELLOW: u8 = 6;
const GRAY: u8 = 7;
const WHITE: u8 = 8;
const RED: u8 = 9;
const BLUE: u8 = 10;
const GREEN: u8 = 11;
const PURPLE: u8 = 12;
const BROWN: u8 = 13;
const DGRAY: u8 = 14;

fn palette(index: u8) -> u32 {
    match index {
        BODY => 0xEEF1F5,
        SHADE => 0xB3BAC7,
        LIGHT => 0xFFFFFF,
        DARK => 0x1B1D23,
        BLUSH => 0xF0A0B0,
        YELLOW => 0xDEC184,
        GRAY => 0x9AA1AE,
        WHITE => 0xF1F3F6,
        RED => 0xD07277,
        BLUE => 0x74ADE8,
        GREEN => 0xA1C181,
        PURPLE => 0xB477CF,
        BROWN => 0xBF956A,
        _ => 0x4B5263,
    }
}

struct Canvas {
    px: [u8; CW * CH],
}

impl Canvas {
    fn set(&mut self, x: i32, y: i32, color: u8) {
        if x >= 0 && y >= 0 && (x as usize) < CW && (y as usize) < CH {
            self.px[y as usize * CW + x as usize] = color;
        }
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u8) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, color);
            }
        }
    }
}

// ---------------------------------------------------------------- activities

macro_rules! activities {
    ($($name:ident = $text:expr),* $(,)?) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum Activity { $($name),* }
        /// Every activity with the name the `buddy` command knows it by.
        pub const ACTIVITIES: &[(Activity, &str)] = &[$((Activity::$name, $text)),*];
    };
}

activities! {
    Idle = "idle", LookAround = "look", Hum = "hum", Whistle = "whistle", Stretch = "stretch",
    Yawn = "yawn", ScratchHead = "scratch", TapFoot = "tap", Sway = "sway", Sigh = "sigh",
    Hiccup = "hiccup", Sneeze = "sneeze", Shiver = "shiver",
    Walk = "walk", Run = "run", Tiptoe = "tiptoe", Skip = "skip", Hop = "hop", Spin = "spin",
    Peek = "peek", Pace = "pace",
    Wave = "wave", Bow = "bow", Nod = "nod", ShakeHead = "shake", Shrug = "shrug",
    ThumbsUp = "thumbsup", Cheer = "cheer", Clap = "clap", Point = "point", Wink = "wink",
    BlowKiss = "kiss", Flex = "flex",
    TypeLaptop = "type", ReadBook = "read", WriteNotes = "write", Debug = "debug", Build = "build",
    Think = "think", Calculate = "calculate", SipCoffee = "coffee", SipTea = "tea", Snack = "snack",
    PlayGame = "play", Dance = "dance", Juggle = "juggle", Balloon = "balloon", Bubbles = "bubbles",
    Stargaze = "stargaze", PaperPlane = "plane",
    Sleep = "sleep", Nap = "nap", Dream = "dream",
    Surprised = "surprised", Scared = "scared", Sad = "sad", Angry = "angry", Love = "love",
    Embarrassed = "blush", Proud = "proud", Celebrate = "celebrate", Confused = "confused",
    Dizzy = "dizzy", Listen = "listen", Present = "present",
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mood {
    Neutral,
    Happy,
    Tired,
    Excited,
    Curious,
    Grumpy,
}

pub const MOOD_NAMES: [&str; 6] = ["neutral", "happy", "tired", "excited", "curious", "grumpy"];

const MOODS: [Mood; 6] = [Mood::Neutral, Mood::Happy, Mood::Tired, Mood::Excited, Mood::Curious, Mood::Grumpy];

/// How many distinct behaviors Bit has: activity x mood x intensity (calm,
/// normal, lively).
pub fn behavior_count() -> usize {
    ACTIVITIES.len() * MOODS.len() * 3
}

fn mood_index(mood: Mood) -> usize {
    MOODS.iter().position(|m| *m == mood).unwrap_or(0)
}

// ---------------------------------------------------------------- the pose

#[derive(Clone, Copy, PartialEq, Eq)]
enum Eyes {
    Open,
    Blink,
    Happy,
    Sleep,
    Wide,
    Left,
    Right,
    Up,
    Down,
    Dizzy,
    Wink,
    Squint,
    Heart,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mouth {
    None,
    Smile,
    Open,
    Tiny,
    Frown,
    Grin,
    Whistle,
    Tongue,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arm {
    Down,
    Out,
    Up,
    Front,
    TypeA,
    TypeB,
    Wave1,
    Wave2,
    Hold,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Head {
    None,
    Headset,
    PartyHat,
    Sunglasses,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Prop {
    None,
    Laptop,
    Controller,
    Book,
    Notes,
    Magnifier,
    Wrench,
    Bulb,
    Coffee,
    Tea,
    Cookie,
    Balloon,
    Wand,
    Plane,
    Telescope,
    Calculator,
    Cloud,
    Trophy,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fx {
    None,
    Sweat,
    Hearts,
    Zzz,
    Notes,
    Stars,
    Question,
    Exclaim,
    Rain,
    Steam,
    Confetti,
    Sparkles,
    Bubbles,
    Hiccup,
}

#[derive(Clone, Copy)]
struct Pose {
    eyes: Eyes,
    mouth: Mouth,
    arm_l: Arm,
    arm_r: Arm,
    /// Buddy pixels above the floor.
    lift: i32,
    /// Squashed down (landing, bowing).
    squash: bool,
    /// Leaning into the direction it faces (-1, 0, 1).
    lean: i32,
    /// Leg animation phase.
    step: u32,
    head: Head,
    prop: Prop,
    fx: Fx,
    /// Animation counter for props and effects.
    phase: u32,
}

impl Pose {
    const fn rest() -> Pose {
        Pose {
            eyes: Eyes::Open,
            mouth: Mouth::Smile,
            arm_l: Arm::Down,
            arm_r: Arm::Down,
            lift: 0,
            squash: false,
            lean: 0,
            step: 0,
            head: Head::None,
            prop: Prop::None,
            fx: Fx::None,
            phase: 0,
        }
    }
}

/// Keystrokes seen so far (the interrupt handler bumps this; it is only an
/// atomic so it is safe there).
static KEYS: AtomicU32 = AtomicU32::new(0);
static LAST_KEY_MS: AtomicU64 = AtomicU64::new(0);

/// Called for every key press.
pub fn note_key() {
    KEYS.fetch_add(1, Ordering::Relaxed);
    LAST_KEY_MS.store(crate::time::uptime_ms(), Ordering::Relaxed);
}

/// The pose of an activity `t` milliseconds in. `typing` is the number of
/// keystrokes so far, so typing can follow the real keyboard.
fn pose(activity: Activity, t: u64, calm: i32, typing: u32) -> Pose {
    use Activity::*;
    // lively behaviors run faster, calm ones slower
    let t = match calm {
        -1 => t * 7 / 10,
        1 => t * 14 / 10,
        _ => t,
    };
    let mut p = Pose::rest();
    p.phase = (t / 160) as u32;
    let beat = |ms: u64| (t / ms) % 2 == 0;
    let blink = |period: u64| if t % period < 130 { Eyes::Blink } else { Eyes::Open };
    match activity {
        Idle => p.eyes = blink(3100),
        LookAround => {
            p.eyes = match (t / 700) % 4 {
                0 => Eyes::Left,
                1 => Eyes::Open,
                2 => Eyes::Right,
                _ => Eyes::Up,
            };
            p.mouth = Mouth::Tiny;
        }
        Hum => {
            p.eyes = Eyes::Happy;
            p.mouth = Mouth::Tiny;
            p.fx = Fx::Notes;
            p.lean = if beat(500) { 1 } else { 0 };
        }
        Whistle => {
            p.mouth = Mouth::Whistle;
            p.eyes = blink(2600);
            p.fx = Fx::Notes;
            p.arm_r = Arm::Hold;
        }
        Stretch => {
            p.arm_l = Arm::Up;
            p.arm_r = Arm::Up;
            p.eyes = Eyes::Happy;
            p.lift = if beat(450) { 1 } else { 0 };
        }
        Yawn => {
            let wide = t > 500 && t < 1500;
            p.eyes = if wide { Eyes::Sleep } else { Eyes::Blink };
            p.mouth = if wide { Mouth::Open } else { Mouth::Tiny };
            if wide {
                p.arm_l = Arm::Up;
                p.arm_r = Arm::Up;
            }
        }
        ScratchHead => {
            p.arm_r = if beat(180) { Arm::Wave1 } else { Arm::Wave2 };
            p.eyes = Eyes::Squint;
            p.mouth = Mouth::Tiny;
            p.fx = if t > 900 { Fx::Question } else { Fx::None };
        }
        TapFoot => {
            p.step = if beat(260) { 1 } else { 0 };
            p.eyes = Eyes::Right;
            p.mouth = Mouth::Tiny;
            p.arm_l = Arm::Front;
            p.arm_r = Arm::Front;
        }
        Sway => {
            p.lean = match (t / 450) % 4 {
                0 => -1,
                2 => 1,
                _ => 0,
            };
            p.eyes = Eyes::Happy;
            p.mouth = Mouth::Tiny;
        }
        Sigh => {
            p.eyes = if t % 2000 < 900 { Eyes::Sleep } else { Eyes::Open };
            p.mouth = Mouth::Frown;
            p.squash = t % 2000 < 700;
            p.fx = if t % 2000 > 600 { Fx::Sweat } else { Fx::None };
        }
        Hiccup => {
            p.fx = Fx::Hiccup;
            p.lift = if t % 1100 < 130 { 2 } else { 0 };
            p.eyes = if t % 1100 < 130 { Eyes::Wide } else { Eyes::Open };
            p.mouth = if t % 1100 < 130 { Mouth::Open } else { Mouth::Tiny };
        }
        Sneeze => {
            let phase = t % 3000;
            p.eyes = if phase < 900 { Eyes::Squint } else if phase < 1100 { Eyes::Sleep } else { Eyes::Open };
            p.mouth = if phase > 1000 && phase < 1300 { Mouth::Open } else { Mouth::Tiny };
            p.lean = if phase > 1000 && phase < 1300 { -1 } else { 0 };
            p.lift = if phase > 1000 && phase < 1150 { 1 } else { 0 };
            p.fx = if phase > 1050 && phase < 1500 { Fx::Bubbles } else { Fx::None };
        }
        Shiver => {
            p.lean = if beat(70) { 1 } else { -1 };
            p.eyes = Eyes::Squint;
            p.mouth = Mouth::Frown;
            p.fx = Fx::Sweat;
        }
        Walk | Run | Tiptoe | Skip | Pace => {
            let speed = match activity {
                Run => 90,
                Skip => 120,
                Tiptoe => 260,
                _ => 150,
            };
            p.step = (t / speed) as u32;
            p.eyes = blink(2800);
            p.lean = if activity == Run { 1 } else { 0 };
            p.mouth = if activity == Run { Mouth::Open } else { Mouth::Smile };
            p.lift = match activity {
                Skip => {
                    if beat(120) {
                        1
                    } else {
                        0
                    }
                }
                Tiptoe => 1,
                _ => 0,
            };
            if activity == Pace {
                p.arm_l = Arm::Front;
                p.arm_r = Arm::Front;
                p.eyes = Eyes::Down;
                p.mouth = Mouth::Tiny;
            }
            if activity == Run {
                p.arm_l = Arm::Out;
                p.arm_r = Arm::Out;
            }
        }
        Hop => {
            let phase = (t % 700) * 1000 / 700;
            p.lift = (phase * (1000 - phase) * 6 / 250_000) as i32;
            p.eyes = Eyes::Happy;
            p.mouth = Mouth::Grin;
            p.squash = phase < 90 || phase > 910;
        }
        Spin => {
            p.eyes = if beat(100) { Eyes::Left } else { Eyes::Right };
            p.mouth = Mouth::Open;
            p.arm_l = Arm::Out;
            p.arm_r = Arm::Out;
            p.step = (t / 100) as u32;
            p.lean = match (t / 100) % 4 {
                0 => -1,
                2 => 1,
                _ => 0,
            };
        }
        Peek => {
            p.eyes = if beat(900) { Eyes::Left } else { Eyes::Wide };
            p.mouth = Mouth::Tiny;
            p.arm_l = Arm::Up;
        }
        Wave => {
            p.arm_r = if beat(200) { Arm::Wave1 } else { Arm::Wave2 };
            p.eyes = blink(2000);
            p.mouth = Mouth::Open;
        }
        Bow => {
            p.squash = t % 2400 > 400 && t % 2400 < 1500;
            p.lean = 1;
            p.eyes = Eyes::Happy;
            p.arm_l = Arm::Front;
            p.arm_r = Arm::Front;
        }
        Nod => {
            p.squash = beat(300);
            p.eyes = Eyes::Happy;
            p.mouth = Mouth::Smile;
        }
        ShakeHead => {
            p.lean = if beat(220) { -1 } else { 1 };
            p.eyes = Eyes::Squint;
            p.mouth = Mouth::Frown;
        }
        Shrug => {
            p.arm_l = Arm::Out;
            p.arm_r = Arm::Out;
            p.lift = if beat(500) { 1 } else { 0 };
            p.eyes = Eyes::Squint;
            p.mouth = Mouth::Tiny;
            p.fx = Fx::Question;
        }
        ThumbsUp => {
            p.arm_r = Arm::Up;
            p.eyes = Eyes::Happy;
            p.mouth = Mouth::Grin;
            p.fx = Fx::Sparkles;
        }
        Cheer => {
            p.arm_l = Arm::Up;
            p.arm_r = Arm::Up;
            p.eyes = Eyes::Happy;
            p.mouth = Mouth::Open;
            p.lift = if beat(150) { 2 } else { 0 };
            p.fx = Fx::Hearts;
        }
        Clap => {
            p.arm_l = if beat(140) { Arm::Front } else { Arm::Out };
            p.arm_r = if beat(140) { Arm::Front } else { Arm::Out };
            p.eyes = Eyes::Happy;
            p.mouth = Mouth::Grin;
            p.fx = Fx::Sparkles;
        }
        Point => {
            p.arm_r = Arm::Out;
            p.eyes = Eyes::Right;
            p.mouth = Mouth::Smile;
            p.fx = if beat(500) { Fx::Exclaim } else { Fx::None };
        }
        Wink => {
            p.eyes = if t % 1600 < 800 { Eyes::Wink } else { Eyes::Open };
            p.mouth = Mouth::Grin;
            p.arm_r = if t % 1600 < 800 { Arm::Out } else { Arm::Down };
        }
        BlowKiss => {
            p.eyes = Eyes::Happy;
            p.mouth = Mouth::Tiny;
            p.arm_r = if t % 1800 < 600 { Arm::Front } else { Arm::Out };
            p.fx = if t % 1800 > 600 { Fx::Hearts } else { Fx::None };
        }
        Flex => {
            p.arm_l = if beat(500) { Arm::Up } else { Arm::Out };
            p.arm_r = if beat(500) { Arm::Up } else { Arm::Out };
            p.eyes = Eyes::Squint;
            p.mouth = Mouth::Grin;
            p.head = Head::Sunglasses;
        }
        TypeLaptop => {
            p.prop = Prop::Laptop;
            // the hands follow the real keyboard: one tap per key press
            let tap = if typing > 0 { typing } else { (t / 130) as u32 };
            p.arm_l = if tap % 2 == 0 { Arm::TypeA } else { Arm::TypeB };
            p.arm_r = if tap % 2 == 0 { Arm::TypeB } else { Arm::TypeA };
            p.eyes = if t % 2600 < 130 {
                Eyes::Blink
            } else if (t / 1300) % 3 == 0 {
                Eyes::Left
            } else {
                Eyes::Open
            };
            p.mouth = if (t / 1500) % 3 == 0 { Mouth::Tiny } else { Mouth::Smile };
        }
        ReadBook => {
            p.prop = Prop::Book;
            p.arm_l = Arm::Front;
            p.arm_r = Arm::Front;
            p.eyes = if (t / 500) % 2 == 0 { Eyes::Left } else { Eyes::Right };
            if t % 2400 > 2250 {
                p.eyes = Eyes::Blink;
            }
            p.mouth = Mouth::None;
        }
        WriteNotes => {
            p.prop = Prop::Notes;
            p.arm_r = if beat(150) { Arm::TypeA } else { Arm::TypeB };
            p.arm_l = Arm::Front;
            p.eyes = Eyes::Down;
            p.mouth = Mouth::Tiny;
        }
        Debug => {
            p.prop = Prop::Magnifier;
            p.arm_r = Arm::Hold;
            p.eyes = if beat(600) { Eyes::Left } else { Eyes::Right };
            p.mouth = Mouth::Tiny;
            p.fx = if t % 3000 > 2300 { Fx::Exclaim } else { Fx::None };
        }
        Build => {
            p.prop = Prop::Wrench;
            p.arm_r = if beat(220) { Arm::Hold } else { Arm::Up };
            p.eyes = Eyes::Squint;
            p.mouth = Mouth::Grin;
            p.fx = if beat(220) { Fx::Sparkles } else { Fx::None };
        }
        Think => {
            p.eyes = Eyes::Up;
            p.mouth = Mouth::Tiny;
            p.arm_r = Arm::Front;
            if t > 800 {
                p.prop = Prop::Bulb;
                p.eyes = Eyes::Wide;
                p.mouth = Mouth::Open;
                p.arm_r = Arm::Up;
            }
        }
        Calculate => {
            p.prop = Prop::Calculator;
            p.arm_l = if beat(200) { Arm::TypeA } else { Arm::TypeB };
            p.arm_r = if beat(200) { Arm::TypeB } else { Arm::TypeA };
            p.eyes = Eyes::Down;
            p.mouth = Mouth::Tiny;
            p.fx = if t % 2400 > 2000 { Fx::Question } else { Fx::None };
        }
        SipCoffee | SipTea => {
            p.prop = if activity == SipCoffee { Prop::Coffee } else { Prop::Tea };
            p.arm_r = Arm::Hold;
            p.eyes = if (t / 1500) % 2 == 1 { Eyes::Sleep } else { blink(2400) };
            p.mouth = Mouth::Tiny;
        }
        Snack => {
            p.prop = Prop::Cookie;
            p.arm_r = Arm::Hold;
            p.eyes = Eyes::Happy;
            p.mouth = if beat(300) { Mouth::Open } else { Mouth::Smile };
        }
        PlayGame => {
            p.head = Head::Headset;
            p.prop = Prop::Controller;
            p.arm_l = Arm::TypeA;
            p.arm_r = Arm::TypeB;
            p.eyes = if (t / 1200) % 5 == 4 { Eyes::Wide } else { Eyes::Open };
            p.mouth = if (t / 900) % 3 == 0 { Mouth::Open } else { Mouth::Tiny };
            p.lean = if beat(240) { 0 } else { 1 };
            p.step = (typing & 1) + 1;
        }
        Dance => {
            let b = beat(190);
            p.arm_l = if b { Arm::Up } else { Arm::Down };
            p.arm_r = if b { Arm::Down } else { Arm::Up };
            p.lift = if b { 1 } else { 0 };
            p.step = if b { 1 } else { 0 };
            p.eyes = Eyes::Happy;
            p.mouth = Mouth::Grin;
            p.fx = Fx::Notes;
            p.head = Head::Headset;
        }
        Juggle => {
            p.arm_l = if beat(200) { Arm::Up } else { Arm::Front };
            p.arm_r = if beat(200) { Arm::Front } else { Arm::Up };
            p.eyes = if beat(400) { Eyes::Up } else { Eyes::Down };
            p.mouth = Mouth::Tiny;
            p.fx = Fx::Stars;
        }
        Balloon => {
            p.prop = Prop::Balloon;
            p.arm_r = Arm::Up;
            p.eyes = blink(2200);
            p.lift = if beat(500) { 0 } else { 1 };
        }
        Bubbles => {
            p.prop = Prop::Wand;
            p.arm_r = Arm::Hold;
            p.eyes = Eyes::Happy;
            p.mouth = Mouth::Whistle;
            p.fx = Fx::Bubbles;
        }
        Stargaze => {
            p.prop = Prop::Telescope;
            p.arm_r = Arm::Hold;
            p.arm_l = Arm::Hold;
            p.eyes = Eyes::Up;
            p.mouth = Mouth::Open;
            p.fx = Fx::Sparkles;
        }
        PaperPlane => {
            p.prop = Prop::Plane;
            p.arm_r = if t % 3000 < 1500 { Arm::Hold } else { Arm::Up };
            p.eyes = if t % 3000 < 1500 { Eyes::Down } else { Eyes::Up };
            p.mouth = Mouth::Grin;
        }
        Sleep | Nap => {
            p.eyes = Eyes::Sleep;
            p.mouth = Mouth::None;
            p.fx = Fx::Zzz;
            p.squash = (t / 900) % 2 == 0;
        }
        Dream => {
            p.eyes = Eyes::Sleep;
            p.mouth = Mouth::Smile;
            p.fx = Fx::Stars;
            p.prop = Prop::Cloud;
            p.squash = (t / 900) % 2 == 0;
        }
        Surprised => {
            p.eyes = Eyes::Wide;
            p.mouth = Mouth::Open;
            p.arm_l = Arm::Up;
            p.arm_r = Arm::Up;
            p.fx = Fx::Exclaim;
            p.lift = if t < 220 { 4 } else { 0 };
        }
        Scared => {
            p.eyes = Eyes::Wide;
            p.mouth = Mouth::Frown;
            p.lean = if beat(60) { 1 } else { -1 };
            p.arm_l = Arm::Front;
            p.arm_r = Arm::Front;
            p.fx = Fx::Sweat;
        }
        Sad => {
            p.eyes = Eyes::Sleep;
            p.mouth = Mouth::Frown;
            p.prop = Prop::Cloud;
            p.fx = Fx::Rain;
            p.squash = true;
        }
        Angry => {
            p.eyes = Eyes::Squint;
            p.mouth = Mouth::Frown;
            p.fx = Fx::Steam;
            p.arm_l = Arm::Front;
            p.arm_r = Arm::Front;
            p.lean = if beat(120) { 1 } else { 0 };
        }
        Love => {
            p.eyes = Eyes::Heart;
            p.mouth = Mouth::Smile;
            p.fx = Fx::Hearts;
            p.lean = if beat(500) { 1 } else { 0 };
        }
        Embarrassed => {
            p.eyes = Eyes::Down;
            p.mouth = Mouth::Tiny;
            p.arm_l = Arm::Front;
            p.arm_r = Arm::Front;
            p.fx = Fx::Sweat;
        }
        Proud => {
            p.eyes = Eyes::Happy;
            p.mouth = Mouth::Grin;
            p.arm_l = Arm::Out;
            p.arm_r = Arm::Out;
            p.lift = 1;
            p.prop = Prop::Trophy;
            p.fx = Fx::Sparkles;
        }
        Celebrate => {
            p.eyes = Eyes::Happy;
            p.mouth = Mouth::Open;
            p.arm_l = Arm::Up;
            p.arm_r = Arm::Up;
            p.lift = if beat(170) { 3 } else { 0 };
            p.head = Head::PartyHat;
            p.fx = Fx::Confetti;
        }
        Confused => {
            p.eyes = if beat(700) { Eyes::Left } else { Eyes::Right };
            p.mouth = Mouth::Tiny;
            p.arm_r = Arm::Wave1;
            p.fx = Fx::Question;
        }
        Dizzy => {
            p.eyes = Eyes::Dizzy;
            p.mouth = Mouth::Open;
            p.fx = Fx::Stars;
            p.lean = match (t / 150) % 4 {
                0 => -1,
                2 => 1,
                _ => 0,
            };
        }
        Listen => {
            p.head = Head::Headset;
            p.eyes = Eyes::Happy;
            p.mouth = Mouth::Tiny;
            p.fx = Fx::Notes;
            p.lean = if beat(420) { 1 } else { 0 };
            p.arm_l = Arm::Down;
            p.arm_r = if beat(840) { Arm::Down } else { Arm::Out };
        }
        Present => {
            p.arm_r = Arm::Out;
            p.eyes = if beat(1400) { Eyes::Right } else { Eyes::Open };
            p.mouth = Mouth::Open;
            p.fx = if beat(700) { Fx::Sparkles } else { Fx::None };
        }
    }
    p
}

/// Mood changes how an activity looks: eyes and mouth that the activity left
/// at their defaults take on the mood, and some moods add small extras.
fn color_with_mood(p: &mut Pose, mood: Mood, t: u64) {
    let default_eyes = matches!(p.eyes, Eyes::Open);
    let default_mouth = matches!(p.mouth, Mouth::Smile);
    match mood {
        Mood::Neutral => {}
        Mood::Happy => {
            if default_mouth {
                p.mouth = Mouth::Grin;
            }
            if default_eyes && t % 4200 < 700 {
                p.eyes = Eyes::Happy;
            }
        }
        Mood::Tired => {
            if default_eyes {
                p.eyes = if t % 5000 < 3500 { Eyes::Squint } else { Eyes::Blink };
            }
            if default_mouth {
                p.mouth = Mouth::Tiny;
            }
        }
        Mood::Excited => {
            if default_eyes {
                p.eyes = Eyes::Wide;
            }
            if default_mouth {
                p.mouth = Mouth::Open;
            }
            if p.lift == 0 && (t / 260) % 2 == 0 {
                p.lift = 1;
            }
        }
        Mood::Curious => {
            if default_eyes {
                p.eyes = match (t / 800) % 3 {
                    0 => Eyes::Left,
                    1 => Eyes::Up,
                    _ => Eyes::Right,
                };
            }
            if p.fx == Fx::None && t % 6000 > 5200 {
                p.fx = Fx::Question;
            }
        }
        Mood::Grumpy => {
            if default_eyes {
                p.eyes = Eyes::Squint;
            }
            if default_mouth {
                p.mouth = Mouth::Frown;
            }
        }
    }
}

// ---------------------------------------------------------------- drawing

fn paint(p: &Pose) -> Canvas {
    let mut c = Canvas { px: [0; CW * CH] };
    let squash = if p.squash { 1 } else { 0 };
    let lean = p.lean;
    let top = BODY_TOP + squash;
    let widths: [i32; 8] = [14; 8];
    let rows = 8 - squash;

    // the body: a wide rounded block, darker along the bottom edge
    for i in 0..rows {
        let w = widths[(i + squash) as usize % 8];
        let y = top + i;
        let left = CX - w / 2 + if i < 3 { lean } else { 0 };
        c.rect(left, y, w, 1, if i + 1 == rows { SHADE } else { BODY });
    }

    // the face
    let ey = top + 2;
    let (lx, rx) = (CX - 4 + lean, CX + 3 + lean);
    let eye = |c: &mut Canvas, x: i32, shape: Eyes, right: bool| {
        let shape = if shape == Eyes::Wink {
            if right {
                Eyes::Open
            } else {
                Eyes::Blink
            }
        } else {
            shape
        };
        match shape {
            Eyes::Open | Eyes::Wink => c.rect(x, ey, 1, 2, DARK),
            Eyes::Left => c.rect(x - 1, ey, 1, 2, DARK),
            Eyes::Right => c.rect(x + 1, ey, 1, 2, DARK),
            Eyes::Up => c.rect(x, ey - 1, 1, 2, DARK),
            Eyes::Down => c.rect(x, ey + 1, 1, 2, DARK),
            Eyes::Wide => {
                c.rect(x - 1, ey - 1, 3, 4, DARK);
                c.rect(x, ey, 1, 2, WHITE);
            }
            Eyes::Blink | Eyes::Sleep => c.rect(x - 1, ey + 1, 3, 1, DARK),
            Eyes::Squint => {
                c.rect(x - 1, ey + 1, 3, 1, DARK);
                c.set(x - 1, ey, DARK);
            }
            Eyes::Happy => {
                c.set(x - 1, ey + 1, DARK);
                c.set(x, ey, DARK);
                c.set(x + 1, ey + 1, DARK);
            }
            Eyes::Dizzy => {
                c.set(x - 1, ey, DARK);
                c.set(x + 1, ey, DARK);
                c.set(x, ey + 1, DARK);
                c.set(x - 1, ey + 2, DARK);
                c.set(x + 1, ey + 2, DARK);
            }
            Eyes::Heart => {
                c.set(x - 1, ey, RED);
                c.set(x + 1, ey, RED);
                c.rect(x - 1, ey + 1, 3, 1, RED);
                c.set(x, ey + 2, RED);
            }
        }
    };
    eye(&mut c, lx, p.eyes, false);
    eye(&mut c, rx, p.eyes, true);
    let mx = CX + lean;
    let my = top + 5;
    match p.mouth {
        Mouth::Smile => {
            c.set(mx - 1, my, DARK);
            c.set(mx, my + 1, DARK);
            c.set(mx + 1, my, DARK);
        }
        Mouth::Grin => {
            c.rect(mx - 2, my, 5, 1, DARK);
            c.rect(mx - 1, my + 1, 3, 1, DARK);
        }
        Mouth::Open => c.rect(mx - 1, my, 2, 2, DARK),
        Mouth::Tiny => c.rect(mx - 1, my + 1, 2, 1, DARK),
        Mouth::Frown => {
            c.set(mx - 1, my + 1, DARK);
            c.set(mx, my, DARK);
            c.set(mx + 1, my + 1, DARK);
        }
        Mouth::Whistle => c.rect(mx, my, 1, 2, DARK),
        Mouth::Tongue => {
            c.rect(mx - 1, my, 3, 1, DARK);
            c.set(mx, my + 1, RED);
        }
        Mouth::None => {}
    }

    // headgear
    match p.head {
        Head::Headset => {
            c.rect(CX - 6 + lean, top - 1, 4, 1, GRAY);
            c.rect(CX + 3 + lean, top - 1, 4, 1, GRAY);
            c.rect(CX - 7 + lean, top, 1, 3, GRAY);
            c.rect(CX + 7 + lean, top, 1, 3, GRAY);
            c.rect(CX - 8 + lean, top + 3, 2, 4, DGRAY);
            c.rect(CX + 7 + lean, top + 3, 2, 4, DGRAY);
            c.rect(CX - 7 + lean, top + 7, 3, 1, GRAY);
            c.set(CX - 5 + lean, top + 8, GRAY);
        }
        Head::PartyHat => {
            c.rect(CX - 2 + lean, top - 1, 5, 1, PURPLE);
            c.rect(CX - 1 + lean, top - 3, 3, 2, YELLOW);
            c.set(CX + lean, top - 4, RED);
        }
        Head::Sunglasses => {
            c.rect(lx - 1, ey, 3, 2, DARK);
            c.rect(rx - 1, ey, 3, 2, DARK);
            c.rect(lx + 2, ey, 2, 1, DARK);
        }
        Head::None => {}
    }

    // arms: nubs on the sides at hand height
    let ay = top + 3;
    let lbx = CX - 9 + lean / 2;
    let rbx = CX + 7 + lean / 2;
    let arm = |c: &mut Canvas, left: bool, pos: Arm| {
        let x = if left { lbx } else { rbx };
        let dir = if left { -1 } else { 1 };
        match pos {
            Arm::Down => c.rect(x, ay, 2, 3, BODY),
            Arm::Out => c.rect(x + dir, ay, 3, 2, BODY),
            Arm::Up => c.rect(x, ay - 4, 2, 5, BODY),
            Arm::Front => c.rect(x - dir * 2, ay + 1, 3, 2, BODY),
            Arm::TypeA => c.rect(x - dir * 2, ay + 2, 3, 2, BODY),
            Arm::TypeB => c.rect(x - dir * 2, ay + 3, 3, 2, BODY),
            Arm::Wave1 => c.rect(x, ay - 3, 2, 4, BODY),
            Arm::Wave2 => c.rect(x + dir, ay - 3, 2, 3, BODY),
            Arm::Hold => c.rect(x - dir, ay, 3, 2, BODY),
        }
    };
    arm(&mut c, true, p.arm_l);
    arm(&mut c, false, p.arm_r);

    // legs
    for (i, dx) in [-6, -3, 2, 5].into_iter().enumerate() {
        let up = (i as u32 + p.step) % 2 == 0 && p.step != 0;
        c.rect(CX + dx, LEG_Y, 2, if up { 1 } else { 2 }, SHADE);
    }

    // props, in front of or beside the body
    let ph = p.phase as i32;
    match p.prop {
        Prop::Laptop => {
            c.rect(CX - 5, BODY_BOTTOM - 3, 11, 3, DGRAY);
            c.set(CX, BODY_BOTTOM - 2, YELLOW);
            c.rect(CX - 6, BODY_BOTTOM, 13, 1, GRAY);
        }
        Prop::Controller => {
            c.rect(CX - 5, BODY_BOTTOM - 3, 11, 4, GRAY);
            c.rect(CX - 4, BODY_BOTTOM - 2, 3, 1, DGRAY);
            c.set(CX - 3, BODY_BOTTOM - 3, DGRAY);
            c.set(CX - 3, BODY_BOTTOM - 1, DGRAY);
            let press = ph % 4;
            c.set(CX + 2, BODY_BOTTOM - 2, if press == 0 { WHITE } else { RED });
            c.set(CX + 4, BODY_BOTTOM - 2, if press == 2 { WHITE } else { BLUE });
        }
        Prop::Book => {
            c.rect(CX - 5, BODY_BOTTOM - 4, 11, 5, BLUE);
            c.rect(CX - 4, BODY_BOTTOM - 3, 9, 3, WHITE);
            c.rect(CX, BODY_BOTTOM - 4, 1, 5, GRAY);
        }
        Prop::Notes => {
            c.rect(CX - 4, BODY_BOTTOM - 4, 9, 5, WHITE);
            c.rect(CX - 3, BODY_BOTTOM - 3, 5, 1, GRAY);
            c.rect(CX - 3, BODY_BOTTOM - 1, 4, 1, GRAY);
            c.rect(CX + 6, BODY_BOTTOM - 5 + (ph % 2), 1, 4, YELLOW);
        }
        Prop::Magnifier => {
            c.rect(CX + 5, top + 1, 4, 4, WHITE);
            c.rect(CX + 6, top + 2, 2, 2, LIGHT);
            c.rect(CX + 8, top + 5, 1, 4, BROWN);
        }
        Prop::Wrench => {
            c.rect(CX + 6, top - 2 + (ph % 2), 1, 5, GRAY);
            c.rect(CX + 5, top - 3 + (ph % 2), 3, 1, GRAY);
        }
        Prop::Bulb => {
            let glow = if ph % 4 < 2 { YELLOW } else { WHITE };
            c.rect(CX + 5, top - 8, 4, 3, glow);
            c.rect(CX + 6, top - 9, 2, 1, glow);
            c.rect(CX + 6, top - 5, 2, 1, GRAY);
            c.set(CX + 4, top - 7, glow);
            c.set(CX + 9, top - 7, glow);
        }
        Prop::Coffee | Prop::Tea => {
            let (cx, cy) = (CX + 7, top + 5);
            let drink = if p.prop == Prop::Coffee { BROWN } else { GREEN };
            c.rect(cx, cy, 4, 3, WHITE);
            c.rect(cx, cy, 4, 1, drink);
            c.set(cx + 4, cy + 1, WHITE);
            c.set(cx + 1 + (ph / 2) % 2, cy - 2, GRAY);
            c.set(cx + 2 - (ph / 2) % 2, cy - 3, GRAY);
        }
        Prop::Cookie => {
            c.rect(CX + 7, top + 5, 3, 3, BROWN);
            c.set(CX + 8, top + 6, DARK);
        }
        Prop::Balloon => {
            let by = top - 12 - (ph % 4) / 2;
            c.rect(CX + 5, by + 1, 5, 4, PURPLE);
            c.rect(CX + 6, by, 3, 6, PURPLE);
            c.set(CX + 6, by + 1, WHITE);
            c.set(CX + 7, by + 6, GRAY);
            c.rect(CX + 7, by + 7, 1, 6, GRAY);
        }
        Prop::Wand => {
            c.rect(CX + 7, top + 4, 1, 5, GRAY);
            c.rect(CX + 6, top + 3, 3, 1, GRAY);
        }
        Prop::Plane => {
            let px = CX + 4 + (ph % 6);
            let py = top - 3 - (ph % 6) / 2;
            c.rect(px, py, 4, 1, WHITE);
            c.rect(px, py + 1, 2, 1, GRAY);
            c.set(px + 3, py - 1, WHITE);
        }
        Prop::Telescope => {
            c.rect(CX + 3, top - 2, 7, 2, GRAY);
            c.rect(CX + 9, top - 3, 2, 4, DGRAY);
            c.rect(CX + 2, top, 2, 3, BROWN);
        }
        Prop::Calculator => {
            c.rect(CX - 3, BODY_BOTTOM - 4, 7, 5, DGRAY);
            c.rect(CX - 2, BODY_BOTTOM - 3, 5, 1, GREEN);
            c.set(CX - 2, BODY_BOTTOM - 1, GRAY);
            c.set(CX, BODY_BOTTOM - 1, GRAY);
            c.set(CX + 2, BODY_BOTTOM - 1, RED);
        }
        Prop::Cloud => {
            let cy = top - 6;
            c.rect(CX - 4, cy + 1, 9, 2, GRAY);
            c.rect(CX - 2, cy, 5, 1, GRAY);
        }
        Prop::Trophy => {
            let (tx, ty) = (CX + 7, top - 2);
            c.rect(tx, ty, 4, 3, YELLOW);
            c.rect(tx + 1, ty + 3, 2, 2, YELLOW);
            c.rect(tx, ty + 5, 4, 1, BROWN);
        }
        Prop::None => {}
    }

    // effects around the head
    match p.fx {
        Fx::Zzz => {
            let n = (ph / 3) % 3;
            for i in 0..=n {
                let (zx, zy) = (CX + 5 + i * 2, top - 2 - i * 3);
                c.rect(zx, zy, 3, 1, WHITE);
                c.set(zx + 1, zy + 1, WHITE);
                c.rect(zx, zy + 2, 3, 1, WHITE);
            }
        }
        Fx::Hearts => {
            let hy = top - 4 - (ph % 6) / 2;
            c.rect(CX + 5, hy, 2, 1, RED);
            c.rect(CX + 8, hy, 2, 1, RED);
            c.rect(CX + 5, hy + 1, 5, 1, RED);
            c.rect(CX + 6, hy + 2, 3, 1, RED);
            c.set(CX + 7, hy + 3, RED);
        }
        Fx::Notes => {
            let ny = top - 4 - (ph % 4);
            let nx = CX + 6 + (ph % 2) * 2;
            c.rect(nx + 2, ny, 1, 4, WHITE);
            c.rect(nx, ny + 3, 2, 2, WHITE);
            c.rect(nx + 2, ny, 2, 1, WHITE);
        }
        Fx::Stars => {
            for (i, (dx, dy)) in [(-6, -4), (0, -6), (6, -4), (0, -2)].into_iter().enumerate() {
                if (ph as usize + i) % 4 == 0 {
                    c.rect(CX + dx, top + dy + 1, 3, 1, YELLOW);
                    c.rect(CX + dx + 1, top + dy, 1, 3, YELLOW);
                }
            }
        }
        Fx::Question => {
            let (qx, qy) = (CX + 6, top - 6);
            c.rect(qx, qy, 3, 1, YELLOW);
            c.set(qx + 2, qy + 1, YELLOW);
            c.set(qx + 1, qy + 2, YELLOW);
            c.set(qx + 1, qy + 4, YELLOW);
        }
        Fx::Exclaim => {
            c.rect(CX + 6, top - 8, 2, 4, YELLOW);
            c.rect(CX + 6, top - 3, 2, 2, YELLOW);
        }
        Fx::Sweat => {
            let sy = top + 1 + (ph % 6) / 2;
            c.rect(CX + 7, sy, 1, 2, BLUE);
            c.set(CX + 7, sy + 2, BLUE);
        }
        Fx::Rain => {
            for i in 0..3 {
                let ry = top - 3 + ((ph + i * 2) % 6) * 2;
                c.set(CX - 3 + i * 3, ry, BLUE);
                c.set(CX - 3 + i * 3, ry + 1, BLUE);
            }
        }
        Fx::Steam => {
            let s = ph % 4;
            c.set(CX - 6, top - 1 - s, WHITE);
            c.set(CX + 6, top - 1 - (s + 2) % 4, WHITE);
            c.rect(CX - 6, top - 2 - s, 1, 1, GRAY);
        }
        Fx::Confetti => {
            for i in 0..8 {
                let x = (CX - 9 + (i * 5 + ph * 3) % 19).clamp(0, CW as i32 - 1);
                let y = (i * 3 + ph * 2) % 17;
                c.set(x, y, [RED, YELLOW, BLUE, GREEN, PURPLE][i as usize % 5]);
            }
        }
        Fx::Sparkles => {
            let n = ph % 3;
            c.rect(CX + 7, top - 3 + n, 3, 1, YELLOW);
            c.rect(CX + 8, top - 4 + n, 1, 3, YELLOW);
            c.rect(CX - 9, top + 1 - n, 3, 1, WHITE);
            c.rect(CX - 8, top - n, 1, 3, WHITE);
        }
        Fx::Bubbles => {
            for i in 0..3 {
                let by = top - 2 - ((ph + i * 2) % 8);
                let bx = CX + 6 + i * 2 + (ph % 2);
                c.rect(bx, by, 2, 2, WHITE);
                c.set(bx, by, LIGHT);
            }
        }
        Fx::Hiccup => {
            c.set(CX + 7, top - 2, WHITE);
            c.set(CX + 8, top - 3, WHITE);
            c.set(CX + 9, top - 2, WHITE);
        }
        Fx::None => {}
    }
    c
}

/// Draws the canvas with its top-left corner at `(x, y)`.
fn blit(fb: &mut Framebuffer, canvas: &Canvas, x: i32, y: i32, scale: usize, flip: bool) {
    for row in 0..CH {
        let mut col = 0;
        while col < CW {
            let at = |c: usize| canvas.px[row * CW + if flip { CW - 1 - c } else { c }];
            let index = at(col);
            if index == 0 {
                col += 1;
                continue;
            }
            let mut run = 1;
            while col + run < CW && at(col + run) == index {
                run += 1;
            }
            let px = x + (col * scale) as i32;
            let py = y + (row * scale) as i32;
            if px >= 0 && py >= 0 {
                fb.fill_rect(px as usize, py as usize, run * scale, scale, palette(index));
            }
            col += run;
        }
    }
}

// ---------------------------------------------------------------- the engine

/// What the system is doing right now.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Context {
    Boot,
    Onboarding,
    Shell,
    Editing,
    /// A script that is a game of action (pong, snake, invaders).
    ActionGame,
    /// A turn-based game (tic-tac-toe, connect four).
    BoardGame,
    Calculator,
    /// Some other script.
    Script,
    Demo3d,
    Doom,
}

/// Things that happen that Bit notices.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Boot,
    Success,
    Failure,
    Surprise,
    /// A game: you scored / got hurt / won / lost / are in danger.
    Score,
    Hurt,
    Win,
    Lose,
    Danger,
}

struct Step {
    activity: Activity,
    mood: Mood,
    ms: u64,
    /// Walk speed in lane permille per second (negative: leftwards), 0 to stay.
    speed: i32,
}

pub struct Buddy {
    activity: Activity,
    mood: Mood,
    /// -1 calm, 0 normal, 1 lively
    intensity: i32,
    started: u64,
    until: u64,
    /// Position along the lane, 0 to 1000.
    pos: i32,
    /// 1 faces right, -1 faces left.
    dir: i32,
    speed: i32,
    rng: u32,
    last_update: u64,
    last_second: u64,
    context: Context,
    // the inner life
    energy: i32,
    valence: i32,
    boredom: i32,
    curiosity: i32,
    /// The last activities, newest last, so it does not repeat itself.
    recent: Vec<Activity>,
    queue: Vec<Step>,
    thought: Option<(String, u64)>,
    next_thought: u64,
    keys_seen: u32,
    keys_at_start: u32,
    command_count: u32,
    name: String,
    /// 0 quiet, 1 normal, 2 chatty
    chatty: u8,
    /// A behavior chosen on purpose is shown until this time.
    forced_until: u64,
}

impl Buddy {
    fn rand(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    fn new(seed: u32, now: u64) -> Buddy {
        Buddy {
            activity: Activity::Idle,
            mood: Mood::Neutral,
            intensity: 0,
            started: now,
            until: now + 1500,
            pos: 500,
            dir: 1,
            speed: 0,
            rng: seed | 1,
            last_update: now,
            last_second: now / 1000,
            context: Context::Boot,
            energy: 80,
            valence: 15,
            boredom: 20,
            curiosity: 40,
            recent: Vec::new(),
            queue: Vec::new(),
            thought: None,
            next_thought: now + 4000,
            keys_seen: 0,
            keys_at_start: 0,
            command_count: 0,
            name: String::new(),
            chatty: 1,
            forced_until: 0,
        }
    }

    fn start(&mut self, activity: Activity, mood: Mood, ms: u64, now: u64) {
        self.activity = activity;
        self.mood = mood;
        self.started = now;
        self.until = now + ms;
        self.keys_at_start = self.keys_seen;
        self.intensity = if self.energy > 72 {
            1
        } else if self.energy < 35 {
            -1
        } else {
            0
        };
        self.recent.push(activity);
        if self.recent.len() > 10 {
            self.recent.remove(0);
        }
        use Activity::*;
        if matches!(activity, Walk | Run | Tiptoe | Skip | Pace | Peek) {
            self.dir = if self.pos > 700 {
                -1
            } else if self.pos < 300 {
                1
            } else if self.rand() % 2 == 0 {
                1
            } else {
                -1
            };
            self.speed = match activity {
                Run => 220,
                Skip => 110,
                Tiptoe => 40,
                _ => 70,
            } + (self.rand() % 30) as i32;
        } else {
            self.speed = 0;
        }
    }

    /// The mood Bit shows, from how it is doing inside.
    fn current_mood(&self) -> Mood {
        if self.energy < 25 {
            Mood::Tired
        } else if self.valence < -30 {
            Mood::Grumpy
        } else if self.valence > 45 {
            Mood::Happy
        } else if self.boredom > 70 && self.energy > 45 {
            Mood::Excited
        } else if self.curiosity > 65 {
            Mood::Curious
        } else {
            Mood::Neutral
        }
    }

    /// How much Bit feels like doing `a` right now. Zero means never here.
    fn fitness(&self, a: Activity, typing_recently: bool) -> u32 {
        use Activity::*;
        use Context::*;
        let ctx = self.context;
        let base: i32 = match ctx {
            Editing => match a {
                TypeLaptop => 60,
                Think => 8,
                SipCoffee | SipTea => 6,
                WriteNotes => 6,
                Debug => 5,
                ReadBook => 3,
                Stretch | Yawn => 3,
                LookAround | ScratchHead => 3,
                Nod => 3,
                Idle => 2,
                _ => 0,
            },
            ActionGame => match a {
                PlayGame => 70,
                Wink | Nod => 3,
                Stretch => 1,
                SipCoffee => 2,
                _ => 0,
            },
            BoardGame => match a {
                PlayGame => 34,
                Think => 18,
                ScratchHead => 6,
                LookAround => 5,
                SipTea | SipCoffee => 4,
                Nod | Confused => 3,
                _ => 0,
            },
            Calculator => match a {
                Calculate => 60,
                Think => 8,
                ScratchHead | Nod => 4,
                WriteNotes => 6,
                _ => 0,
            },
            Demo3d => match a {
                Stargaze => 14,
                Listen => 16,
                Dance => 14,
                Present => 8,
                Sway | Hum | Clap => 8,
                Point | Wave | Cheer => 4,
                LookAround => 4,
                _ => 0,
            },
            Doom => match a {
                PlayGame => 80,
                Scared => 3,
                Surprised => 3,
                Angry => 2,
                _ => 0,
            },
            Script => match a {
                Think | ReadBook | Debug | LookAround | Nod | Build | WriteNotes => 8,
                _ => 0,
            },
            Boot | Onboarding => match a {
                Idle => 4,
                LookAround => 6,
                Wave => 4,
                Hop => 4,
                Sway => 4,
                _ => 0,
            },
            Shell => match a {
                Idle => 16,
                LookAround => 10,
                Hum => 5,
                Whistle => 4,
                Stretch => 3,
                Yawn => 2,
                ScratchHead => 3,
                TapFoot => 4,
                Sway => 4,
                Sigh => 2,
                Hiccup => 1,
                Sneeze => 1,
                Shiver => 1,
                Walk => 18,
                Run => 3,
                Tiptoe => 3,
                Skip => 3,
                Hop => 3,
                Spin => 2,
                Peek => 3,
                Pace => 4,
                Wave => 3,
                Bow => 1,
                Nod => 2,
                Shrug => 2,
                ThumbsUp => 1,
                Wink => 2,
                Flex => 1,
                ReadBook => 6,
                WriteNotes => 4,
                Debug => 3,
                Build => 3,
                Think => 6,
                Calculate => 2,
                SipCoffee => 5,
                SipTea => 5,
                Snack => 4,
                PlayGame => 2,
                Dance => 3,
                Juggle => 3,
                Balloon => 4,
                Bubbles => 3,
                Stargaze => 2,
                PaperPlane => 3,
                Sleep => 2,
                Nap => 3,
                Dream => 1,
                Confused => 1,
                Listen => 4,
                TypeLaptop => {
                    if typing_recently {
                        40
                    } else {
                        2
                    }
                }
                _ => 0,
            },
        };
        if base == 0 {
            return 0;
        }
        let mut w = base * 10;
        // energy: tired Bit rests, energetic Bit moves
        let restful = matches!(a, Sleep | Nap | Dream | Yawn | Sigh | SipCoffee | SipTea | Snack | Sway);
        let lively = matches!(a, Run | Skip | Hop | Spin | Dance | Juggle | Flex | Cheer);
        if self.energy < 30 {
            if restful {
                w *= 3;
            }
            if lively {
                w /= 4;
            }
        }
        if self.energy > 70 && lively {
            w = w * 3 / 2;
        }
        if self.energy > 55 && matches!(a, Sleep | Nap) {
            w /= 6;
        }
        if self.energy < 15 && matches!(ctx, Shell) && matches!(a, Sleep | Nap | Dream) {
            w *= 6;
        }
        // boredom wants something to do
        if self.boredom > 60 && matches!(a, Spin | Hop | Juggle | Balloon | Bubbles | PaperPlane | Dance | Skip | Run | Peek) {
            w *= 2;
        }
        // curiosity: looking and tinkering
        if self.curiosity > 60 && matches!(a, LookAround | Debug | Stargaze | Peek | ReadBook | Think) {
            w = w * 3 / 2;
        }
        // a good mood hums and dances, a bad one sighs
        if self.valence > 40 && matches!(a, Hum | Whistle | Dance | Skip | Sway | Wave | ThumbsUp | Love) {
            w *= 2;
        }
        if self.valence < -20 && matches!(a, Sigh | Sad | Shiver | Shrug | ShakeHead) {
            w *= 3;
        }
        if self.valence < -20 && matches!(a, Dance | Cheer | Skip | Hop) {
            w /= 4;
        }
        // not what it just did
        if let Some(age) = self.recent.iter().rev().position(|r| *r == a) {
            w = w * (age as i32 + 1) / 11;
        }
        w.max(1) as u32
    }

    fn pick(&mut self, now: u64) {
        let typing_recently = now.saturating_sub(LAST_KEY_MS.load(Ordering::Relaxed)) < 4000;
        let weights: Vec<(Activity, u32)> = ACTIVITIES
            .iter()
            .map(|(a, _)| (*a, self.fitness(*a, typing_recently)))
            .filter(|(_, w)| *w > 0)
            .collect();
        let total: u32 = weights.iter().map(|(_, w)| *w).sum();
        if total == 0 {
            self.start(Activity::Idle, Mood::Neutral, 2000, now);
            return;
        }
        let mut roll = self.rand() % total;
        let mut chosen = Activity::Idle;
        for (a, w) in weights {
            if roll < w {
                chosen = a;
                break;
            }
            roll -= w;
        }
        let ms = self.duration_of(chosen);
        let mood = self.current_mood();
        self.start(chosen, mood, ms, now);
    }

    fn duration_of(&mut self, a: Activity) -> u64 {
        use Activity::*;
        let (base, spread): (u64, u64) = match a {
            Idle => (2200, 3000),
            LookAround | Think | ReadBook | WriteNotes | Debug | Build | Calculate => (3000, 3500),
            Sleep | Nap => (6500, 6000),
            Dream => (5000, 3000),
            TypeLaptop => (4500, 4500),
            PlayGame => (6000, 5000),
            Walk | Run | Tiptoe | Skip | Pace | Peek => (3000, 4000),
            Yawn => (2000, 400),
            Hiccup | Sneeze | Hop | Spin | Surprised | Wink => (1500, 1200),
            Celebrate | Cheer | Proud => (2400, 1500),
            _ => (2600, 2800),
        };
        base + self.rand() as u64 % spread
    }

    /// Moves Bit on to `now`: its inner life, walking, and choosing what to do next.
    fn advance(&mut self, now: u64) {
        let dt = now.saturating_sub(self.last_update).min(500) as i32;
        self.last_update = now;
        self.keys_seen = KEYS.load(Ordering::Relaxed);

        // once a second the inner life ticks over
        if now / 1000 != self.last_second {
            self.last_second = now / 1000;
            self.tick_inner(now);
        }

        use Activity::*;
        if matches!(self.activity, Walk | Run | Tiptoe | Skip | Pace | Peek) && self.speed != 0 {
            self.pos += self.dir * self.speed * dt / 1000;
            if self.pos <= 0 || self.pos >= 1000 {
                self.pos = self.pos.clamp(0, 1000);
                self.dir = -self.dir;
                if self.queue.is_empty() && now + 400 < self.until {
                    let mood = self.current_mood();
                    self.start(LookAround, mood, 1600, now);
                }
            }
        }

        // typing: while keys are coming in, Bit types along
        let typing_recently = now.saturating_sub(LAST_KEY_MS.load(Ordering::Relaxed)) < 1200;
        if typing_recently
            && matches!(self.context, Context::Shell | Context::Editing)
            && !matches!(self.activity, TypeLaptop)
            && self.forced_until <= now
            && self.queue.is_empty()
        {
            self.start(TypeLaptop, Mood::Neutral, 3500, now);
            if self.context == Context::Shell {
                self.say_maybe(now, Topic::Typing);
            }
        }
        // in a game: headset on and eyes on the screen
        if matches!(self.context, Context::ActionGame | Context::BoardGame | Context::Doom)
            && !matches!(
                self.activity,
                PlayGame | Cheer | Sad | Surprised | Scared | Celebrate | Angry | Proud | Think | Wink | Nod | Confused | ScratchHead | SipTea | SipCoffee | LookAround | Stretch | Love
            )
            && self.forced_until <= now
        {
            self.start(PlayGame, Mood::Excited, 6000, now);
        }

        if now >= self.until && self.forced_until <= now {
            if self.queue.is_empty() {
                self.pick(now);
            } else {
                let step = self.queue.remove(0);
                self.start(step.activity, step.mood, step.ms, now);
                if step.speed != 0 {
                    self.speed = step.speed.abs();
                    self.dir = if step.speed > 0 { 1 } else { -1 };
                }
            }
            self.maybe_think(now);
        }
        if let Some((_, until)) = &self.thought {
            if now >= *until {
                self.thought = None;
            }
        }
    }

    fn tick_inner(&mut self, now: u64) {
        use Activity::*;
        let quiet = now.saturating_sub(LAST_KEY_MS.load(Ordering::Relaxed)) > 8000;
        match self.activity {
            Sleep | Nap | Dream => self.energy = (self.energy + 5).min(100),
            SipCoffee | SipTea | Snack => self.energy = (self.energy + 2).min(100),
            Run | Spin | Dance | Skip | Hop | Juggle | PlayGame => self.energy = (self.energy - 1).max(0),
            _ => {
                if self.last_second % 3 == 0 {
                    self.energy = (self.energy - 1).max(0);
                }
            }
        }
        if quiet && matches!(self.context, Context::Shell) {
            self.boredom = (self.boredom + 1).min(100);
        } else {
            self.boredom = (self.boredom - 2).max(0);
        }
        let up = if self.rand() % 3 == 0 { 1 } else { 0 };
        let down = if self.rand() % 4 == 0 { 1 } else { 0 };
        self.curiosity = (self.curiosity + up - down).clamp(0, 100);
        // feelings drift back to calm
        if self.valence > 0 {
            self.valence -= 1;
        } else if self.valence < 0 {
            self.valence += 1;
        }
        if matches!(self.activity, Hum | Dance | Whistle | Sway) {
            self.valence = (self.valence + 1).min(100);
        }
    }

    /// Reacts to something the system tells it.
    fn react(&mut self, event: Event, now: u64) {
        use Activity::*;
        match event {
            Event::Boot => {
                self.queue.clear();
                self.pos = 0;
                self.dir = 1;
                self.valence = 30;
                self.energy = 90;
                self.start(Walk, Mood::Happy, 2300, now);
                self.speed = 170;
                self.forced_until = now + 2300;
                self.queue.push(Step { activity: Wave, mood: Mood::Happy, ms: 3200, speed: 0 });
                self.queue.push(Step { activity: Hop, mood: Mood::Excited, ms: 1500, speed: 0 });
                self.queue.push(Step { activity: Idle, mood: Mood::Happy, ms: 1500, speed: 0 });
                return;
            }
            Event::Success => {
                self.valence = (self.valence + 12).min(100);
                self.boredom = 0;
                let a = [Cheer, ThumbsUp, Nod, Wink, Clap, Proud][(self.rand() % 6) as usize];
                self.queue.clear();
                self.start(a, Mood::Happy, 1500, now);
                self.forced_until = now + 900;
                self.say_maybe(now, Topic::Success);
            }
            Event::Failure => {
                self.valence = (self.valence - 14).max(-100);
                let a = [Sad, Confused, Shrug, ShakeHead, Embarrassed, Sigh, Surprised][(self.rand() % 7) as usize];
                self.queue.clear();
                self.start(a, Mood::Grumpy, 2000, now);
                self.forced_until = now + 1000;
                self.say_maybe(now, Topic::Failure);
            }
            Event::Surprise => {
                self.start(Surprised, Mood::Excited, 1400, now);
                self.forced_until = now + 700;
            }
            Event::Score => {
                self.valence = (self.valence + 8).min(100);
                let a = [Cheer, ThumbsUp, Clap, Hop][(self.rand() % 4) as usize];
                self.start(a, Mood::Happy, 1100, now);
                self.forced_until = now + 700;
            }
            Event::Hurt => {
                self.valence = (self.valence - 8).max(-100);
                let a = [Surprised, Scared, Dizzy, Sad][(self.rand() % 4) as usize];
                self.start(a, Mood::Grumpy, 1300, now);
                self.forced_until = now + 800;
            }
            Event::Win => {
                self.valence = 90;
                self.start(Celebrate, Mood::Excited, 3500, now);
                self.forced_until = now + 3000;
                self.say_now(now, Topic::Win);
            }
            Event::Lose => {
                self.valence = (self.valence - 25).max(-100);
                let a = [Sad, Angry, Embarrassed][(self.rand() % 3) as usize];
                self.start(a, Mood::Grumpy, 2800, now);
                self.forced_until = now + 2500;
                self.say_now(now, Topic::Lose);
            }
            Event::Danger => {
                let a = [Scared, Surprised][(self.rand() % 2) as usize];
                self.start(a, Mood::Excited, 900, now);
                self.forced_until = now + 500;
            }
        }
    }

    /// Bit sees a command being run and has an opinion about it.
    fn observe(&mut self, command: &str, ok: bool, now: u64) {
        self.command_count += 1;
        let first = command.split_whitespace().next().unwrap_or("");
        let second = command.split_whitespace().nth(1).unwrap_or("");
        let topic = match first {
            "help" => Some(Topic::Help),
            "ls" => Some(Topic::List),
            "cd" => Some(Topic::Travel),
            "rm" => Some(Topic::Delete),
            "clear" => Some(Topic::Clear),
            "run" | "exec" | "eval" => {
                if matches!(second, "pong" | "snake" | "invaders" | "tictactoe" | "connect4") {
                    Some(Topic::Game)
                } else {
                    Some(Topic::Script)
                }
            }
            "3d" | "cube" => Some(Topic::Demo),
            "doom" => Some(Topic::Doom),
            "edit" => Some(Topic::Edit),
            "mem" | "uptime" | "screen" | "disk" => Some(Topic::Info),
            "res" => Some(Topic::Resolution),
            "color" => Some(Topic::Color),
            "panic" => Some(Topic::Panic),
            "reboot" | "shutdown" => Some(Topic::Goodbye),
            "write" | "touch" | "mkdir" | "cp" | "mv" => Some(Topic::Files),
            _ => None,
        };
        if self.command_count % 25 == 0 {
            self.say_now(now, Topic::Milestone);
        } else if let Some(topic) = topic {
            if ok {
                self.say_maybe(now, topic);
            }
        }
        // some commands change how Bit feels
        match first {
            "help" => self.curiosity = (self.curiosity + 10).min(100),
            "panic" => self.valence = (self.valence - 30).max(-100),
            "clear" => self.boredom = 0,
            _ => {}
        }
    }

    fn maybe_think(&mut self, now: u64) {
        if now < self.next_thought || self.chatty == 0 {
            return;
        }
        let gap: u32 = match self.chatty {
            2 => 6000,
            _ => 16000,
        };
        self.next_thought = now + gap as u64 + (self.rand() % gap) as u64;
        let topic = match self.activity {
            Activity::TypeLaptop => Topic::Typing,
            Activity::PlayGame => Topic::Playing,
            Activity::Sleep | Activity::Nap | Activity::Dream => Topic::Sleeping,
            Activity::SipCoffee | Activity::SipTea => Topic::Drinking,
            Activity::ReadBook => Topic::Reading,
            Activity::Think => Topic::Thinking,
            Activity::Walk | Activity::Run | Activity::Skip | Activity::Tiptoe | Activity::Pace => Topic::Walking,
            Activity::Dance | Activity::Hum | Activity::Listen => Topic::Music,
            Activity::Stargaze => Topic::Stars,
            _ => Topic::Idle,
        };
        self.say_maybe(now, topic);
    }

    fn say_maybe(&mut self, now: u64, topic: Topic) {
        if self.chatty == 0 || self.thought.is_some() {
            return;
        }
        if self.chatty == 1 && self.rand() % 3 != 0 {
            return;
        }
        self.say_now(now, topic);
    }

    fn say_now(&mut self, now: u64, topic: Topic) {
        if self.chatty == 0 {
            return;
        }
        let lines = topic.lines();
        let line = lines[(self.rand() as usize) % lines.len()];
        let text = if line.contains("{name}") {
            let name = if self.name.is_empty() { "friend" } else { self.name.as_str() };
            line.replace("{name}", name)
        } else {
            String::from(line)
        };
        let ms = 2200 + text.len() as u64 * 55;
        self.thought = Some((text, now + ms));
    }
}

/// What a thought is about.
#[derive(Clone, Copy)]
enum Topic {
    Idle,
    Typing,
    Playing,
    Sleeping,
    Drinking,
    Reading,
    Thinking,
    Walking,
    Music,
    Stars,
    Success,
    Failure,
    Win,
    Lose,
    Help,
    List,
    Travel,
    Delete,
    Clear,
    Game,
    Script,
    Demo,
    Doom,
    Edit,
    Info,
    Resolution,
    Color,
    Panic,
    Goodbye,
    Files,
    Milestone,
}

impl Topic {
    fn lines(self) -> &'static [&'static str] {
        match self {
            Topic::Idle => &[
                "hmm.", "nice day for bits", "i wonder what's in /", "did you hear that? ...no.", "{name}, you've got this",
                "pixels are my favourite", "still here!", "la la la", "what shall we make?", "tab completes things, you know",
                "i counted my legs: four", "the cursor blinks. i blink back",
            ],
            Topic::Typing => &[
                "tap tap tap", "ooh, fast typer", "i'll keep up!", "don't forget to save", "that's a lot of keys", "semicolons are brave",
                "typing with you", "clack clack",
            ],
            Topic::Playing => &["just one more round", "focus mode", "gg", "i can hear my own heartbeat", "headset on, world off", "pew pew"],
            Topic::Sleeping => &["zzz...", "five more minutes", "dreaming of cursors", "snore.exe", "mmm... pancakes"],
            Topic::Drinking => &["fresh brew", "sip.", "caffeine loaded", "tea time", "careful, hot"],
            Topic::Reading => &["chapter 3 is wild", "the manual, again", "no spoilers", "page turner", "so that's how it works"],
            Topic::Thinking => &["hm, what if...", "processing feelings", "aha?", "thinking thinking", "let me ponder"],
            Topic::Walking => &["a stroll", "left, right, left", "exploring the status bar", "leg day", "nothing to see here"],
            Topic::Music => &["this beat though", "la la la", "turn it up", "my favourite track", "dancing is just walking with style"],
            Topic::Stars => &["so many pixels up there", "is that a constellation?", "shooting star!", "wish granted"],
            Topic::Success => &["nice one!", "that worked!", "smooth", "{name} is on a roll", "flawless", "easy", "ding!"],
            Topic::Failure => &["oops", "that didn't work", "it happens", "try again?", "typo? probably a typo", "uh oh", "don't worry, {name}"],
            Topic::Win => &["we won!", "champion!", "that was amazing", "victory dance time", "gg, {name}!"],
            Topic::Lose => &["so close!", "rematch?", "next time", "we'll get it", "ouch"],
            Topic::Help => &["need a hand?", "ah, the manual", "good idea to ask", "so many commands"],
            Topic::List => &["what's in there?", "peeking inside", "neat and tidy", "ooh, files"],
            Topic::Travel => &["going somewhere?", "off we go", "new directory, new vibes", "cd, my favourite two letters"],
            Topic::Delete => &["bye bye file", "careful with that", "gone", "tidying up!"],
            Topic::Clear => &["fresh start", "ahh, clean slate", "where did everything go?"],
            Topic::Game => &["game time!", "can i play too?", "my headset is ready", "good luck, {name}"],
            Topic::Script => &["running a script", "tiny program, big dreams", "zs is neat"],
            Topic::Demo => &["ooh, shiny", "look at it go", "math is pretty", "3d! in my terminal?"],
            Topic::Doom => &["rip and tear... carefully", "mind the imps", "this is a classic", "find the keys"],
            Topic::Edit => &["writing time", "i'll type along", "save early, save often", "what are we writing?"],
            Topic::Info => &["numbers!", "stats, stats, stats", "everything looks healthy"],
            Topic::Resolution => &["how's the view?", "new resolution, new me", "i grew! or shrank?"],
            Topic::Color => &["pretty", "color me impressed", "good taste, {name}"],
            Topic::Panic => &["WHY", "oh no oh no oh no", "{name}!!", "that was on purpose, right?"],
            Topic::Goodbye => &["see you soon!", "bye {name}!", "i'll miss you", "sleep tight"],
            Topic::Files => &["files!", "organised, i like it", "saved for later", "tidy"],
            Topic::Milestone => &["25 commands! look at us", "we're getting good at this", "you type, i watch. teamwork"],
        }
    }
}

// ---------------------------------------------------------------- public interface

static BUDDY: Mutex<Option<Buddy>> = Mutex::new(None);

fn with_buddy<R>(f: impl FnOnce(&mut Buddy, u64) -> R) -> R {
    let now = crate::time::uptime_ms();
    let mut guard = BUDDY.lock();
    let buddy = guard.get_or_insert_with(|| {
        let mut b = Buddy::new(crate::time::ticks() as u32 ^ (now as u32).wrapping_mul(2654435761) ^ 0xB17B_17B1, now);
        if let Some(name) = crate::settings::get("name") {
            b.name = name;
        }
        if let Some(chat) = crate::settings::get("chatty") {
            b.chatty = chat.parse().unwrap_or(1);
        }
        b
    });
    f(buddy, now)
}

/// Starts the greeting: Bit walks in and waves.
pub fn greet() {
    with_buddy(|b, now| b.react(Event::Boot, now));
}

pub fn react(event: Event) {
    with_buddy(|b, now| b.react(event, now));
}

/// Tells Bit which screen is showing.
pub fn set_context(context: Context) {
    with_buddy(|b, now| {
        if b.context != context {
            b.context = context;
            // a change of scene deserves a reaction
            b.forced_until = 0;
            b.queue.clear();
            match context {
                Context::ActionGame | Context::BoardGame | Context::Doom => {
                    b.start(Activity::PlayGame, Mood::Excited, 5000, now);
                    b.say_maybe(now, Topic::Game);
                }
                Context::Editing => {
                    b.start(Activity::TypeLaptop, Mood::Neutral, 4000, now);
                    b.say_maybe(now, Topic::Edit);
                }
                Context::Calculator => b.start(Activity::Calculate, Mood::Neutral, 4000, now),
                Context::Demo3d => {
                    b.start(Activity::Present, Mood::Excited, 2500, now);
                    b.say_maybe(now, Topic::Demo);
                }
                Context::Shell => b.until = now, // choose afresh
                _ => {}
            }
        }
    });
}

/// Bit watches a command go by.
pub fn observe_command(command: &str, ok: bool) {
    with_buddy(|b, now| b.observe(command, ok, now));
}

/// Sets what Bit calls you (it uses it in thoughts).
pub fn set_name(name: &str) {
    with_buddy(|b, _| b.name = String::from(name));
}

/// 0 quiet, 1 normal, 2 chatty.
pub fn set_chatty(level: u8) {
    with_buddy(|b, _| b.chatty = level.min(2));
}

/// Scripts the next few things Bit does (the welcome tour uses this).
pub fn script(steps: &[(Activity, Mood, u64)]) {
    with_buddy(|b, now| {
        b.queue.clear();
        let mut first = true;
        for &(activity, mood, ms) in steps {
            if first {
                b.start(activity, mood, ms, now);
                b.forced_until = now + ms;
                first = false;
            } else {
                b.queue.push(Step { activity, mood, ms, speed: 0 });
            }
        }
    });
}

/// Makes Bit do something right now (`buddy <name>`). Returns whether it
/// knew the activity.
pub fn perform(name: &str, mood: Option<Mood>) -> bool {
    match ACTIVITIES.iter().find(|(_, n)| *n == name) {
        Some(&(activity, _)) => {
            with_buddy(|b, now| {
                let ms = b.duration_of(activity).max(3000);
                b.queue.clear();
                let mood = mood.unwrap_or_else(|| b.current_mood());
                b.start(activity, mood, ms, now);
                b.forced_until = now + ms;
            });
            true
        }
        None => false,
    }
}

/// Puts a thought in Bit's head.
pub fn say(text: &str) {
    with_buddy(|b, now| {
        let ms = 2500 + text.len() as u64 * 60;
        b.thought = Some((String::from(text), now + ms));
    });
}

/// What Bit is saying right now, if anything.
pub fn thought() -> Option<String> {
    with_buddy(|b, now| match &b.thought {
        Some((text, until)) if now < *until => Some(text.clone()),
        _ => None,
    })
}

/// A snapshot of Bit's state for the `buddy` command.
pub struct Status {
    pub activity: &'static str,
    pub mood: &'static str,
    pub intensity: &'static str,
    pub behavior_id: usize,
    pub energy: i32,
    pub valence: i32,
    pub boredom: i32,
    pub curiosity: i32,
    pub context: Context,
}

pub fn status() -> Status {
    with_buddy(|b, _| {
        let name = ACTIVITIES.iter().find(|(a, _)| *a == b.activity).map(|(_, n)| *n).unwrap_or("idle");
        let ai = ACTIVITIES.iter().position(|(a, _)| *a == b.activity).unwrap_or(0);
        let ii = (b.intensity + 1) as usize;
        Status {
            activity: name,
            mood: MOOD_NAMES[mood_index(b.mood)],
            intensity: ["calm", "normal", "lively"][ii],
            behavior_id: (ai * MOODS.len() + mood_index(b.mood)) * 3 + ii + 1,
            energy: b.energy,
            valence: b.valence,
            boredom: b.boredom,
            curiosity: b.curiosity,
            context: b.context,
        }
    })
}

/// Parses a mood name.
pub fn mood_from(name: &str) -> Option<Mood> {
    MOOD_NAMES.iter().position(|n| *n == name).map(|i| MOODS[i])
}

/// Advances Bit and draws it standing on `floor_y`, somewhere along the lane
/// `lane_x..lane_x + lane_w` (screen pixels). Draws only Bit; the caller has
/// already painted the background (and may have set a clip rectangle).
pub fn draw(fb: &mut Framebuffer, lane_x: usize, lane_w: usize, floor_y: usize, scale: usize) {
    let (pose_now, pos, dir) = with_buddy(|b, now| {
        b.advance(now);
        let t = now.saturating_sub(b.started);
        let typing = b.keys_seen.wrapping_sub(b.keys_at_start);
        let mut p = pose(b.activity, t, b.intensity, typing);
        color_with_mood(&mut p, b.mood, t);
        (p, b.pos, b.dir)
    });
    let sprite_w = CW * scale;
    let room = lane_w.saturating_sub(sprite_w);
    let x = lane_x + room * pos as usize / 1000;
    let y = floor_y as i32 - (CH * scale) as i32 - pose_now.lift * scale as i32;
    let canvas = paint(&pose_now);
    blit(fb, &canvas, x as i32, y, scale, dir < 0);
}

/// Where Bit stands in the lane right now (screen x of its left edge).
pub fn screen_x(lane_x: usize, lane_w: usize, scale: usize) -> usize {
    let pos = with_buddy(|b, _| b.pos);
    lane_x + lane_w.saturating_sub(CW * scale) * pos as usize / 1000
}

/// The sprite's size in buddy pixels.
pub const SPRITE: (usize, usize) = (CW, CH);

#[test_case]
fn test_there_are_over_a_thousand_behaviors() {
    assert!(behavior_count() > 1000);
}

#[test_case]
fn test_every_activity_draws_in_every_mood() {
    for &(activity, _) in ACTIVITIES.iter() {
        for &mood in MOODS.iter() {
            for calm in -1..=1 {
                for t in (0..3000).step_by(250) {
                    let mut p = pose(activity, t, calm, 3);
                    color_with_mood(&mut p, mood, t);
                    let _ = paint(&p);
                }
            }
        }
    }
}

/// Moves Bit along the lane by hand (the welcome tour lets you steer it).
pub fn nudge(delta: i32) {
    with_buddy(|b, now| {
        b.pos = (b.pos + delta).clamp(0, 1000);
        b.dir = if delta >= 0 { 1 } else { -1 };
        b.activity = Activity::Walk;
        b.started = now;
        b.until = now + 350;
        b.forced_until = now + 350;
        b.speed = 0;
    });
}
