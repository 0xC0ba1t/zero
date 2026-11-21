//! Icons for apps and file types, drawn with vector-ish primitives at any size.

use super::gfx::{mix, Canvas};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Terminal,
    Files,
    Settings,
    Browser,
    Activity,
    Media,
    Buddy,
    Notes,
    Launchpad,
    Trash,
    TrashFull,
    Snake,
    Pong,
    Invaders,
    TicTacToe,
    Connect4,
    Calculator,
    Bounce,
    Doom,
    Script,
    // file types
    Folder,
    Document,
    Image,
    Audio,
    Video,
    Code,
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

fn squircle(cv: &mut Canvas, x: i32, y: i32, s: i32, top: u32, bottom: u32) {
    let r = s * 23 / 100;
    cv.shadow(x, y + s / 40, s, s, r, (s / 14).max(2), 70);
    cv.gradient(x, y, s, s, r, top, bottom);
    // a soft highlight along the top edge and a hairline border
    cv.round_outline(x, y, s, s, r, 0xFFFFFF, 40);
}

/// Draws `icon` in a `s` x `s` square.
pub fn draw(cv: &mut Canvas, icon: Icon, x: i32, y: i32, s: i32) {
    let u = |v: i32| v * s / 64;
    match icon {
        Icon::Folder => {
            // a folder, no squircle
            cv.round(x + u(4), y + u(12), u(24), u(10), u(3), 0x5FA8F5);
            cv.gradient(x + u(4), y + u(17), u(56), u(38), u(5), 0x7DBBFA, 0x4A94EA);
            cv.rect_a(x + u(4), y + u(22), u(56), 1, 0xFFFFFF, 90);
            return;
        }
        Icon::Document | Icon::Image | Icon::Audio | Icon::Video | Icon::Code => {
            let (px, py, pw, ph) = (x + u(12), y + u(4), u(40), u(56));
            cv.shadow(px, py, pw, ph, u(3), u(3).max(1), 60);
            cv.round(px, py, pw, ph, u(4), 0xFBFBFD);
            cv.round_outline(px, py, pw, ph, u(4), 0x000000, 40);
            // folded corner
            cv.rect(px + pw - u(12), py, u(12), u(12), mix(0xFBFBFD, 0xD8D8DE, 200));
            match icon {
                Icon::Image => {
                    cv.gradient(px + u(5), py + u(16), pw - u(10), u(26), u(2), 0x6EC6FF, 0x2F80ED);
                    cv.circle(px + u(26), py + u(23), u(4), 0xFFE07A);
                    for i in 0..u(26) {
                        let hh = (i * 2).min(u(14)).min((u(26) - i) * 2);
                        cv.rect(px + u(5) + i, py + u(42) - hh, 1, hh, 0x2E9E5B);
                    }
                }
                Icon::Audio => {
                    cv.circle(px + u(15), py + u(40), u(5), 0xE5487A);
                    cv.circle(px + u(29), py + u(36), u(5), 0xE5487A);
                    cv.rect(px + u(19), py + u(18), u(2), u(22), 0xE5487A);
                    cv.rect(px + u(33), py + u(14), u(2), u(22), 0xE5487A);
                    cv.thick_line(px + u(20), py + u(18), px + u(34), py + u(14), u(3).max(2), 0xE5487A);
                }
                Icon::Video => {
                    cv.round(px + u(5), py + u(18), pw - u(10), u(24), u(3), 0x2B2D33);
                    for k in 0..3 {
                        cv.circle(px + u(15) + k * u(5), py + u(30) + 0, u(1).max(1), 0x2B2D33);
                    }
                    let (tx, ty) = (px + u(16), py + u(23));
                    for i in 0..u(10) {
                        cv.rect(tx + i, ty + i / 2, 1, u(14) - i, 0xFFFFFF);
                    }
                }
                Icon::Code => {
                    cv.text(px + u(6), py + u(22), "zs", &super::gfx::font(u(18), true), 0x7B61FF);
                    for k in 0..3 {
                        cv.rect(px + u(6), py + u(42) + k * u(4), u(20 + k * 4), u(2).max(1), 0xB8B8C4);
                    }
                }
                _ => {
                    for k in 0..6 {
                        cv.rect(px + u(6), py + u(16) + k * u(6), pw - u(12) - (k % 3) * u(5), u(2).max(1), 0xB8B8C4);
                    }
                }
            }
            return;
        }
        Icon::Trash | Icon::TrashFull => {
            let (bx, by, bw, bh) = (x + u(14), y + u(14), u(36), u(46));
            cv.shadow(bx, by, bw, bh, u(5), u(3).max(1), 60);
            cv.gradient(bx, by, bw, bh, u(5), 0xE9EAEE, 0xB9BCC4);
            for k in 0..4 {
                cv.rect_a(bx + u(6) + k * u(8), by + u(8), u(2).max(1), bh - u(14), 0x6B6F78, 120);
            }
            cv.round(x + u(10), y + u(8), u(44), u(8), u(3), 0xD5D7DD);
            if icon == Icon::TrashFull {
                cv.round(x + u(18), y + u(2), u(28), u(10), u(4), 0xF6F0D8);
            }
            return;
        }
        _ => {}
    }
    let (top, bottom) = match icon {
        Icon::Terminal => (0x3A3D46, 0x15171B),
        Icon::Files => (0x7CC4FF, 0x1F6FE0),
        Icon::Settings => (0xB4B9C4, 0x6A717E),
        Icon::Browser => (0x8ED1FF, 0x1677F0),
        Icon::Activity => (0x2F333B, 0x121417),
        Icon::Media => (0xFFFFFF, 0xE9EBF0),
        Icon::Buddy => (0xA8E6FF, 0x5B8DEF),
        Icon::Notes => (0xFFE58A, 0xF5C542),
        Icon::Launchpad => (0x9AA3B4, 0x4D5566),
        Icon::Snake => (0x8BE29B, 0x23914A),
        Icon::Pong => (0x30343D, 0x101216),
        Icon::Invaders => (0x7A57B5, 0x2A1B4B),
        Icon::TicTacToe => (0xFFC26B, 0xE0602A),
        Icon::Connect4 => (0x5B9BFF, 0x1E43B5),
        Icon::Calculator => (0x50555F, 0x22252B),
        Icon::Bounce => (0xFFB86B, 0xEA4C3C),
        Icon::Doom => (0xD32F2F, 0x3E0909),
        Icon::Script => (0x9C8CFF, 0x5A3FE0),
        _ => (0x888888, 0x444444),
    };
    squircle(cv, x, y, s, top, bottom);
    let white = 0xFFFFFF;
    match icon {
        Icon::Terminal => {
            let f = super::gfx::font(u(24), true);
            cv.text(x + u(12), y + u(16), ">_", &f, 0x9EF0A8);
        }
        Icon::Files => {
            // a smiling face, like a friendly file manager
            cv.round(x + u(32), y + u(6), u(26), u(52), u(6), 0xEAF4FF);
            cv.rect(x + u(22), y + u(20), u(3), u(10), 0x14325E);
            cv.rect(x + u(41), y + u(20), u(3), u(10), 0x14325E);
            for i in 0..u(26) {
                let t = i - u(13);
                let dy = t * t / u(13).max(1) / 3;
                cv.rect(x + u(19) + i, y + u(44) - dy, 1, u(2).max(1), 0x14325E);
            }
        }
        Icon::Settings => {
            let (cx, cy) = (x + s / 2, y + s / 2);
            for k in 0..12 {
                let a = k * 30;
                let (dx, dy) = (cos_deg(a) * u(20) / 1000, sin_deg(a) * u(20) / 1000);
                cv.circle(cx + dx, cy + dy, u(5).max(2), 0x3A3F48);
            }
            cv.circle(cx, cy, u(20), 0x3A3F48);
            cv.circle(cx, cy, u(14), 0xD9DCE2);
            cv.circle(cx, cy, u(7), 0x3A3F48);
        }
        Icon::Browser => {
            let (cx, cy) = (x + s / 2, y + s / 2);
            cv.circle(cx, cy, u(23), 0xF4F8FF);
            cv.circle(cx, cy, u(21), 0x2F8CF8);
            // a compass needle
            for i in 0..u(18) {
                let w = (u(18) - i) / 3;
                cv.rect(cx - w / 2 + i / 2, cy - i, w.max(1), 1, 0xFF4E4E);
                cv.rect(cx - w / 2 - i / 2, cy + i, w.max(1), 1, white);
            }
        }
        Icon::Activity => {
            let pts = [(10, 40), (20, 40), (26, 22), (34, 48), (42, 30), (48, 36), (56, 36)];
            for w in pts.windows(2) {
                cv.thick_line(x + u(w[0].0), y + u(w[0].1), x + u(w[1].0), y + u(w[1].1), u(3).max(2), 0x5CF08A);
            }
        }
        Icon::Media => {
            let (cx, cy) = (x + s / 2, y + s / 2);
            let petals = [0xFF9F0A, 0xFFD60A, 0x30D158, 0x64D2FF, 0x0A84FF, 0xBF5AF2, 0xFF375F, 0xFF453A];
            for (k, c) in petals.iter().enumerate() {
                let a = k as i32 * 45;
                let (dx, dy) = (cos_deg(a) * u(13) / 1000, sin_deg(a) * u(13) / 1000);
                cv.circle_a(cx + dx, cy + dy, u(10), *c, 200);
            }
        }
        Icon::Buddy => {
            // Bit's face
            cv.round(x + u(14), y + u(18), u(36), u(28), u(6), white);
            cv.rect(x + u(23), y + u(26), u(4), u(7), 0x1B1D22);
            cv.rect(x + u(37), y + u(26), u(4), u(7), 0x1B1D22);
            cv.rect(x + u(29), y + u(37), u(6), u(3), 0x1B1D22);
            for k in 0..4 {
                cv.rect(x + u(17) + k * u(8), y + u(46), u(4), u(7), 0xDDE3EE);
            }
        }
        Icon::Notes => {
            cv.rect(x + u(8), y + u(16), u(48), u(2).max(1), 0xC99A1A);
            for k in 0..4 {
                cv.rect(x + u(12), y + u(26) + k * u(8), u(40 - k * 6), u(2).max(1), 0x8A6A12);
            }
        }
        Icon::Launchpad => {
            let colors = [0xFF453A, 0xFF9F0A, 0x30D158, 0x0A84FF, 0xBF5AF2, 0xFFD60A, 0x64D2FF, 0xFF375F, 0xFFFFFF];
            for k in 0..9 {
                let (cx, cy) = (k % 3, k / 3);
                cv.round(x + u(12) + cx * u(15), y + u(12) + cy * u(15), u(11), u(11), u(3), colors[k as usize]);
            }
        }
        Icon::Snake => {
            for (dx, dy) in [(10, 42), (18, 42), (26, 42), (26, 34), (26, 26), (34, 26), (42, 26), (42, 18)] {
                cv.round(x + u(dx), y + u(dy), u(8), u(8), u(2), 0xEAFCE8);
            }
            cv.rect(x + u(46), y + u(20), u(2).max(1), u(2).max(1), 0x115528);
            cv.circle(x + u(50), y + u(46), u(4), 0xFF5A5A);
        }
        Icon::Pong => {
            cv.round(x + u(10), y + u(16), u(5), u(22), u(2), white);
            cv.round(x + u(49), y + u(26), u(5), u(22), u(2), white);
            for k in 0..6 {
                cv.rect(x + u(31), y + u(8) + k * u(9), u(2).max(1), u(5), 0x666B75);
            }
            cv.circle(x + u(38), y + u(30), u(3), white);
        }
        Icon::Invaders => {
            for (ry, row) in ALIEN.iter().enumerate() {
                for (rx, c) in row.bytes().enumerate() {
                    if c == b'X' {
                        cv.rect(x + u(10) + rx as i32 * u(4), y + u(16) + ry as i32 * u(4), u(4), u(4), 0xB8F7B2);
                    }
                }
            }
        }
        Icon::TicTacToe => {
            for k in 1..3 {
                cv.round(x + u(10) + k * u(14) - u(1), y + u(10), u(3), u(44), u(1), white);
                cv.round(x + u(10), y + u(10) + k * u(14) - u(1), u(44), u(3), u(1), white);
            }
            cv.thick_line(x + u(13), y + u(13), x + u(21), y + u(21), u(3), white);
            cv.thick_line(x + u(21), y + u(13), x + u(13), y + u(21), u(3), white);
            cv.circle(x + u(45), y + u(45), u(5), white);
            cv.circle(x + u(45), y + u(45), u(3), mix(top, bottom, 200));
        }
        Icon::Connect4 => {
            for row in 0..3 {
                for col in 0..3 {
                    let c = match (row * 3 + col) % 4 {
                        0 => 0xFF5252,
                        1 => 0xFFD740,
                        2 => 0x0F2A6B,
                        _ => 0xFF5252,
                    };
                    cv.circle(x + u(18) + col * u(14), y + u(18) + row * u(14), u(6), c);
                }
            }
        }
        Icon::Calculator => {
            cv.round(x + u(10), y + u(9), u(44), u(12), u(3), 0x9DD3A0);
            for row in 0..3 {
                for col in 0..4 {
                    let c = if col == 3 { 0xFF9F0A } else { 0xD9DCE2 };
                    cv.circle(x + u(15) + col * u(11), y + u(30) + row * u(10), u(4), c);
                }
            }
        }
        Icon::Bounce => {
            cv.circle(x + u(32), y + u(26), u(13), white);
            cv.circle_a(x + u(28), y + u(22), u(4), 0xFFE2C0, 200);
            cv.round(x + u(14), y + u(48), u(36), u(4), u(2), 0x8A2E22);
        }
        Icon::Doom => {
            let f = super::gfx::font(u(18), true);
            let w = super::gfx::text_width(&f, "DOOM");
            cv.text(x + s / 2 - w / 2, y + s / 2 - f.height() as i32 / 2, "DOOM", &f, 0xFFD39A);
        }
        Icon::Script => {
            let f = super::gfx::font(u(22), true);
            let w = super::gfx::text_width(&f, "{zs}");
            cv.text(x + s / 2 - w / 2, y + s / 2 - f.height() as i32 / 2, "{zs}", &f, white);
        }
        _ => {}
    }
}

/// cos of an angle in degrees, times 1000.
pub fn cos_deg(d: i32) -> i32 {
    sin_deg(d + 90)
}

/// sin of an angle in degrees, times 1000 (Bhaskara's approximation).
pub fn sin_deg(d: i32) -> i32 {
    let d = d.rem_euclid(360);
    let (d, sign) = if d > 180 { (d - 180, -1) } else { (d, 1) };
    let num = 4 * d * (180 - d);
    let den = 40500 - d * (180 - d);
    sign * num * 1000 / den.max(1)
}

/// The zero logo: a glossy ring with a slash.
pub fn zero_logo(cv: &mut Canvas, cx: i32, cy: i32, r: i32) {
    cv.shadow(cx - r, cy - r + 4, r * 2, r * 2, r, r / 3, 90);
    cv.gradient(cx - r, cy - r, r * 2, r * 2, r, 0x9D8CFF, 0x3B2FA8);
    cv.circle(cx, cy, r * 6 / 10, mix(0x3B2FA8, 0x000000, 60));
    cv.thick_line(cx - r / 3, cy + r / 3, cx + r / 3, cy - r / 3, (r / 6).max(2), 0xFFFFFF);
}
