//! The boot screen: pixel art drawn straight onto the framebuffer, gently
//! animated.
//!
//! The still parts (sky, moon, rules, text) are drawn once and kept as a
//! snapshot of the picture. Every frame the snapshot is put back and the
//! moving parts are drawn on top: stars that twinkle, clouds that drift, and
//! now and then a shooting star.
//!
//! The picture is laid out on a grid of "units" (`u` pixels each) so it keeps
//! its proportions at any resolution.

use crate::graphics::display::Display;
use crate::graphics::font::Font;
use crate::graphics::framebuffer::{self, Framebuffer};
use crate::graphics::ui::{self, lerp};
use alloc::vec;
use alloc::vec::Vec;
use noto_sans_mono_bitmap::RasterHeight;

const SKY: u32 = 0x21252B;
const TEXT_MUTED: u32 = ui::TEXT_MUTED;
const DOTS: u32 = 0x464B57;
const STAR: u32 = 0xDCE0E5;
const STAR_DIM: u32 = 0x4B5263;
const CLOUD: u32 = 0x4B5263;
const MOON: u32 = 0xDCE0E5;
const MOON_SHADE: u32 = 0x9AA1AE;

/// A small asterisk, drawn at pixel size `p`.
const STAR_SPRITE: [&str; 5] = [
    "..X..",
    "X.X.X",
    ".XXX.",
    "X.X.X",
    "..X..",
];

/// Cloud shapes as rectangles `(x, y, w, h)` in half units.
const CLOUD_A: [(usize, usize, usize, usize); 4] = [(0, 20, 92, 10), (5, 10, 15, 10), (34, 10, 48, 10), (44, 0, 28, 10)];
const CLOUD_B: [(usize, usize, usize, usize); 4] = [(0, 20, 77, 10), (9, 10, 10, 10), (19, 0, 20, 20), (39, 10, 9, 10)];
const CLOUD_C: [(usize, usize, usize, usize); 3] = [(0, 14, 52, 8), (8, 7, 14, 7), (22, 0, 20, 14)];

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
}

fn sprite(fb: &mut Framebuffer, x: usize, y: usize, px: usize, rows: &[&str], color: u32) {
    for (ry, row) in rows.iter().enumerate() {
        for (rx, c) in row.bytes().enumerate() {
            if c == b'X' {
                fb.fill_rect(x + rx * px, y + ry * px, px, px, color);
            }
        }
    }
}

/// A cloud as a stipple mask (non-zero = a dot), so it can be moved cheaply.
struct Cloud {
    mask: Vec<u8>,
    w: usize,
    h: usize,
    /// Position in 24.8 fixed point (x) and pixels (y).
    x: i64,
    y: i32,
    /// How far it drifts per frame, 24.8 fixed point.
    speed: i64,
}

impl Cloud {
    fn new(shape: &[(usize, usize, usize, usize)], half: usize, x: usize, y: usize, speed: i64) -> Cloud {
        let w = shape.iter().map(|&(rx, _, rw, _)| (rx + rw) * half).max().unwrap_or(1);
        let h = shape.iter().map(|&(_, ry, _, rh)| (ry + rh) * half).max().unwrap_or(1);
        let mut mask = vec![0u8; w * h];
        let d = (half / 3).max(1);
        for &(rx, ry, rw, rh) in shape {
            for py in ry * half..(ry + rh) * half {
                for px in rx * half..(rx + rw) * half {
                    if (px / d + py / d) % 2 == 0 {
                        mask[py * w + px] = 1;
                    }
                }
            }
        }
        Cloud { mask, w, h, x: (x as i64) << 8, y: y as i32, speed }
    }

    fn step(&mut self, screen_w: usize) {
        self.x += self.speed;
        if (self.x >> 8) as usize > screen_w && self.x > 0 {
            self.x = -((self.w as i64) << 8);
        }
    }

    fn draw(&self, fb: &mut Framebuffer) {
        fb.blit_mask((self.x >> 8) as i32, self.y, self.w, self.h, &self.mask, CLOUD);
    }
}

/// A crescent opening to the right, built from square blocks.
fn moon(fb: &mut Framebuffer, cx: usize, cy: usize, block: usize) {
    const R: i32 = 31; // outer radius in blocks
    const INNER_DX: i32 = 16; // inner circle's offset
    const INNER_R: i32 = 25;
    const SHADE_R: i32 = INNER_R + 3;
    for by in -R..=R {
        for bx in -R..=R {
            let outer = bx * bx + by * by;
            let inner = (bx - INNER_DX) * (bx - INNER_DX) + by * by;
            if outer > R * R - R || inner <= INNER_R * INNER_R {
                continue;
            }
            // the rim along the inner edge is dithered to look like shading
            let color = if inner <= SHADE_R * SHADE_R && (bx + by) % 2 == 0 { MOON_SHADE } else { MOON };
            let px = (cx as i32 + bx * block as i32) as usize;
            let py = (cy as i32 + by * block as i32) as usize;
            fb.fill_rect(px, py, block, block, color);
        }
    }
}

struct Shooting {
    x: i32,
    y: i32,
    vx: i32,
    vy: i32,
}

struct Scene {
    snapshot: Vec<u8>,
    width: usize,
    u: usize,
    sp: usize,
    x0: usize,
    top: usize,
    bottom: usize,
    clouds: [Cloud; 3],
    /// Star positions `(x, y, phase)` in pixels.
    stars: Vec<(usize, usize, u32)>,
    shooting: Option<Shooting>,
    next_shooting: u32,
    frame: u32,
    rng: Rng,
}

impl Scene {
    /// Draws the still picture, remembers it, and sets up the moving parts.
    fn new(fb: &mut Framebuffer) -> Scene {
        let (w, h) = (fb.width(), fb.height());
        let u = (w / 150).min(h / 90).max(1);
        let half = (u / 2).max(1);

        fb.clear_to(SKY);

        let title_font = Font::regular(RasterHeight::Size32);
        let foot_font = Font::regular(RasterHeight::Size24);
        let panel_w = 140 * u;
        let panel_h = 70 * u;
        let total = title_font.height() + 3 * u + panel_h + 4 * u + foot_font.height();
        let x0 = w.saturating_sub(panel_w) / 2;
        let y0 = h.saturating_sub(total) / 2;
        let top = y0 + title_font.height() + 3 * u;
        let bottom = top + panel_h;

        // title
        let version = alloc::format!("v{}", env!("CARGO_PKG_VERSION"));
        let tx = title_font.draw_text(fb, x0, y0, "Welcome to Zero ", ui::ACCENT, SKY);
        title_font.draw_text(fb, tx, y0, &version, TEXT_MUTED, SKY);

        // dotted rules
        let dot = (u / 4).max(1);
        let pitch = (u * 2 / 3).max(dot + 1);
        for line_y in [top, bottom] {
            let mut x = x0;
            while x + dot <= x0 + panel_w {
                fb.fill_rect(x, line_y, dot, dot, DOTS);
                x += pitch;
            }
        }

        moon(fb, x0 + 117 * u, top + 21 * u, half);

        // footer
        let fy = bottom + 4 * u;
        let after = foot_font.draw_text(fb, x0, fy, "Press ", TEXT_MUTED, SKY);
        let after = foot_font.draw_text(fb, after, fy, "any key", ui::ACCENT, SKY);
        foot_font.draw_text(fb, after, fy, " to enter", TEXT_MUTED, SKY);
        let info = alloc::format!("{}x{}", w, h);
        let iw = foot_font.text_width(&info);
        foot_font.draw_text(fb, x0 + panel_w - iw, fy, &info, ui::TEXT_FAINT, SKY);

        let snapshot = fb.snapshot();

        let rel = |ux: usize, uy: usize| (x0 + ux * u, top + uy * u);
        let (ax, ay) = rel(7, 16);
        let (bx, by) = rel(74, 36);
        let (cx, cy) = rel(18, 52);
        let drift = |per_u: i64| (u as i64 * 256 / 40) * per_u / 10;
        let clouds = [
            Cloud::new(&CLOUD_A, half, ax, ay, drift(10)),
            Cloud::new(&CLOUD_B, half, bx, by, drift(17)),
            Cloud::new(&CLOUD_C, half, cx, cy, drift(7)),
        ];
        let star_units: [(usize, usize); 10] =
            [(13, 9), (80, 14), (63, 29), (3, 39), (136, 54), (100, 59), (54, 64), (34, 42), (90, 6), (122, 44)];
        let stars = star_units
            .iter()
            .enumerate()
            .map(|(i, &(sx, sy))| (x0 + sx * u, top + sy * u, i as u32 * 3))
            .collect();

        Scene {
            snapshot,
            width: w,
            u,
            sp: (u / 3).max(1),
            x0,
            top,
            bottom,
            clouds,
            stars,
            shooting: None,
            next_shooting: 40,
            frame: 0,
            rng: Rng(crate::time::ticks() as u32 | 0x9E37_79B9),
        }
    }

    /// Draws one frame of the animation.
    fn frame(&mut self, fb: &mut Framebuffer) {
        self.frame += 1;
        fb.restore(&self.snapshot);

        for cloud in self.clouds.iter_mut() {
            cloud.step(self.width);
            cloud.draw(fb);
        }

        // twinkling: each star slowly cycles dim -> bright -> dim, out of step
        for &(sx, sy, phase) in &self.stars {
            let l = (self.frame / 5 + phase) % 8;
            let v = if l < 4 { l } else { 7 - l };
            sprite(fb, sx, sy, self.sp, &STAR_SPRITE, lerp(STAR_DIM, STAR, v * 85));
        }

        // a shooting star every few seconds
        if self.shooting.is_none() && self.frame >= self.next_shooting {
            let u = self.u as i32;
            let start_x = self.x0 as i32 + (self.rng.next() % (60 * self.u as u32)) as i32;
            let start_y = self.top as i32 + (self.rng.next() % (14 * self.u as u32)) as i32;
            self.shooting = Some(Shooting { x: start_x, y: start_y, vx: u * 3, vy: u + (self.rng.next() % self.u as u32) as i32 });
            self.next_shooting = self.frame + 150 + self.rng.next() % 150;
        }
        let mut done = false;
        if let Some(s) = self.shooting.as_mut() {
            s.x += s.vx;
            s.y += s.vy;
            let t = (self.sp as i32).max(2);
            const TAIL: i32 = 36;
            for i in 0..TAIL {
                let px = s.x - s.vx * i / 9;
                let py = s.y - s.vy * i / 9;
                if px < 0 || py < 0 {
                    continue;
                }
                let size = (t + 1 - i * (t + 1) / (TAIL / 2)).max(1) as usize;
                let color = lerp(SKY, 0xFFFFFF, (255 - i * 255 / TAIL) as u32);
                fb.fill_rect(px as usize, py as usize, size, size, color);
            }
            done = s.x as usize > self.x0 + 140 * self.u || s.y as usize > self.bottom;
        }
        if done {
            self.shooting = None;
        }

        fb.flush();
    }
}

/// Shows the boot screen until a key is pressed.
pub fn run() {
    let scene = framebuffer::with(|fb| Scene::new(fb));
    let mut scene = match scene {
        Some(scene) => scene,
        None => {
            let _ = crate::input::read_key();
            return;
        }
    };
    let mut last = crate::time::ticks();
    loop {
        framebuffer::with(|fb| scene.frame(fb));
        if crate::input::try_read_key().is_some() {
            return;
        }
        // about 30 frames per second (the timer ticks at 100 Hz)
        while crate::time::ticks() < last + 3 {
            x86_64::instructions::hlt();
        }
        last = crate::time::ticks();
    }
}
