//! Drawing helpers shared by the desktop and its applications: blended
//! text, discs, rounded gradients, the dock icons and the mouse pointer.

use crate::graphics::font::Font;
use crate::graphics::framebuffer::{Framebuffer, Rect};
use crate::graphics::display::Display;
use crate::graphics::ui;
use core::sync::atomic::{AtomicU32, Ordering};
use noto_sans_mono_bitmap::RasterHeight;

// ---- colors (One Dark, with the accent the user picked) --------------------

pub const BG: u32 = 0x282C34;
pub const BG_DARK: u32 = 0x21252B;
pub const BG_PANEL: u32 = 0x2F343E;
pub const BG_HOVER: u32 = 0x3A3F4B;
pub const BORDER: u32 = 0x464B57;
pub const TEXT: u32 = 0xDCE0E5;
pub const TEXT_MUTED: u32 = 0xA9AFBC;
pub const TEXT_FAINT: u32 = 0x6B717D;
pub const RED: u32 = 0xE06C75;
pub const GREEN: u32 = 0x98C379;
pub const YELLOW: u32 = 0xE5C07B;

static ACCENT_COLOR: AtomicU32 = AtomicU32::new(0x61AFEF);

pub fn accent() -> u32 {
    ACCENT_COLOR.load(Ordering::Relaxed)
}

pub fn set_accent(color: u32) {
    ACCENT_COLOR.store(color, Ordering::Relaxed);
}

/// The whole surface as a clip rectangle.
pub fn full(fb: &Framebuffer) -> Rect {
    (0, 0, fb.width() as i32, fb.height() as i32)
}

/// A font of about `px` pixels high.
pub fn font(px: usize, bold: bool) -> Font {
    let size = if px >= 30 {
        RasterHeight::Size32
    } else if px >= 22 {
        RasterHeight::Size24
    } else if px >= 18 {
        RasterHeight::Size20
    } else {
        RasterHeight::Size16
    };
    if bold {
        Font::bold(size)
    } else {
        Font::regular(size)
    }
}

/// Draws text blended over what is already there (no background box).
pub fn text(fb: &mut Framebuffer, font: &Font, x: i32, y: i32, s: &str, color: u32) -> i32 {
    let w = font.width() as i32;
    let mut cx = x;
    let (c0, c1, c2, c3) = fb.clip_bounds();
    let (c0, c1, c2, c3) = (c0 as i32, c1 as i32, c2 as i32, c3 as i32);
    for c in s.chars() {
        if cx + w > c0 && cx < c2 {
            let glyph = font.raster(c);
            for (ry, row) in glyph.iter().enumerate() {
                let yy = y + ry as i32;
                if yy < c1 || yy >= c3 {
                    continue;
                }
                for (rx, &a) in row.iter().enumerate() {
                    let xx = cx + rx as i32;
                    if a > 8 && xx >= c0 && xx < c2 {
                        fb.blend_pixel(xx as usize, yy as usize, color, a as u32);
                    }
                }
            }
        }
        cx += w;
    }
    cx
}

/// Draws text with its middle at `cx`.
pub fn text_centered(fb: &mut Framebuffer, font: &Font, cx: i32, y: i32, s: &str, color: u32) {
    let w = font.text_width(s) as i32;
    text(fb, font, cx - w / 2, y, s, color);
}

/// Text cut to `max_w` pixels (with `..` when it does not fit).
pub fn fit_text(font: &Font, s: &str, max_w: i32) -> alloc::string::String {
    let cw = font.width() as i32;
    let max_chars = (max_w / cw).max(0) as usize;
    if s.chars().count() <= max_chars {
        return alloc::string::String::from(s);
    }
    let keep = max_chars.saturating_sub(2);
    let mut out: alloc::string::String = s.chars().take(keep).collect();
    out.push_str("..");
    out
}

/// A flat rectangle.
pub fn rect(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, color: u32) {
    let clip = full(fb);
    fb.tint_rect(x, y, w, h, color, 255, clip);
}

/// A translucent rectangle.
pub fn tint(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, color: u32, alpha: u32) {
    let clip = full(fb);
    fb.tint_rect(x, y, w, h, color, alpha, clip);
}

/// A rectangle with rounded corners.
pub fn round(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, r: i32, color: u32) {
    let clip = full(fb);
    fb.fill_rounded(x, y, w, h, r, color, clip);
}

/// A one pixel outline.
pub fn outline(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, color: u32) {
    rect(fb, x, y, w, 1, color);
    rect(fb, x, y + h - 1, w, 1, color);
    rect(fb, x, y, 1, h, color);
    rect(fb, x + w - 1, y, 1, h, color);
}

fn isqrt(n: i64) -> i64 {
    let mut r = 0;
    while (r + 1) * (r + 1) <= n {
        r += 1;
    }
    r
}

/// A filled circle.
pub fn disc(fb: &mut Framebuffer, cx: i32, cy: i32, r: i32, color: u32) {
    let clip = full(fb);
    for dy in -r..=r {
        let span = isqrt((r * r - dy * dy) as i64) as i32;
        fb.tint_rect(cx - span, cy + dy, span * 2 + 1, 1, color, 255, clip);
    }
}

/// A rounded square filled with a vertical gradient.
pub fn gradient_round(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, r: i32, top: u32, bottom: u32, clip: Rect) {
    let r = r.min(w / 2).min(h / 2).max(0);
    for row in 0..h {
        let d = if row < r {
            r - row
        } else if row + r >= h {
            row + r - h + 1
        } else {
            0
        };
        let inset = if d == 0 { 0 } else { r - isqrt((r * r - (d - 1) * (d - 1)) as i64) as i32 };
        let t = (row * 255 / h.max(1)) as u32;
        fb.tint_rect(x + inset, y + row, w - 2 * inset, 1, ui::lerp(top, bottom, t), 255, clip);
    }
}

// ---- the pointer ------------------------------------------------------------

const POINTER: [&str; 18] = [
    "X           ",
    "XX          ",
    "XOX         ",
    "XOOX        ",
    "XOOOX       ",
    "XOOOOX      ",
    "XOOOOOX     ",
    "XOOOOOOX    ",
    "XOOOOOOOX   ",
    "XOOOOOOOOX  ",
    "XOOOOOOOOOX ",
    "XOOOOOOXXXXX",
    "XOOXOOX     ",
    "XOX XOOX    ",
    "XX  XOOX    ",
    "X    XOOX   ",
    "     XOOX   ",
    "      XX    ",
];

/// The size of the pointer on screen.
pub fn pointer_size(scale: i32) -> (i32, i32) {
    (12 * scale, 18 * scale)
}

pub fn draw_pointer(fb: &mut Framebuffer, x: i32, y: i32, scale: i32) {
    let (w, h) = (fb.width() as i32, fb.height() as i32);
    for (ry, row) in POINTER.iter().enumerate() {
        for (rx, c) in row.bytes().enumerate() {
            let color = match c {
                b'X' => 0x101216,
                b'O' => 0xFFFFFF,
                _ => continue,
            };
            let (px, py) = (x + rx as i32 * scale, y + ry as i32 * scale);
            if px >= 0 && py >= 0 && px + scale <= w && py + scale <= h {
                let clip = (0, 0, w, h);
                fb.tint_rect(px, py, scale, scale, color, 255, clip);
            }
        }
    }
}

// ---- dock icons --------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Terminal,
    Files,
    Settings,
    Browser,
    Monitor,
    Snake,
    Pong,
    Invaders,
    TicTacToe,
    Connect4,
    Calculator,
    Bounce,
    Doom,
    Text,
    Folder,
    Document,
}

const ALIEN: [&str; 8] = [
    "..X.....X..",
    "...X...X...",
    "..XXXXXXX..",
    ".XX.XXX.XX.",
    "XXXXXXXXXXX",
    "X.XXXXXXX.X",
    "X.X.....X.X",
    "...XX.XX...",
];

/// Draws an icon in a `size` square at `(x, y)`.
pub fn icon(fb: &mut Framebuffer, which: Icon, x: i32, y: i32, size: i32, clip: Rect) {
    let u = |v: i32| v * size / 48;
    let (top, bottom) = match which {
        Icon::Terminal => (0x3E4451, 0x1B1E24),
        Icon::Files => (0x6CB6FF, 0x2F6FD0),
        Icon::Settings => (0xA7B1C2, 0x5E6A80),
        Icon::Browser => (0x5AC8FA, 0x1F6FEB),
        Icon::Monitor => (0x3C4350, 0x1D2128),
        Icon::Snake => (0x7BD88F, 0x2C8C4A),
        Icon::Pong => (0x2B303B, 0x14171C),
        Icon::Invaders => (0x6A4C93, 0x2A1B47),
        Icon::TicTacToe => (0xF6AD55, 0xD9642B),
        Icon::Connect4 => (0x3B82F6, 0x1E40AF),
        Icon::Calculator => (0x4B5262, 0x23272E),
        Icon::Bounce => (0xFFB86B, 0xE5533D),
        Icon::Doom => (0xC62828, 0x4A0A0A),
        Icon::Text => (0x8AB4F8, 0x4A7BD0),
        Icon::Folder => (0x6CB6FF, 0x2F6FD0),
        Icon::Document => (0xE8EAEE, 0xB9BFC9),
    };
    gradient_round(fb, x, y, size, size, size / 5, top, bottom, clip);
    let mut r = |dx: i32, dy: i32, w: i32, h: i32, c: u32| fb.tint_rect(x + u(dx), y + u(dy), u(w).max(1), u(h).max(1), c, 255, clip);
    match which {
        Icon::Terminal => {
            for i in 0..6 {
                r(12 + i * 2, 13 + i * 2, 4, 3, 0xA6E3A1);
                r(12 + i * 2, 29 - i * 2, 4, 3, 0xA6E3A1);
            }
            r(26, 33, 12, 3, 0xDCE0E5);
        }
        Icon::Files | Icon::Folder => {
            r(8, 13, 16, 7, 0xBFD9FF);
            r(8, 18, 32, 22, 0xEAF3FF);
            r(8, 24, 32, 16, 0xD3E6FF);
        }
        Icon::Settings => {
            drop(r);
            disc(fb, x + u(24), y + u(24), u(13), 0xF4F6FA);
            for k in 0..8 {
                let (dx, dy) = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)][k];
                disc(fb, x + u(24) + dx * u(15), y + u(24) + dy * u(15), u(4).max(2), 0xF4F6FA);
            }
            disc(fb, x + u(24), y + u(24), u(6), 0x6B778C);
        }
        Icon::Browser => {
            drop(r);
            disc(fb, x + u(24), y + u(24), u(17), 0xF4F6FA);
            disc(fb, x + u(24), y + u(24), u(15), 0x2F7BEF);
            let c = |fb: &mut Framebuffer, dx, dy, w, h| fb.tint_rect(x + u(dx), y + u(dy), u(w).max(1), u(h).max(1), 0xF4F6FA, 255, clip);
            c(fb, 8, 23, 32, 2);
            c(fb, 23, 8, 2, 32);
            c(fb, 11, 15, 26, 2);
            c(fb, 11, 31, 26, 2);
            disc(fb, x + u(24), y + u(24), u(8), 0x2F7BEF);
            disc(fb, x + u(24), y + u(24), u(6), 0x2F7BEF);
            c(fb, 23, 8, 2, 32);
            c(fb, 8, 23, 32, 2);
        }
        Icon::Monitor => {
            r(9, 28, 6, 10, 0x98C379);
            r(18, 20, 6, 18, 0x61AFEF);
            r(27, 12, 6, 26, 0xE5C07B);
            r(36, 24, 4, 14, 0xE06C75);
        }
        Icon::Snake => {
            for (dx, dy) in [(9, 30), (17, 30), (25, 30), (25, 22), (25, 14), (33, 14)] {
                r(dx, dy, 7, 7, 0xE3F9D8);
            }
            r(33, 14, 7, 7, 0xFFFFFF);
            r(37, 16, 2, 2, 0x1B5E2F);
        }
        Icon::Pong => {
            r(8, 13, 4, 20, 0xF4F6FA);
            r(36, 17, 4, 20, 0xF4F6FA);
            for k in 0..5 {
                r(23, 8 + k * 8, 2, 4, 0x6B717D);
            }
            r(26, 22, 4, 4, 0xFFFFFF);
        }
        Icon::Invaders => {
            for (ry, row) in ALIEN.iter().enumerate() {
                for (rx, c) in row.bytes().enumerate() {
                    if c == b'X' {
                        r(7 + rx as i32 * 3, 12 + ry as i32 * 3, 3, 3, 0xB5F5B0);
                    }
                }
            }
        }
        Icon::TicTacToe => {
            r(18, 9, 3, 30, 0xFFF3E0);
            r(28, 9, 3, 30, 0xFFF3E0);
            r(9, 18, 30, 3, 0xFFF3E0);
            r(9, 28, 30, 3, 0xFFF3E0);
            r(11, 10, 4, 4, 0xFFFFFF);
            r(14, 13, 4, 4, 0xFFFFFF);
            drop(r);
            disc(fb, x + u(34), y + u(34), u(4), 0xFFFFFF);
        }
        Icon::Connect4 => {
            drop(r);
            for row in 0..3 {
                for col in 0..3 {
                    let color = match (row + col) % 3 {
                        0 => 0xFF5252,
                        1 => 0xFFD740,
                        _ => 0x0F2A6B,
                    };
                    disc(fb, x + u(12 + col * 12), y + u(12 + row * 12), u(5).max(2), color);
                }
            }
        }
        Icon::Calculator => {
            r(9, 9, 30, 9, 0x98C379);
            for row in 0..3 {
                for col in 0..4 {
                    let color = if col == 3 { 0xF6AD55 } else { 0xDCE0E5 };
                    r(9 + col * 8, 22 + row * 7, 6, 5, color);
                }
            }
        }
        Icon::Bounce => {
            drop(r);
            disc(fb, x + u(24), y + u(22), u(12), 0xFFFFFF);
            fb.tint_rect(x + u(12), y + u(37), u(24), u(3).max(1), 0x7A2E1F, 255, clip);
        }
        Icon::Doom => {
            let f = font((size / 3) as usize, true);
            text_centered(fb, &f, x + size / 2, y + size / 2 - f.height() as i32 / 2, "DOOM", 0xFFD9A0);
        }
        Icon::Text => {
            r(10, 12, 28, 3, 0xFFFFFF);
            r(10, 20, 28, 3, 0xFFFFFF);
            r(10, 28, 20, 3, 0xFFFFFF);
        }
        Icon::Document => {
            r(11, 8, 26, 32, 0xFFFFFF);
            r(15, 16, 18, 2, 0x9AA3B2);
            r(15, 22, 18, 2, 0x9AA3B2);
            r(15, 28, 12, 2, 0x9AA3B2);
        }
    }
}
