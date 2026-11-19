//! The boot screen: pixel art drawn straight onto the framebuffer.
//!
//! The picture is laid out on a grid of "units" (`u` pixels each) so it keeps
//! its proportions at any resolution. Nothing here goes through a font except
//! the title and the footer text.

use crate::graphics::display::Display;
use crate::graphics::font::Font;
use crate::graphics::framebuffer::Framebuffer;
use noto_sans_mono_bitmap::RasterHeight;

const SKY: u32 = 0x1B1B25;
const ACCENT: u32 = 0x8FB4FF;
const GRAY: u32 = 0x9A9AA8;
const DOTS: u32 = 0x8A8A98;
const STAR: u32 = 0xF2F2F2;
const STAR_DIM: u32 = 0x6E6E7C;
const CLOUD: u32 = 0x7C7C8A;
const MOON: u32 = 0xF6F6F6;
const MOON_SHADE: u32 = 0xB4B4C0;

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

fn sprite(fb: &mut Framebuffer, x: usize, y: usize, px: usize, rows: &[&str], color: u32) {
    for (ry, row) in rows.iter().enumerate() {
        for (rx, c) in row.bytes().enumerate() {
            if c != b'X' {
                continue;
            }
            fb.fill_rect(x + rx * px, y + ry * px, px, px, color);
        }
    }
}

/// Fills `(x, y, w, h)` with a checkerboard of `d`-pixel dots. The pattern is
/// anchored to the screen, so overlapping rectangles line up.
fn stipple(fb: &mut Framebuffer, x: usize, y: usize, w: usize, h: usize, d: usize, color: u32) {
    let (gx0, gx1) = (x / d, (x + w + d - 1) / d);
    let (gy0, gy1) = (y / d, (y + h + d - 1) / d);
    for gy in gy0..gy1 {
        for gx in gx0..gx1 {
            if (gx + gy) % 2 == 0 {
                let px = (gx * d).max(x);
                let py = (gy * d).max(y);
                let pw = ((gx + 1) * d).min(x + w).saturating_sub(px);
                let ph = ((gy + 1) * d).min(y + h).saturating_sub(py);
                fb.fill_rect(px, py, pw, ph, color);
            }
        }
    }
}

fn cloud(fb: &mut Framebuffer, x: usize, y: usize, half: usize, shape: &[(usize, usize, usize, usize)]) {
    for &(rx, ry, rw, rh) in shape {
        stipple(fb, x + rx * half, y + ry * half, rw * half, rh * half, (half / 3).max(1), CLOUD);
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

/// Draws the whole boot screen and shows it.
pub fn draw(fb: &mut Framebuffer) {
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
    let tx = title_font.draw_text(fb, x0, y0, "Welcome to Zero ", ACCENT, SKY);
    title_font.draw_text(fb, tx, y0, &version, GRAY, SKY);

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

    // clouds
    cloud(fb, x0 + 67 * u / 10, top + 16 * u, half, &CLOUD_A);
    cloud(fb, x0 + 744 * u / 10, top + 36 * u, half, &CLOUD_B);
    cloud(fb, x0 + 18 * u, top + 52 * u, half, &CLOUD_C);

    // moon
    moon(fb, x0 + 117 * u, top + 21 * u, half);

    // stars
    let sp = (u / 3).max(1);
    let stars: [(usize, usize, u32); 10] = [
        (13, 9, STAR),
        (80, 14, STAR),
        (63, 29, STAR),
        (3, 39, STAR_DIM),
        (136, 54, STAR_DIM),
        (100, 59, STAR),
        (54, 64, STAR),
        (34, 42, STAR_DIM),
        (90, 6, STAR_DIM),
        (122, 44, STAR),
    ];
    for (sx, sy, color) in stars {
        sprite(fb, x0 + sx * u, top + sy * u, sp, &STAR_SPRITE, color);
    }

    // footer
    let fy = bottom + 4 * u;
    let after = foot_font.draw_text(fb, x0, fy, "Press ", GRAY, SKY);
    let after = foot_font.draw_text(fb, after, fy, "any key", ACCENT, SKY);
    foot_font.draw_text(fb, after, fy, " to enter", GRAY, SKY);
    let info = alloc::format!("{}x{}", w, h);
    let iw = foot_font.text_width(&info);
    foot_font.draw_text(fb, x0 + panel_w - iw, fy, &info, STAR_DIM, SKY);

    fb.flush();
}
