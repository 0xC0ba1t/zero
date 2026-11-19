//! The Settings application and the Activity monitor.

use super::desktop::{ACCENTS, MAGNIFY, TILING, WALLPAPER, WALLPAPERS};
use super::draw::{self, accent, BG, BG_DARK, BG_HOVER, BG_PANEL, BORDER, TEXT, TEXT_FAINT, TEXT_MUTED};
use super::widgets::{inside, label, slider, switch, AppUi};
use super::Event;
use crate::graphics::display::Display;
use crate::graphics::framebuffer::{self, Framebuffer};
use alloc::string::String;
use core::sync::atomic::Ordering;

use crate::audio::VOLUME;

const PANES: [&str; 5] = ["Appearance", "Desktop", "Display", "Sound", "About"];

pub struct Settings {
    pane: usize,
    mouse: (i32, i32),
    dragging_volume: bool,
}

impl Settings {
    pub fn new() -> Settings {
        if let Some(v) = crate::settings::get("volume") {
            VOLUME.store(v.parse().unwrap_or(80).min(100), Ordering::Relaxed);
        }
        Settings { pane: 0, mouse: (0, 0), dragging_volume: false }
    }
}

const SIDE_W: i32 = 170;

fn swatch_rect(k: usize, base_x: i32, y: i32, size: i32, per_row: usize) -> (i32, i32, i32, i32) {
    let (col, row) = (k % per_row, k / per_row);
    (base_x + col as i32 * (size + 14), y + row as i32 * (size + 34), size, size)
}

impl AppUi for Settings {
    fn event(&mut self, ev: Event, w: i32, _h: i32) -> bool {
        let content_x = SIDE_W + 24;
        match ev {
            Event::MouseMove { x, y } => {
                self.mouse = (x, y);
                if self.dragging_volume {
                    self.set_volume(x, content_x, w);
                }
                true
            }
            Event::MouseUp { .. } => {
                if self.dragging_volume {
                    self.dragging_volume = false;
                    crate::settings::set("volume", &alloc::format!("{}", VOLUME.load(Ordering::Relaxed)));
                    crate::audio::effect(crate::audio::Effect::Click);
                }
                false
            }
            Event::MouseDown { x, y, button: 0 } => {
                for k in 0..PANES.len() {
                    if inside(x, y, (8, 16 + k as i32 * 34, SIDE_W - 16, 30)) {
                        self.pane = k;
                        return true;
                    }
                }
                match self.pane {
                    0 => {
                        for k in 0..WALLPAPERS.len() {
                            if inside(x, y, swatch_rect(k, content_x, 70, 96, 3)) {
                                WALLPAPER.store(k, Ordering::Relaxed);
                                crate::settings::set("wallpaper", &alloc::format!("{}", k));
                            }
                        }
                        for k in 0..ACCENTS.len() {
                            if inside(x, y, swatch_rect(k, content_x, 330, 34, 6)) {
                                draw::set_accent(ACCENTS[k].1);
                                crate::settings::set("accent", &alloc::format!("{:06X}", ACCENTS[k].1));
                            }
                        }
                    }
                    1 => {
                        if inside(x, y, (w - 80, 60, 50, 30)) {
                            let on = !MAGNIFY.load(Ordering::Relaxed);
                            MAGNIFY.store(on, Ordering::Relaxed);
                            crate::settings::set("magnify", if on { "1" } else { "0" });
                        }
                        if inside(x, y, (w - 80, 120, 50, 30)) {
                            let on = !TILING.load(Ordering::Relaxed);
                            TILING.store(on, Ordering::Relaxed);
                            crate::settings::set("tiling", if on { "1" } else { "0" });
                        }
                    }
                    3 => {
                        if inside(x, y, (w - 80, 150, 50, 30)) {
                            let on = !crate::audio::EFFECTS_ON.load(Ordering::Relaxed);
                            crate::audio::EFFECTS_ON.store(on, Ordering::Relaxed);
                            crate::settings::set("effects", if on { "1" } else { "0" });
                            if on {
                                crate::audio::effect(crate::audio::Effect::Click);
                            }
                        }
                        if inside(x, y, (content_x - 10, 60, w - content_x - 20, 34)) {
                            self.dragging_volume = true;
                            self.set_volume(x, content_x, w);
                        }
                    }
                    _ => {}
                }
                true
            }
            _ => false,
        }
    }

    fn tick(&mut self) -> bool {
        // the About page shows live numbers
        self.pane == 4
    }

    fn draw(&mut self, fb: &mut Framebuffer) {
        let (w, h) = (fb.width() as i32, fb.height() as i32);
        let f = draw::font(16, false);
        let fb_ = draw::font(16, true);
        let big = draw::font(24, true);
        draw::rect(fb, 0, 0, w, h, BG);
        draw::rect(fb, 0, 0, SIDE_W, h, BG_DARK);
        draw::rect(fb, SIDE_W, 0, 1, h, BORDER);
        for (k, name) in PANES.iter().enumerate() {
            let r = (8, 16 + k as i32 * 34, SIDE_W - 16, 30);
            if k == self.pane {
                draw::round(fb, r.0, r.1, r.2, r.3, 7, accent());
                draw::text(fb, &f, r.0 + 12, r.1 + 6, name, 0x14171C);
            } else {
                if inside(self.mouse.0, self.mouse.1, r) {
                    draw::round(fb, r.0, r.1, r.2, r.3, 7, BG_HOVER);
                }
                draw::text(fb, &f, r.0 + 12, r.1 + 6, name, TEXT);
            }
        }
        let x = SIDE_W + 24;
        draw::text(fb, &big, x, 16, PANES[self.pane], TEXT);
        match self.pane {
            0 => {
                label(fb, &fb_, x, 56, "Wallpaper", false);
                for (k, name) in WALLPAPERS.iter().enumerate() {
                    let r = swatch_rect(k, x, 84, 96, 3);
                    let (a, b) = wallpaper_colors(k);
                    draw::gradient_round(fb, r.0, r.1, r.2, r.3 * 2 / 3 + 6, 8, a, b, draw::full(fb));
                    if WALLPAPER.load(Ordering::Relaxed) == k {
                        draw::outline(fb, r.0 - 3, r.1 - 3, r.2 + 6, r.3 * 2 / 3 + 12, accent());
                    }
                    label(fb, &f, r.0, r.1 + r.3 * 2 / 3 + 12, name, true);
                }
                label(fb, &fb_, x, 316, "Accent color", false);
                for (k, (name, color)) in ACCENTS.iter().enumerate() {
                    let r = swatch_rect(k, x, 344, 34, 6);
                    draw::disc(fb, r.0 + 17, r.1 + 17, 16, *color);
                    if accent() == *color {
                        draw::disc(fb, r.0 + 17, r.1 + 17, 5, 0x14171C);
                    }
                    let _ = name;
                }
            }
            1 => {
                row(fb, &f, &fb_, x, 56, w, "Dock magnification", "Icons grow when the pointer is near", MAGNIFY.load(Ordering::Relaxed));
                row(fb, &f, &fb_, x, 116, w, "Automatic tiling", "Windows arrange themselves (Super+T)", TILING.load(Ordering::Relaxed));
                label(fb, &fb_, x, 190, "Keyboard shortcuts", false);
                let lines = [
                    "Super+Space   search and launch",
                    "Super+Enter   terminal",
                    "Super+Q       close window",
                    "Super+Left/Right   snap to half",
                    "Super+Up/Down      maximize / restore",
                    "Super+F       maximize     Super+M  minimize",
                    "Super+1..9    open the n-th dock app",
                    "Alt+Tab       switch window",
                ];
                for (k, l) in lines.iter().enumerate() {
                    draw::text(fb, &draw::font(15, false), x, 220 + k as i32 * 22, l, TEXT_MUTED);
                }
            }
            2 => {
                let (pw, ph, lw, lh) = framebuffer::with_screen(|s| {
                    let (p, _) = (s.physical(), 0);
                    (p.0, p.1, s.width(), s.height())
                })
                .unwrap_or((0, 0, 0, 0));
                row_text(fb, &f, &fb_, x, 56, "Screen", &alloc::format!("{}x{} pixels", pw, ph));
                row_text(fb, &f, &fb_, x, 96, "Desktop resolution", &alloc::format!("{}x{}", lw, lh));
                draw::text(fb, &draw::font(15, false), x, 150, "Change the resolution from the terminal with:  res 1280x720", TEXT_MUTED);
            }
            3 => {
                label(fb, &fb_, x, 56, "Output volume", false);
                let v = VOLUME.load(Ordering::Relaxed) as i32;
                slider(fb, x, 70, w - x - 60, v);
                draw::text(fb, &f, w - 50, 72, &alloc::format!("{}", v), TEXT);
                label(fb, &f, x, 120, "Sound card", true);
                let card = if crate::audio::available() { "Intel AC97 (found)" } else { "none detected" };
                label(fb, &f, x + 140, 120, card, false);
                row(fb, &f, &fb_, x, 150, w, "Interface sounds", "Window and game sound effects", crate::audio::EFFECTS_ON.load(Ordering::Relaxed));
            }
            _ => {
                let up = crate::time::uptime_secs();
                let (hh, mm, ss) = crate::time::split_hms(up);
                let heap = crate::allocator::heap_used() / 1024;
                row_text(fb, &f, &fb_, x, 56, "System", concat!("zero ", env!("CARGO_PKG_VERSION")));
                row_text(fb, &f, &fb_, x, 96, "Uptime", &alloc::format!("{:02}:{:02}:{:02}", hh, mm, ss));
                row_text(fb, &f, &fb_, x, 136, "Memory in use", &alloc::format!("{} KiB of {} MiB", heap, crate::allocator::HEAP_SIZE / (1024 * 1024)));
                row_text(fb, &f, &fb_, x, 176, "Threads", &alloc::format!("{}", crate::sched::count()));
                row_text(fb, &f, &fb_, x, 216, "Processors", &alloc::format!("{}", crate::smp::cpu_count()));
                draw::text(fb, &draw::font(15, false), x, 270, "A small operating system written in Rust.", TEXT_MUTED);
            }
        }
    }
}

impl Settings {
    fn set_volume(&mut self, x: i32, content_x: i32, w: i32) {
        let width = (w - content_x - 60).max(1);
        let v = ((x - content_x) * 100 / width).clamp(0, 100);
        VOLUME.store(v as u32, Ordering::Relaxed);
    }
}

fn row(fb: &mut Framebuffer, f: &crate::graphics::font::Font, fb_: &crate::graphics::font::Font, x: i32, y: i32, w: i32, title: &str, hint: &str, on: bool) {
    draw::round(fb, x - 10, y - 8, w - x - 10, 54, 8, BG_PANEL);
    draw::text(fb, fb_, x + 4, y, title, TEXT);
    draw::text(fb, &draw::font(15, false), x + 4, y + 22, hint, TEXT_FAINT);
    switch(fb, w - 80, y + 6, on);
    let _ = f;
}

fn row_text(fb: &mut Framebuffer, f: &crate::graphics::font::Font, fb_: &crate::graphics::font::Font, x: i32, y: i32, title: &str, value: &str) {
    draw::text(fb, f, x, y, title, TEXT_MUTED);
    draw::text(fb, fb_, x + 200, y, value, TEXT);
}

fn wallpaper_colors(k: usize) -> (u32, u32) {
    match k {
        1 => (0x0F4C5C, 0x1E90FF),
        2 => (0x24432E, 0x3C8D5A),
        3 => (0x2B2F38, 0x5C6370),
        4 => (0xF7768E, 0x7A3E4B),
        _ => (0x4C6EF5, 0x3A2A55),
    }
}

// ---- Activity monitor --------------------------------------------------------

pub struct Monitor {
    history: alloc::vec::Vec<u32>,
    last_switches: u64,
    last_ms: u64,
}

impl Monitor {
    pub fn new() -> Monitor {
        Monitor { history: alloc::vec::Vec::new(), last_switches: 0, last_ms: 0 }
    }
}

impl AppUi for Monitor {
    fn event(&mut self, _ev: Event, _w: i32, _h: i32) -> bool {
        false
    }

    fn tick(&mut self) -> bool {
        let now = crate::time::uptime_ms();
        if now < self.last_ms + 500 {
            return false;
        }
        let sw = crate::sched::switches();
        let rate = ((sw - self.last_switches) * 1000 / (now - self.last_ms).max(1)) as u32;
        self.last_switches = sw;
        self.last_ms = now;
        self.history.push(rate);
        if self.history.len() > 60 {
            self.history.remove(0);
        }
        true
    }

    fn draw(&mut self, fb: &mut Framebuffer) {
        let (w, h) = (fb.width() as i32, fb.height() as i32);
        let f = draw::font(16, false);
        let fb_ = draw::font(16, true);
        draw::rect(fb, 0, 0, w, h, BG);
        draw::text(fb, &draw::font(24, true), 20, 14, "Activity", TEXT);

        // memory bar
        let used = crate::allocator::heap_used();
        let total = crate::allocator::HEAP_SIZE;
        label(fb, &fb_, 20, 56, "Memory", false);
        draw::round(fb, 20, 82, w - 40, 14, 7, 0x3A3F4B);
        draw::round(fb, 20, 82, ((w - 40) as usize * used / total).max(14) as i32, 14, 7, accent());
        label(fb, &f, 20, 102, &alloc::format!("{} KiB used of {} MiB", used / 1024, total / (1024 * 1024)), true);

        // thread switches per second
        label(fb, &fb_, 20, 140, "Context switches per second", false);
        let (gx, gy, gw, gh) = (20, 166, w - 40, 70);
        draw::round(fb, gx, gy, gw, gh, 8, BG_DARK);
        let max = self.history.iter().copied().max().unwrap_or(1).max(1) as i32;
        let n = self.history.len() as i32;
        for (k, v) in self.history.iter().enumerate() {
            let bh = (*v as i32 * (gh - 8) / max).max(1);
            let bx = gx + 4 + k as i32 * (gw - 8) / 60;
            draw::rect(fb, bx, gy + gh - 4 - bh, ((gw - 8) / 60).max(2) - 1, bh, accent());
        }
        let _ = n;

        // the threads
        label(fb, &fb_, 20, 252, "Running programs", false);
        let mut y = 280;
        for (id, name) in crate::sched::names() {
            if y > h - 24 {
                break;
            }
            draw::text(fb, &f, 28, y, &alloc::format!("{:>3}", id), TEXT_FAINT);
            draw::text(fb, &f, 80, y, name, TEXT);
            y += 22;
        }
        let _ = (BG_HOVER, TEXT_MUTED);
        let _: Option<String> = None;
    }
}
