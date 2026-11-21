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

    /// Draws the stars as plain one pixel streaks written straight into the picture
    /// (no blending: this runs for hundreds of stars every frame).
    fn draw(&self, cv: &mut Canvas, dirty: &mut Dirty, w: i32, h: i32, tint: u32) {
        let (cx, cy) = (w / 2, h / 2);
        let focal = w.max(h) / 7;
        let stride = cv.stride;
        let near = gfx::mix(0xFFFFFF, tint, 60);
        let px = &mut *cv.px;
        for &(x, y, z, pz) in &self.s {
            let (x1, y1) = (cx + x * focal / z.max(1), cy + y * focal / z.max(1));
            if x1 < 0 || x1 >= w || y1 < 0 || y1 >= h {
                continue;
            }
            let bright = ((1000 - z) * 255 / 1000).clamp(30, 255) as u32;
            let c = gfx::mix(0x05040F, near, bright);
            let back = pz + (pz - z) * 2;
            let (x0, y0) = (cx + x * focal / back.max(1), cy + y * focal / back.max(1));
            // a DDA line: step along the longer axis
            let (dx, dy) = (x1 - x0, y1 - y0);
            let steps = dx.abs().max(dy.abs()).clamp(1, 64);
            for i in 0..=steps {
                let (lx, ly) = (x0 + dx * i / steps, y0 + dy * i / steps);
                if lx >= 0 && lx < w && ly >= 0 && ly < h {
                    // the streak fades towards its tail
                    let f = gfx::mix(0x05040F, c, (i * 255 / steps) as u32);
                    px[ly as usize * stride + lx as usize] = f;
                    dirty.mark_px(lx, ly);
                }
            }
            if z < 220 && x1 + 1 < w && y1 + 1 < h {
                let i = y1 as usize * stride + x1 as usize;
                px[i + 1] = c;
                px[i + stride] = c;
                px[i + stride + 1] = c;
                dirty.mark_px(x1, y1);
                dirty.mark_px(x1 + 1, y1 + 1);
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
            let (sx, sy) = crate::mouse::scale(dx, dy);
            self.pos = ((self.pos.0 + sx).clamp(0, w - 1), (self.pos.1 - sy).clamp(0, h - 1));
            inp.mouse = self.pos;
            let prev = self.buttons;
            let (pe, re) = crate::mouse::last_edges();
            self.buttons = b;
            inp.down = b & 1 != 0;
            if (b | pe) & 1 != 0 && prev & 1 == 0 {
                inp.pressed = true;
                let now = crate::time::uptime_ms();
                inp.clicks = if now < self.last_click.0 + 420 { 2 } else { 1 };
                self.last_click = (now, self.pos.0, self.pos.1);
            }
            if (prev & 1 != 0 || (b | pe) & 1 != 0 && prev & 1 == 0) && (b & 1 == 0 || re & 1 != 0) && !(b & 1 != 0 && prev & 1 == 0) {
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

/// Which 64-pixel pieces of the screen changed: one bit per piece, one word per row.
struct Dirty {
    rows: Vec<u64>,
    w: usize,
    h: usize,
}

impl Dirty {
    fn new(w: i32, h: i32, all: bool) -> Dirty {
        let (w, h) = (w as usize, h as usize);
        let mut d = Dirty { rows: alloc::vec![0; h], w, h };
        if all {
            d.mark_all();
        }
        d
    }

    fn clear(&mut self) {
        self.rows.iter_mut().for_each(|r| *r = 0);
    }

    fn mark_all(&mut self) {
        let chunks = (self.w + 63) / 64;
        let all = if chunks >= 64 { u64::MAX } else { (1u64 << chunks) - 1 };
        self.rows.iter_mut().for_each(|r| *r = all);
    }

    #[inline]
    fn mark_px(&mut self, x: i32, y: i32) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.rows[y as usize] |= 1u64 << ((x as usize) >> 6);
        }
    }

    fn mark_rect(&mut self, x: i32, y: i32, rw: i32, rh: i32) {
        let (x0, x1) = (x.max(0) as usize, ((x + rw).max(0) as usize).min(self.w));
        let (y0, y1) = (y.max(0) as usize, ((y + rh).max(0) as usize).min(self.h));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let (c0, c1) = (x0 >> 6, (x1 - 1) >> 6);
        let mut mask = 0u64;
        for c in c0..=c1 {
            mask |= 1u64 << c;
        }
        for y in y0..y1 {
            self.rows[y] |= mask;
        }
    }
}

/// Puts the background back where the last frame drew something.
fn restore(buf: &mut [u32], space: &[u32], prev: &Dirty) {
    let w = prev.w;
    for y in 0..prev.h {
        let mut m = prev.rows[y];
        while m != 0 {
            let c = m.trailing_zeros() as usize;
            m &= m - 1;
            let (x0, x1) = (c * 64, (c * 64 + 64).min(w));
            let o = y * w;
            buf[o + x0..o + x1].copy_from_slice(&space[o + x0..o + x1]);
        }
    }
}

/// Shows the pieces that changed this frame or the last one (the old ones were restored).
fn present(buf: &[u32], cur: &Dirty, prev: &Dirty) {
    let w = cur.w;
    framebuffer::with_screen(|fb| {
        for y in 0..cur.h {
            let mut m = cur.rows[y] | prev.rows[y];
            while m != 0 {
                // a run of neighbouring pieces goes out in one piece
                let c0 = m.trailing_zeros() as usize;
                let run = (m >> c0).trailing_ones() as usize;
                let run_mask: u64 = if run >= 64 { u64::MAX } else { ((1u64 << run) - 1) << c0 };
                m &= !run_mask;
                let (x0, x1) = (c0 * 64, ((c0 + run) * 64).min(w));
                fb.present32(buf, x0, y, x1, y + 1);
            }
        }
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
    let mut space = alloc::vec![0u32; (w * h) as usize];
    // pieces of the screen drawn on in the last frame / this frame (the first frame sends everything)
    let mut prev_dirty = Dirty::new(w, h, true);
    let mut cur_dirty = Dirty::new(w, h, false);
    {
        let mut cv = Canvas::new(&mut space, w as usize, h as usize);
        for y in 0..h {
            let c = gfx::mix(0x05040F, 0x1B1040, (y * 255 / h) as u32);
            cv.hline(0, y, w, c, 255);
        }
        // the line Bit walks on is part of the background
        cv.hline(0, h - 40, w, 0xFFFFFF, 30);
    }
    let mut stars = Stars::new(360);
    let (mut frames, mut fps_at) = (0u32, crate::time::uptime_ms());
    let mut ptr = Pointer { pos: (w / 2, h / 2), buttons: 0, last_click: (0, 0, 0) };
    let mut st = UiState::default();
    let mut step = 0usize;
    let mut step_at = crate::time::uptime_ms();
    // always start empty: an old saved name must not be typed into
    let mut name = String::new();
    let mut wall = 0usize;
    let mut user = String::new();
    let mut pass = String::new();
    let mut last_step = 99usize;
    let mut speed: i32 = 6;
    // the card (shadow, frosted fill, outline) over the background, drawn once and then just copied
    let mut card_cache: Option<Vec<u32>> = None;
    buddy::set_context(buddy::Context::Onboarding);
    let started = crate::time::uptime_ms();
    loop {
        let now = crate::time::uptime_ms();
        let inp = ptr.poll(w, h);
        // space: the gradient was painted once; copying it is just a memcpy
        restore(&mut buf, &space, &prev_dirty);
        cur_dirty.clear();
        let mut cv = Canvas::new(&mut buf, w as usize, h as usize);
        let target = match step {
            0 => 6,
            7 => 60,
            _ => 10,
        };
        speed += (target - speed).signum() * 1;
        stars.step(speed);
        stars.draw(&mut cv, &mut cur_dirty, w, h, theme::accent());
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
                let word = HELLOS[k];
                let old = ui.cv.opacity;
                ui.cv.opacity = a as u32;
                ui.cv.text_big(w / 2, h / 2 - 140, word, 4, 0xFFFFFF);
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
                let (rx, ry, rw, rh) = (cx - 70, cy - 50, card_w + 140, card_h + 190);
                let cacheable = rx >= 0 && ry >= 0 && rx + rw <= w && ry + rh <= h;
                if cacheable && (step > 1 || age >= 600) {
                    if card_cache.is_none() {
                        let mut c = alloc::vec![0u32; (rw * rh) as usize];
                        for r in 0..rh {
                            let src = ((ry + r) * w + rx) as usize;
                            c[(r * rw) as usize..((r + 1) * rw) as usize].copy_from_slice(&space[src..src + rw as usize]);
                        }
                        {
                            let mut cc = Canvas::new(&mut c, rw as usize, rh as usize);
                            cc.shadow(70, 50 + 10, card_w, card_h, 22, 40, 160);
                            cc.round_a(70, 50, card_w, card_h, 22, 0x1C1B2A, 228);
                            cc.round_outline(70, 50, card_w, card_h, 22, 0xFFFFFF, 50);
                        }
                        card_cache = Some(c);
                    }
                    if let Some(c) = &card_cache {
                        for r in 0..rh {
                            let dst = ((ry + r) * w + rx) as usize;
                            ui.cv.px[dst..dst + rw as usize].copy_from_slice(&c[(r * rw) as usize..((r + 1) * rw) as usize]);
                        }
                    }
                } else {
                    ui.cv.shadow(cx, y + 10, card_w, card_h, 22, 40, 160);
                    ui.cv.round_a(cx, y, card_w, card_h, 22, 0x1C1B2A, 228);
                    ui.cv.round_outline(cx, y, card_w, card_h, 22, 0xFFFFFF, 50);
                }
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
                        ui.cv.text_center(w / 2, y + 34, "Create your account", &title_f, 0xFFFFFF);
                        ui.cv.text_center(w / 2, y + 74, "Bit would like to know what to call you.", &body_f, 0xD8D6E8);
                        let ids = [id("name"), id("user"), id("pass")];
                        if last_step != 2 {
                            ui.st.focus = ids[0];
                        }
                        if ui.key(Key::Char('\t')) {
                            let at = ids.iter().position(|&i| i == ui.st.focus).unwrap_or(2);
                            ui.st.focus = ids[(at + 1) % 3];
                        }
                        let fx = w / 2 - 160;
                        // Enter moves to the next field; the focus changes only after all three were drawn,
                        // so one Enter cannot run through every field
                        let mut to = None;
                        if ui.field(ids[0], (fx, y + 104, 320, 38), &mut name, "Your name") {
                            to = Some(ids[1]);
                        }
                        if ui.field(ids[1], (fx, y + 154, 320, 38), &mut user, "Username (optional)") {
                            to = Some(ids[2]);
                        }
                        if ui.field_ex(ids[2], (fx, y + 204, 320, 38), &mut pass, "Password (optional)", true) && !name.trim().is_empty() {
                            next = true;
                        }
                        if let Some(f) = to {
                            ui.st.focus = f;
                        }
                        ui.cv.text_center(w / 2, y + 254, "Leave the password empty for none. Change it later in Settings.", &gfx::font(13, false), 0x9A98B0);
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
                last_step = step;
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
        draw_bit(&mut ui.cv, bx, floor, 3);
        if step > 0 && step < 7 {
            bubble(&mut ui.cv, bx + 40, floor - 110, speech, (fade * 255 / 1000) as u32);
        }
        // the pointer
        ui.finish();
        let p = ui.inp.mouse;
        draw_pointer(&mut ui.cv, p);
        drop(ui);
        // where the UI drew this frame (generous: pieces that did not really change cost little)
        match step {
            0 => cur_dirty.mark_rect(w / 2 - 420, h / 2 - 180, 840, 280),
            1..=6 => cur_dirty.mark_rect(cx - 70, cy - 50, card_w + 140, card_h + 190),
            _ => cur_dirty.mark_all(),
        }
        cur_dirty.mark_rect(bx - 30, floor - 210, 470, 250);
        cur_dirty.mark_rect(p.0 - 16, p.1 - 16, 48, 52);
        present(&buf, &cur_dirty, &prev_dirty);
        core::mem::swap(&mut prev_dirty, &mut cur_dirty);
        frames += 1;
        if now >= fps_at + 1000 {
            crate::serial_println!("[setup] {} fps", frames);
            frames = 0;
            fps_at = now;
        }
        if next {
            match step {
                2 => {
                    let n = name.trim();
                    if !n.is_empty() {
                        crate::settings::set("name", n);
                        buddy::set_name(n);
                        let u: String = if user.trim().is_empty() {
                            n.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()
                        } else {
                            user.trim().into()
                        };
                        crate::settings::set("user", if u.is_empty() { "user" } else { &u });
                        crate::settings::set_password(&pass);
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

/// The lock screen: the time over a dark gradient. With no password a click or key opens it;
/// otherwise the password has to be typed.
pub fn lock_screen(w: i32, h: i32) {
    let mut back = alloc::vec![0u32; (w * h) as usize];
    {
        let mut cv = Canvas::new(&mut back, w as usize, h as usize);
        for y in 0..h {
            cv.hline(0, y, w, gfx::mix(0x101030, 0x402050, (y * 255 / h) as u32), 255);
        }
    }
    let mut ptr = Pointer { pos: (w / 2, h / 2), buttons: 0, last_click: (0, 0, 0) };
    let name = crate::settings::get_or("name", "friend");
    let need_pw = crate::settings::has_password();
    let mut st = UiState::default();
    let mut typed = String::new();
    let mut wrong_at: u64 = 0;
    let start = crate::time::uptime_ms();
    let df = gfx::font(24, false);
    let months = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    loop {
        let now = crate::time::uptime_ms();
        let inp = ptr.poll(w, h);
        let mut buf = back.clone();
        let mut unlock = false;
        {
            let cv = Canvas::new(&mut buf, w as usize, h as usize);
            let mut ui = Ui::new(cv, inp.clone(), &mut st);
            let (hh, mm, _, d, mo, _) = super::desktop::read_rtc();
            let date = alloc::format!("{} {}", months.get((mo as usize).wrapping_sub(1)).copied().unwrap_or(""), d);
            ui.cv.text_center(w / 2, 80, &date, &df, 0xFFFFFF);
            // the time, drawn with a real large font so it stays sharp
            let t = alloc::format!("{:02}:{:02}", hh, mm);
            ui.cv.text_big(w / 2, 100, &t, 4, 0xFFFFFF);
            let initial: String = name.chars().next().map(|c| c.to_ascii_uppercase()).into_iter().collect();
            ui.cv.gradient(w / 2 - 36, h - 260, 72, 72, 36, 0x8E8EFF, 0x5E5CE6);
            ui.cv.text_center(w / 2, h - 242, &initial, &gfx::font(32, true), 0xFFFFFF);
            ui.cv.text_center(w / 2, h - 176, &name, &gfx::font(20, true), 0xFFFFFF);
            if need_pw {
                ui.st.focus = id("lockpw");
                let shake = if now < wrong_at + 400 { ((now - wrong_at) as i32 / 40 % 2) * 12 - 6 } else { 0 };
                if ui.field_ex(id("lockpw"), (w / 2 - 130 + shake, h - 140, 260, 38), &mut typed, "Password", true) {
                    if crate::settings::check_password(&typed) {
                        unlock = true;
                    } else {
                        typed.clear();
                        wrong_at = now;
                    }
                }
                let (msg, col) = if now < wrong_at + 2500 && wrong_at != 0 { ("Incorrect password", 0xFF6B6B) } else { ("Type your password and press enter", 0xA0A0C0) };
                ui.cv.text_center(w / 2, h - 90, msg, &gfx::font(16, false), col);
            } else {
                let pulse = (now / 8 % 200) as i32;
                let a = if pulse < 100 { pulse } else { 200 - pulse };
                ui.cv.text_center(w / 2, h - 130, "Click or press any key to unlock", &gfx::font(16, false), gfx::mix(0x8080A0, 0xFFFFFF, (a * 2) as u32));
                if (inp.pressed || !inp.keys.is_empty()) && now > start + 400 {
                    unlock = true;
                }
            }
            ui.finish();
            draw_pointer(&mut ui.cv, inp.mouse);
        }
        framebuffer::with_screen(|fb| fb.present32(&buf, 0, 0, w as usize, h as usize));
        if unlock {
            break;
        }
        let _ = ui::inside;
        // the lock screen is nearly still: about 30 frames a second is plenty
        while crate::time::uptime_ms() < now + 33 {
            crate::sched::yield_idle();
        }
    }
}
