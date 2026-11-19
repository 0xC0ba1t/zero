//! VGA-style graphics: the 16 color palette and a renderer for code page 437
//! characters.
//!
//! The old screen was an 80x25 grid of CP437 characters and scripts still draw
//! with those codes. Instead of looking each one up in a font, the shapes are
//! drawn as pixels, scaled to whatever the cell size is: blocks fill the whole
//! cell, shades are real dither patterns, box-drawing lines meet in the middle
//! of the cell, dots and triangles are geometry. Only letters, digits and
//! punctuation come from the font.

use crate::graphics::display::Display;
use crate::graphics::font::Font;
use crate::graphics::framebuffer::Framebuffer;

/// The sixteen text colors, indexed like the VGA palette.
pub const PALETTE: [u32; 16] = [
    0x101018, // 0  black
    0x2E5BD8, // 1  blue
    0x2EA043, // 2  green
    0x1FA9B3, // 3  cyan
    0xD04040, // 4  red
    0xA64DB8, // 5  magenta
    0xB07A2A, // 6  brown
    0xD0D0D8, // 7  light gray
    0x6B6B78, // 8  dark gray
    0x6A8DFF, // 9  light blue
    0x5FE07A, // 10 light green
    0x5FE8F0, // 11 light cyan
    0xFF6B6B, // 12 light red
    0xFF7ADF, // 13 pink
    0xFFE45E, // 14 yellow
    0xFFFFFF, // 15 white
];

pub const BACKGROUND: u32 = PALETTE[0];

/// Names accepted by the `color` command, in palette order.
pub const COLOR_NAMES: [&str; 16] = [
    "black", "blue", "green", "cyan", "red", "magenta", "brown", "lightgray",
    "darkgray", "lightblue", "lightgreen", "lightcyan", "lightred", "pink",
    "yellow", "white",
];

/// Looks up a palette index by color name.
pub fn color_index(name: &str) -> Option<u8> {
    COLOR_NAMES
        .iter()
        .position(|candidate| name.eq_ignore_ascii_case(candidate))
        .map(|index| index as u8)
}

/// Line style of one arm of a box-drawing character.
const SINGLE: u8 = 1;
const DOUBLE: u8 = 2;

/// `(up, down, left, right)` arms of a CP437 box-drawing character.
fn box_arms(ch: u8) -> Option<(u8, u8, u8, u8)> {
    Some(match ch {
        179 => (1, 1, 0, 0),
        180 => (1, 1, 1, 0),
        181 => (1, 1, 2, 0),
        182 => (2, 2, 1, 0),
        183 => (0, 2, 1, 0),
        184 => (0, 1, 2, 0),
        185 => (2, 2, 2, 0),
        186 => (2, 2, 0, 0),
        187 => (0, 2, 2, 0),
        188 => (2, 0, 2, 0),
        189 => (2, 0, 1, 0),
        190 => (1, 0, 2, 0),
        191 => (0, 1, 1, 0),
        192 => (1, 0, 0, 1),
        193 => (1, 0, 1, 1),
        194 => (0, 1, 1, 1),
        195 => (1, 1, 0, 1),
        196 => (0, 0, 1, 1),
        197 => (1, 1, 1, 1),
        198 => (1, 1, 0, 2),
        199 => (2, 2, 0, 1),
        200 => (2, 0, 0, 2),
        201 => (0, 2, 0, 2),
        202 => (2, 0, 2, 2),
        203 => (0, 2, 2, 2),
        204 => (2, 2, 0, 2),
        205 => (0, 0, 2, 2),
        206 => (2, 2, 2, 2),
        207 => (1, 0, 2, 2),
        208 => (2, 0, 1, 1),
        209 => (0, 1, 2, 2),
        210 => (0, 2, 1, 1),
        211 => (2, 0, 0, 1),
        212 => (1, 0, 0, 2),
        213 => (0, 1, 0, 2),
        214 => (0, 2, 0, 1),
        215 => (2, 2, 1, 1),
        216 => (1, 1, 2, 2),
        217 => (1, 0, 1, 0),
        218 => (0, 1, 0, 1),
        _ => return None,
    })
}

/// Draws character `ch` (a code page 437 byte) into the `w` x `h` cell whose
/// top-left corner is `(x, y)`. The cell is always completely repainted.
pub fn draw_cell(
    fb: &mut Framebuffer,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    ch: u8,
    fg: u32,
    bg: u32,
    font: &Font,
) {
    fb.fill_rect(x, y, w, h, bg);
    match ch {
        0 | b' ' => {}
        219 => fb.fill_rect(x, y, w, h, fg),
        220 => fb.fill_rect(x, y + h / 2, w, h - h / 2, fg),
        223 => fb.fill_rect(x, y, w, h / 2, fg),
        221 => fb.fill_rect(x, y, w / 2, h, fg),
        222 => fb.fill_rect(x + w / 2, y, w - w / 2, h, fg),
        176..=178 => shade(fb, x, y, w, h, fg, ch - 175),
        254 => {
            let side = (w.min(h) * 3) / 5;
            fb.fill_rect(x + (w - side) / 2, y + (h - side) / 2, side, side, fg);
        }
        249 | 250 => {
            let d = if ch == 250 { (h / 14).max(2) } else { (h / 9).max(3) };
            fb.fill_rect(x + (w - d) / 2, y + (h - d) / 2, d, d, fg);
        }
        30 | 31 | 16 | 17 => triangle(fb, x, y, w, h, ch, fg),
        0x20..=0x7E => glyph(fb, x, y, w, h, ch as char, fg, bg, font),
        _ => match box_arms(ch) {
            Some(arms) => box_lines(fb, x, y, w, h, arms, fg),
            None => glyph(fb, x, y, w, h, '?', fg, bg, font),
        },
    }
}

/// A letter, digit or punctuation mark. When the cell is clearly bigger than
/// the font's glyph the glyph is stretched to fill the cell (the way a VGA
/// character fills its cell) instead of floating in the middle of it.
fn glyph(fb: &mut Framebuffer, x: usize, y: usize, w: usize, h: usize, c: char, fg: u32, bg: u32, font: &Font) {
    if w * 4 > font.width() * 5 || h * 4 > font.height() * 5 {
        fb.draw_alpha_scaled(x, y, w, h, font.raster(c), fg, bg);
    } else {
        let gx = x + w.saturating_sub(font.width()) / 2;
        let gy = y + h.saturating_sub(font.height()) / 2;
        font.draw_char(fb, gx, gy, c, fg, bg);
    }
}

/// Dither fill: level 1 is 25% covered, 2 is 50% and 3 is 75%.
fn shade(fb: &mut Framebuffer, x: usize, y: usize, w: usize, h: usize, fg: u32, level: u8) {
    let s = (h / 16).max(1); // size of one dither dot
    let cols = (w + s - 1) / s;
    let rows = (h + s - 1) / s;
    for ry in 0..rows {
        for rx in 0..cols {
            let on = match level {
                1 => rx % 2 == 0 && ry % 2 == 0,
                2 => (rx + ry) % 2 == 0,
                _ => !(rx % 2 == 1 && ry % 2 == 1),
            };
            if on {
                let px = rx * s;
                let py = ry * s;
                fb.fill_rect(x + px, y + py, s.min(w - px), s.min(h - py), fg);
            }
        }
    }
}

/// ▲ (30), ▼ (31), ► (16) and ◄ (17).
fn triangle(fb: &mut Framebuffer, x: usize, y: usize, w: usize, h: usize, ch: u8, fg: u32) {
    let (tw, th) = ((w * 3) / 4, (h * 2) / 3);
    let (ox, oy) = (x + (w - tw) / 2, y + (h - th) / 2);
    match ch {
        30 | 31 => {
            for i in 0..th {
                let row = if ch == 30 { i } else { th - 1 - i };
                let rw = ((row + 1) * tw / th).max(1);
                fb.fill_rect(ox + (tw - rw) / 2, oy + i, rw, 1, fg);
            }
        }
        _ => {
            // sideways: columns instead of rows
            for i in 0..tw {
                let col = if ch == 16 { i } else { tw - 1 - i };
                let rh = ((col + 1) * th / tw).max(1);
                fb.fill_rect(ox + i, oy + (th - rh) / 2, 1, rh, fg);
            }
        }
    }
}

/// Box-drawing characters: every arm runs from the middle of the cell to its edge.
fn box_lines(
    fb: &mut Framebuffer,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    (up, down, left, right): (u8, u8, u8, u8),
    fg: u32,
) {
    let t = (w.min(h) / 12).max(1); // stroke width
    let cx = x + w / 2;
    let cy = y + h / 2;
    // how far the strokes extend across the middle of the cell
    let vext = if up == DOUBLE || down == DOUBLE { 3 * t } else { t };
    let hext = if left == DOUBLE || right == DOUBLE { 3 * t } else { t };

    // strokes of one arm, as offsets of their left/top edge from the centre line
    let strokes = |style: u8| -> [Option<isize>; 2] {
        match style {
            SINGLE => [Some(-((t / 2) as isize)), None],
            DOUBLE => [Some(-((3 * t / 2) as isize)), Some((t / 2) as isize)],
            _ => [None, None],
        }
    };

    // vertical arms
    for (style, is_up) in [(up, true), (down, false)] {
        for off in strokes(style).into_iter().flatten() {
            let left_edge = (cx as isize + off) as usize;
            if is_up {
                fb.fill_rect(left_edge, y, t, (cy + hext / 2 + 1).saturating_sub(y), fg);
            } else {
                let top = cy.saturating_sub(hext / 2);
                fb.fill_rect(left_edge, top, t, (y + h).saturating_sub(top), fg);
            }
        }
    }
    // horizontal arms
    for (style, is_left) in [(left, true), (right, false)] {
        for off in strokes(style).into_iter().flatten() {
            let top_edge = (cy as isize + off) as usize;
            if is_left {
                fb.fill_rect(x, top_edge, (cx + vext / 2 + 1).saturating_sub(x), t, fg);
            } else {
                let start = cx.saturating_sub(vext / 2);
                fb.fill_rect(start, top_edge, (x + w).saturating_sub(start), t, fg);
            }
        }
    }
}

#[test_case]
fn test_color_names_resolve_to_their_palette_slot() {
    assert_eq!(color_index("black"), Some(0));
    assert_eq!(color_index("YELLOW"), Some(14));
    assert_eq!(color_index("white"), Some(15));
    assert_eq!(color_index("chartreuse"), None);
}

#[test_case]
fn test_box_characters_have_arms() {
    assert_eq!(box_arms(205), Some((0, 0, 2, 2)));
    assert_eq!(box_arms(179), Some((1, 1, 0, 0)));
    assert_eq!(box_arms(b'a'), None);
}
