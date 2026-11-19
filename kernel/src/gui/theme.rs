//! Colors of the desktop: a dark and a light appearance plus an accent color.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static DARK: AtomicBool = AtomicBool::new(true);
static ACCENT: AtomicU32 = AtomicU32::new(0x0A84FF);
/// Bumped whenever the appearance changes, so windows know to repaint.
pub static GENERATION: AtomicU32 = AtomicU32::new(0);

pub const ACCENTS: [(&str, u32); 8] = [
    ("Blue", 0x0A84FF),
    ("Purple", 0xBF5AF2),
    ("Pink", 0xFF375F),
    ("Red", 0xFF453A),
    ("Orange", 0xFF9F0A),
    ("Yellow", 0xFFD60A),
    ("Green", 0x30D158),
    ("Graphite", 0x8E8E93),
];

#[derive(Clone, Copy)]
pub struct Theme {
    pub dark: bool,
    pub accent: u32,
    pub window: u32,
    pub sidebar: u32,
    pub titlebar: u32,
    pub control: u32,
    pub control_hover: u32,
    pub control_border: u32,
    pub field: u32,
    pub separator: u32,
    pub text: u32,
    pub text2: u32,
    pub text3: u32,
    pub selection: u32,
    pub menu: u32,
    pub menu_border: u32,
    pub bar_tint: u32,
    pub bar_alpha: u32,
    pub on_accent: u32,
}

pub fn current() -> Theme {
    let accent = ACCENT.load(Ordering::Relaxed);
    if DARK.load(Ordering::Relaxed) {
        Theme {
            dark: true,
            accent,
            window: 0x232428,
            sidebar: 0x1C1D21,
            titlebar: 0x2B2C31,
            control: 0x34363C,
            control_hover: 0x41434A,
            control_border: 0x45474E,
            field: 0x1B1C20,
            separator: 0x3A3C42,
            text: 0xEDEEF0,
            text2: 0xA7AAB1,
            text3: 0x6E717A,
            selection: accent,
            menu: 0x2A2B30,
            menu_border: 0x4A4C53,
            bar_tint: 0x16171B,
            bar_alpha: 150,
            on_accent: 0xFFFFFF,
        }
    } else {
        Theme {
            dark: false,
            accent,
            window: 0xF5F5F7,
            sidebar: 0xE9E9EC,
            titlebar: 0xE8E8EB,
            control: 0xFFFFFF,
            control_hover: 0xEFEFF2,
            control_border: 0xC9C9CE,
            field: 0xFFFFFF,
            separator: 0xD6D6DA,
            text: 0x1D1D1F,
            text2: 0x58585E,
            text3: 0x9A9AA1,
            selection: accent,
            menu: 0xF3F3F5,
            menu_border: 0xC4C4CA,
            bar_tint: 0xF2F2F4,
            bar_alpha: 165,
            on_accent: 0xFFFFFF,
        }
    }
}

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

pub fn set_dark(dark: bool) {
    DARK.store(dark, Ordering::Relaxed);
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

pub fn accent() -> u32 {
    ACCENT.load(Ordering::Relaxed)
}

pub fn set_accent(c: u32) {
    ACCENT.store(c, Ordering::Relaxed);
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// Reads the saved appearance.
pub fn load() {
    if let Some(v) = crate::settings::get("dark") {
        DARK.store(v != "0", Ordering::Relaxed);
    }
    if let Some(v) = crate::settings::get("accent2") {
        if let Ok(c) = u32::from_str_radix(&v, 16) {
            ACCENT.store(c, Ordering::Relaxed);
        }
    }
}

pub fn save() {
    crate::settings::set("dark", if is_dark() { "1" } else { "0" });
    crate::settings::set("accent2", &alloc::format!("{:06X}", accent()));
}
