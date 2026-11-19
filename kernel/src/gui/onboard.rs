//! The desktop's first-run setup, in the spirit of a new Mac: an animated
//! "hello", then a few friendly steps (your name, light or dark, a wallpaper,
//! a quick tour), all over a warp-speed starfield, with Bit along for the ride.
//! Also the lock screen.

use super::gfx::{self, ease_back, ease_out, Canvas};
use super::icons::{self, Icon};
use super::theme;
use super::ui::{self, id, idn, Input, Ui, UiState};
use crate::graphics::buddy;
use crate::graphics::framebuffer::{self, Framebuffer};
use crate::input::Key;
use alloc::string::String;
use alloc::vec::Vec;

struct Stars {
    s: Vec<(i32, i32, i32, i32)>, // x, y in -1000..1000, z 1..1000, previous z
    rng: u32,
}

impl Stars {
    fn new(n: usize) -> Stars {
        let mut st = Stars { s: Vec::with_capacity(n), rng: 0x9E37_79B9 ^ crate::time::uptime_ms() as u32 | 1 };
        for _ in 0..n {
            let (x, y, z) = (st.r(2000) - 1000, st.r(2000) - 1000, st.r(1000) + 1);
            st.s.push((x, y, z, z));
        }
        st
    }

    fn r(&mut self, n: u32) -> i32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng % n.max(1)) as i32
    }

    /// Moves the stars towards the viewer by `speed` (depth units per frame).
    fn step(&mut self, speed: i32) {
        for i in 0..self.s.len() {
            let (x, y, z, _) = self.s[i];
            let nz = z - speed;
            if nz <= 8 {
                let (x, y) = (self.r(2000) - 1000, self.r(2000) - 1000);
                self.s[i] = (x, y, 1000, 1000);
            } else {
                self.s[i] = (x, y, nz, z);
            }
        }
    }

    fn draw(&self, cv: &mut Canvas, w: i32, h: i32, tint: u32) {
        let (cx, cy) = (w / 2, h / 2);
        let focal = w.max(h) / 7;
        for &(x, y, z, pz) in &self.s {
            let p = |zz: i32| ((cx + x * focal / zz.max(1)) * 16, (cy + y * focal / zz.max(1)) * 16);
            let (x1, y1) = p(z);
            let (x0, y0) = p(pz + (pz - z) * 2);
            let bright = ((1000 - z) * 255 / 1000).clamp(20, 255) as u32;
            let c = gfx::mix(0x000000, gfx::mix(0xFFFFFF, tint, 60), bright);
            if x1 / 16 < -10 || x1 / 16 > w + 10 || y1 / 16 < -10 || y1 / 16 > h + 10 {
                continue;
            }
            cv.line16(x0, y0, x1, y1, c, 255);
            if z < 200 {
                cv.circle(x1 / 16, y1 / 16, 1, c);
            }
        }
    }
}

/// Reads the mouse and keyboard into the toolkit's input (the setup owns the screen).
struct Pointer {
    pos: (i32, i32),
    buttons: u8,
    last_click: (u64, i32, i32),
}

impl Pointer {
    fn poll(&mut self, w: i32, h: i32) -> Input {
        let mut inp = Input { mouse: self.pos, inside: true, focused: true, ..Default::default() };
        inp.down = self.buttons & 1 != 0;
        if let Some((dx, dy, b)) = crate::mouse::take() {
            let accel = |d: i32| d + d.signum() * (d * d / 24).min(d.abs() * 2);
            self.pos = ((self.pos.0 + accel(dx)).clamp(0, w - 1), (self.pos.1 - accel(dy)).clamp(0, h - 1));
            inp.mouse = self.pos;
            let prev = self.buttons;
            self.buttons = b;
            inp.down = b & 1 != 0;
            if b & 1 != 0 && prev & 1 == 0 {
                inp.pressed = true;
                let now = crate::time::uptime_ms();
                inp.clicks = if now < self.last_click.0 + 420 { 2 } else { 1 };
                self.last_click = (now, self.pos.0, self.pos.1);
            }
            if b & 1 == 0 && prev & 1 != 0 {
                inp.released = true;
            }
            if b & 2 != 0 && prev & 2 == 0 {
                inp.rpressed = true;
            }
        }
        while crate::input::pop_raw_key().is_some() {}
        while let Some(k) = crate::input::pop_gui_key() {
            inp.keys.push(k);
        }
        inp.wheel = crate::mouse::take_wheel();
        inp
    }
}

fn present(buf: &[u32], w: i32, h: i32) {
    framebuffer::with_screen(|fb| {
        fb.put32_rect(buf, 0, 0, w as usize, h as usize);
        fb.flush();
    });
}

fn draw_bit(cv: &mut Canvas, x: i32, floor: i32, scale: usize) {
    let (cw, ch) = buddy::SPRITE;
    let w = (cw * scale) as i32 + 4;
    let h = (ch * scale) as i32 + 40;
    let key = 0xFF00FF;
    let Some(mut fb) = framebuffer::with_screen(|s| Framebuffer::offscreen32(s, w as usize, h as usize)) else { return };
    fb.pixels32().fill(key);
    buddy::draw(&mut fb, 0, w as usize, h as usize, scale);
    let px = fb.pixels32_ref();
    cv.round_a(x + 4, floor - 3, w - 8, 5, 2, 0x000000, 80);
    for yy in 0..h {
        for xx in 0..w {
            let c = px[(yy * w + xx) as usize];
            if c != key {
                cv.put(x + xx, floor - h + yy, c, 255);
            }
        }
    }
}

fn bubble(cv: &mut Canvas, x: i32, bottom: i32, text: &str, alpha: u32) {
    let f = gfx::font(16, false);
    let lines = gfx::wrap(&f, text, 300);
    let bw = lines.iter().map(|l| gfx::text_width(&f, l)).max().unwrap_or(0) + 28;
    let bh = lines.len() as i32 * 22 + 18;
    let by = bottom - bh;
    let old = cv.opacity;
    cv.opacity = alpha;
    cv.shadow(x, by + 4, bw, bh, 14, 14, 90);
    cv.round(x, by, bw, bh, 14, 0xFFFFFF);
    for i in 0..9 {
        cv.rect(x + 18 + i, by + bh - 1 + i, 18 - 2 * i, 1, 0xFFFFFF);
    }
    for (k, l) in lines.iter().enumerate() {
        cv.text(x + 14, by + 9 + k as i32 * 22, l, &f, 0x1D1D1F);
    }
    cv.opacity = old;
}

const HELLOS: [&str; 8] = ["hello", "hola", "bonjour", "hallo", "ciao", "namaste", "ola", "salut"];

/// Runs the setup. Returns when the user finishes it.
pub fn run(w: i32, h: i32) {
    let mut buf = alloc::vec![0u32; (w * h) as usize];
    let mut stars = Stars::new(420);
    let mut ptr = Pointer { pos: (w / 2, h / 2), buttons: 0, last_click: (0, 0, 0) };
    let mut st = UiState::default();
    let mut step = 0usize;
    let mut step_at = crate::time::uptime_ms();
    // always start empty: an old saved name must not be typed into
    let mut name = String::new();
    let mut wall = 0usize;
    let mut speed: i32 = 6;
    buddy::set_context(buddy::Context::Onboarding);
    let started = crate::time::uptime_ms();
    loop {
        let now = crate::time::uptime_ms();
        let inp = ptr.poll(w, h);
        let mut cv = Canvas::new(&mut buf, w as usize, h as usize);
        // space: a deep gradient, then the stars
        for y in 0..h {
            let c = gfx::mix(0x05040F, 0x1B1040, (y * 255 / h) as u32);
            cv.hline(0, y, w, c, 255);
        }
        let target = match step {
            0 => 6,
            7 => 60,
            _ => 10,
        };
        speed += (target - speed).signum() * 1;
        stars.step(speed);
        stars.draw(&mut cv, w, h, theme::accent());
        let age = now.saturating_sub(step_at) as i32;
        let fade = ease_out((age * 1000 / 500).min(1000));
        let mut ui = Ui::new(cv, inp.clone(), &mut st);
        let any_input = !inp.keys.is_empty() || inp.pressed;
        let mut next = false;
        let card_w = 560.min(w - 40);
        let card_h = 380.min(h - 160);
        let (cx, cy) = ((w - card_w) / 2, (h - card_h) / 2 - 30);
        let slide = 40 - 40 * ease_back((age * 1000 / 600).min(1000)) / 1000;
        let speech: &str;
        match step {
            0 => {
                // "hello" in several languages, each fading in and out
                let per = 1600;
                let k = (age / per) as usize % HELLOS.len();
                let t = age % per;
                let a = if t < 400 { t * 255 / 400 } else if t > per - 400 { (per - t) * 255 / 400 } else { 255 };
                let f = gfx::font(32, true);
                let word = HELLOS[k];
                // drawn large by repeating the 32px font at 3x through a small buffer
                let tw = gfx::text_width(&f, word);
                let (sw_, sh_) = ((tw + 4) as usize, 40usize);
                let mut small = alloc::vec![0u32; sw_ * sh_];
                {
                    let mut sc = Canvas::new(&mut small, sw_, sh_);
                    sc.text(2, 2, word, &f, 0xFFFFFF);
                }
                // the word as an alpha mask in white, scaled up smoothly
                let mask: Vec<u32> = small.iter().map(|&p| ((p & 0xFF) << 24) | 0xFFFFFF).collect();
                let scale = 3;
                let (dw, dh) = (sw_ as i32 * scale, sh_ as i32 * scale);
                let old = ui.cv.opacity;
                ui.cv.opacity = a as u32;
                ui.cv.blit_scaled(((w - dw) / 2, h / 2 - dh / 2 - 40, dw, dh), &mask, sw_, sh_, true, true);
                ui.cv.opacity = old;
                if age > 1200 {
                    let blink = ((now / 600) % 2 == 0) as u32 * 120 + 100;
                    let f2 = gfx::font(16, false);
                    ui.cv.text_center(w / 2, h / 2 + 60, "click or press any key to begin", &f2, gfx::mix(0x000000, 0xFFFFFF, blink));
                }
                speech = "...";
                if any_input && age > 600 {
                    next = true;
                }
            }
            1..=6 => {
                // a frosted card in the middle
                let old = ui.cv.opacity;
                ui.cv.opacity = (fade * 255 / 1000) as u32;
                let y = cy + slide;
                ui.cv.shadow(cx, y + 10, card_w, card_h, 22, 40, 160);
                ui.cv.blur(cx, y, card_w, card_h, 10);
                ui.cv.round_a(cx, y, card_w, card_h, 22, 0x1C1B2A, 200);
                ui.cv.round_outline(cx, y, card_w, card_h, 22, 0xFFFFFF, 50);
                let title_f = gfx::font(32, true);
                let body_f = gfx::font(16, false);
                // step dots
                for d in 1..=6 {
                    let col = if d == step { 0xFFFFFF } else { 0x6A6880 };
                    ui.cv.circle(w / 2 - 50 + (d as i32 - 1) * 20, y + card_h - 22, 4, col);
                }
                match step {
                    1 => {
                        icons::zero_logo(&mut ui.cv, w / 2, y + 70, 40);
                        ui.cv.text_center(w / 2, y + 128, "Welcome to zero", &title_f, 0xFFFFFF);
                        let lines = gfx::wrap(&body_f, "A tiny operating system with a desktop, apps, games, a web browser and a buddy named Bit. Let's set it up together.", card_w - 80);
                        for (k, l) in lines.iter().enumerate() {
                            ui.cv.text_center(w / 2, y + 180 + k as i32 * 24, l, &body_f, 0xD8D6E8);
                        }
                        speech = "hi! i'm Bit. i'll come along while we set things up!";
                    }
                    2 => {
                        ui.cv.text_center(w / 2, y + 40, "What's your name?", &title_f, 0xFFFFFF);
                        ui.cv.text_center(w / 2, y + 86, "Bit would like to know what to call you.", &body_f, 0xD8D6E8);
                        ui.st.focus = id("name");
                        if ui.field(id("name"), (w / 2 - 160, y + 140, 320, 40), &mut name, "Your name") && !name.trim().is_empty() {
                            next = true;
                        }
                        speech = if name.trim().is_empty() { "type your name, then press enter." } else { "what a great name!" };
                    }
                    3 => {
                        ui.cv.text_center(w / 2, y + 40, "Choose a look", &title_f, 0xFFFFFF);
                        let dark = theme::is_dark();
                        for (k, (label, d)) in [("Light", false), ("Dark", true)].iter().enumerate() {
                            let r = (w / 2 - 175 + k as i32 * 190, y + 96, 160, 110);
                            if ui.interact(idn("mode", k), r).clicked {
                                theme::set_dark(*d);
                            }
                            let (bg, bar, win) = if *d { (0x3A2E5E, 0x1C1D21, 0x2B2C31) } else { (0xB9C6F5, 0xE9E9EC, 0xFFFFFF) };
                            ui.cv.round(r.0, r.1, r.2, r.3, 12, bg);
                            ui.cv.rect(r.0 + 8, r.1 + 6, r.2 - 16, 8, bar);
                            ui.cv.round(r.0 + 24, r.1 + 30, r.2 - 48, r.3 - 44, 7, win);
                            ui.cv.round(r.0 + 24, r.1 + 30, r.2 - 48, 12, 6, bar);
                            if dark == *d {
                                ui.cv.round_outline(r.0 - 4, r.1 - 4, r.2 + 8, r.3 + 8, 15, theme::accent(), 255);
                                ui.cv.round_outline(r.0 - 3, r.1 - 3, r.2 + 6, r.3 + 6, 14, theme::accent(), 255);
                            }
                            ui.cv.text_center(r.0 + r.2 / 2, r.1 + r.3 + 10, label, &body_f, 0xFFFFFF);
                        }
                        let n = theme::ACCENTS.len() as i32;
                        for (k, (_, c)) in theme::ACCENTS.iter().enumerate() {
                            let (ax, ay) = (w / 2 - n * 14 + k as i32 * 28 + 14, y + 262);
                            if ui.interact(idn("acc", k), (ax - 11, ay - 11, 22, 22)).clicked {
                                theme::set_accent(*c);
                            }
                            ui.cv.circle(ax, ay, 10, *c);
                            if theme::accent() == *c {
                                ui.cv.circle(ax, ay, 4, 0xFFFFFF);
                            }
                        }
                        speech = if theme::is_dark() { "dark mode. very mysterious." } else { "light mode. bright and sunny!" };
                    }
                    4 => {
                        ui.cv.text_center(w / 2, y + 40, "Pick a wallpaper", &title_f, 0xFFFFFF);
                        let colors = [(0x4C6EF5, 0xC056A0), (0x1E90FF, 0x00D1B2), (0x3FA65D, 0x1F4A2E), (0x50555F, 0x2C2F37), (0xFFB45E, 0xC25B5B), (0x23D5AB, 0x0B1E33)];
                        for k in 0..6 {
                            let r = (w / 2 - 225 + (k as i32 % 3) * 155, y + 96 + (k as i32 / 3) * 104, 140, 86);
                            if ui.interact(idn("wall", k), r).clicked {
                                wall = k;
                            }
                            ui.cv.gradient(r.0, r.1, r.2, r.3, 12, colors[k].0, colors[k].1);
                            if wall == k {
                                ui.cv.round_outline(r.0 - 4, r.1 - 4, r.2 + 8, r.3 + 8, 15, 0xFFFFFF, 255);
                                ui.cv.round_outline(r.0 - 3, r.1 - 3, r.2 + 6, r.3 + 6, 14, 0xFFFFFF, 255);
                            }
                        }
                        speech = "ooh, they're all so pretty.";
                    }
                    5 => {
                        ui.cv.text_center(w / 2, y + 34, "A quick tour", &title_f, 0xFFFFFF);
                        let tips: [(Icon, &str, &str); 4] = [
                            (Icon::Launchpad, "Dock and Launchpad", "Your apps live in the dock. F4 shows all of them."),
                            (Icon::Activity, "Spotlight", "Super+Space finds apps, files, answers sums and asks Bit."),
                            (Icon::Files, "Windows", "Drag to the edges to snap. F3 shows every window at once."),
                            (Icon::Buddy, "Bit", "That's me! Pet me with a click, double-click to chat."),
                        ];
                        for (k, (icon, t, d)) in tips.iter().enumerate() {
                            let appear = ease_back(((age - k as i32 * 180) * 1000 / 500).clamp(0, 1000));
                            let ry = y + 86 + k as i32 * 62;
                            let rx = cx + 40 + (1000 - appear) * 60 / 1000;
                            let old2 = ui.cv.opacity;
                            ui.cv.opacity = (appear.clamp(0, 1000) * old2 as i32 / 1000) as u32;
                            icons::draw(&mut ui.cv, *icon, rx, ry, 44);
                            let bf = gfx::font(16, true);
                            ui.cv.text(rx + 60, ry + 2, t, &bf, 0xFFFFFF);
                            ui.cv.text(rx + 60, ry + 22, d, &body_f, 0xD8D6E8);
                            ui.cv.opacity = old2;
                        }
                        ui.animate(1000);
                        speech = "right-click almost anything for more options!";
                    }
                    _ => {
                        let n = name.trim();
                        ui.cv.text_center(w / 2, y + 60, "You're all set", &title_f, 0xFFFFFF);
                        let line = alloc::format!("Enjoy zero, {}!", if n.is_empty() { "friend" } else { n });
                        ui.cv.text_center(w / 2, y + 110, &line, &body_f, 0xD8D6E8);
                        // a spinning ring while we "set up"
                        let a = (age as i32 / 3) % 360;
                        for k in 0..12 {
                            let ang = a + k * 30;
                            let (px, py) = (w / 2 + icons::cos_deg(ang) * 34 / 1000, y + 200 + icons::sin_deg(ang) * 34 / 1000);
                            ui.cv.circle_a(px, py, 5, 0xFFFFFF, (40 + k * 18) as u32);
                        }
                        speech = "let's go!";
                    }
                }
                // buttons
                let label = match step {
                    1 => "Get Started",
                    6 => "Start Using zero",
                    _ => "Continue",
                };
                let br = (w / 2 - 100, y + card_h - 76, 200, 40);
                let enter = inp.keys.iter().any(|(k, _)| *k == Key::Char('\n')) && step != 2;
                if ui.primary(br, label) || enter {
                    if step != 2 || !name.trim().is_empty() {
                        next = true;
                    }
                }
                if step > 1 && ui.button((cx + 20, y + card_h - 70, 80, 30), "Back") {
                    step -= 1;
                    step_at = now;
                }
                ui.cv.opacity = old;
            }
            _ => {
                // warp out: everything rushes past and fades to the desktop
                speech = "whee!";
                let t = (age * 1000 / 900).min(1000);
                ui.cv.rect_a(0, 0, w, h, 0xFFFFFF, (t * t / 1000 * 255 / 1000) as u32);
                if age > 900 {
                    break;
                }
            }
        }
        // Bit walks along the bottom the whole time
        let since = now.saturating_sub(started) as i32;
        let bx = 40 + ((since / 20) % (w - 200).max(1)).min(w - 200);
        let floor = h - 40;
        ui.cv.hline(0, floor, w, 0xFFFFFF, 30);
        draw_bit(&mut ui.cv, bx, floor, 3);
        if step > 0 && step < 7 {
            bubble(&mut ui.cv, bx + 40, floor - 110, speech, (fade * 255 / 1000) as u32);
        }
        // the pointer
        ui.finish();
        let p = ui.inp.mouse;
        draw_pointer(&mut ui.cv, p);
        drop(ui);
        present(&buf, w, h);
        if next {
            match step {
                2 => {
                    let n = name.trim();
                    if !n.is_empty() {
                        crate::settings::set("name", n);
                        buddy::set_name(n);
                    }
                }
                4 => {
                    super::desktop::WALLPAPER.store(wall, core::sync::atomic::Ordering::Relaxed);
                    crate::settings::set("wallpaper", &alloc::format!("{}", wall));
                }
                _ => {}
            }
            step += 1;
            step_at = now;
            crate::audio::effect(crate::audio::Effect::Click);
        }
        crate::sched::yield_now();
        // about 60 frames a second
        while crate::time::uptime_ms() < now + 16 {
            crate::sched::yield_idle();
        }
    }
    theme::save();
    crate::settings::set("desk_setup", "1");
    crate::settings::set("onboarded", "1");
    buddy::set_context(buddy::Context::Shell);
    crate::audio::effect(crate::audio::Effect::Win);
}

fn draw_pointer(cv: &mut Canvas, p: (i32, i32)) {
    super::desktop::draw_cursor(cv, p, super::Cursor::Arrow);
}

pub fn request_rerun() {
    crate::settings::set("desk_setup", "");
}

/// The lock screen: the time over the blurred desktop, until a click or a key.
pub fn lock_screen(w: i32, h: i32) {
    let mut back = alloc::vec![0u32; (w * h) as usize];
    // take what is on screen as the background
    let shot = framebuffer::with_screen(|fb| {
        use crate::graphics::display::Display;
        let _ = fb.width();
    });
    let _ = shot;
    {
        let mut cv = Canvas::new(&mut back, w as usize, h as usize);
        for y in 0..h {
            cv.hline(0, y, w, gfx::mix(0x101030, 0x402050, (y * 255 / h) as u32), 255);
        }
    }
    let mut ptr = Pointer { pos: (w / 2, h / 2), buttons: 0, last_click: (0, 0, 0) };
    let name = crate::settings::get_or("name", "friend");
    let mut st = UiState::default();
    let start = crate::time::uptime_ms();
    loop {
        let now = crate::time::uptime_ms();
        let inp = ptr.poll(w, h);
        let mut buf = back.clone();
        {
            let cv = Canvas::new(&mut buf, w as usize, h as usize);
            let mut ui = Ui::new(cv, inp.clone(), &mut st);
            let (hh, mm, _, d, mo, _) = super::desktop::read_rtc();
            let months = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
            let date = alloc::format!("{} {}", months.get((mo as usize).wrapping_sub(1)).copied().unwrap_or(""), d);
            let f = gfx::font(24, false);
            ui.cv.text_center(w / 2, 90, &date, &f, 0xFFFFFF);
            // the time, large
            let t = alloc::format!("{:02}:{:02}", hh, mm);
            let tf = gfx::font(32, true);
            let tw = gfx::text_width(&tf, &t);
            let (sw_, sh_) = ((tw + 4) as usize, 40usize);
            let mut small = alloc::vec![0u32; sw_ * sh_];
            {
                let mut sc = Canvas::new(&mut small, sw_, sh_);
                sc.text(2, 2, &t, &tf, 0xFFFFFF);
            }
            let mask: Vec<u32> = small.iter().map(|&p| ((p & 0xFF) << 24) | 0xFFFFFF).collect();
            ui.cv.blit_scaled(((w - sw_ as i32 * 3) / 2, 120, sw_ as i32 * 3, sh_ as i32 * 3), &mask, sw_, sh_, true, true);
            let initial: String = name.chars().next().map(|c| c.to_ascii_uppercase()).into_iter().collect();
            ui.cv.gradient(w / 2 - 36, h - 220, 72, 72, 36, 0x8E8EFF, 0x5E5CE6);
            ui.cv.text_center(w / 2, h - 202, &initial, &gfx::font(32, true), 0xFFFFFF);
            ui.cv.text_center(w / 2, h - 136, &name, &gfx::font(20, true), 0xFFFFFF);
            let pulse = (now / 8 % 200) as i32;
            let a = if pulse < 100 { pulse } else { 200 - pulse };
            ui.cv.text_center(w / 2, h - 100, "Click or press any key to unlock", &gfx::font(16, false), gfx::mix(0x8080A0, 0xFFFFFF, (a * 2) as u32));
            draw_pointer(&mut ui.cv, inp.mouse);
        }
        present(&buf, w, h);
        if (inp.pressed || !inp.keys.is_empty()) && now > start + 400 {
            break;
        }
        let _ = ui::inside;
        crate::sched::yield_idle();
    }
}
